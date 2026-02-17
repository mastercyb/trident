//! GPU-accelerated batch forward pass for neural optimizer training.
//!
//! Runs all (individual × block) forward passes in a single GPU dispatch.
//! Each GPU thread executes one complete forward pass independently.

use crate::field::fixed::SCALE;
use crate::field::goldilocks::{Goldilocks, MODULUS};
use crate::field::PrimeField;
use crate::ir::tir::encode::{TIRBlock, MAX_NODES, WORDS_PER_NODE};
use crate::ir::tir::neural::model::{MAX_OUTPUT, PARAM_COUNT};

use super::shaders;

const WORKGROUP_SIZE: u32 = 64;
/// Total flat weight count including input_proj.
const WEIGHT_COUNT: u32 =
    (PARAM_COUNT + WORDS_PER_NODE * crate::ir::tir::neural::model::DIM) as u32;
/// Per-thread scratch size in vec2<u32> units (must match shader SCRATCH_PER_THREAD).
const SCRATCH_PER_THREAD: u32 = 11264;

/// GPU params struct matching the WGSL Params layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    num_individuals: u32,
    num_blocks: u32,
    inv_scale_lo: u32,
    inv_scale_hi: u32,
    half_p_lo: u32,
    half_p_hi: u32,
    _pad0: u32,
    _pad1: u32,
}

pub struct NeuralAccelerator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    block_buf: wgpu::Buffer,
    meta_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    scratch_buf: wgpu::Buffer,
    num_blocks: u32,
    num_individuals: u32,
}

impl NeuralAccelerator {
    /// Create a GPU accelerator and upload blocks. Returns None if no GPU available.
    pub fn try_new(blocks: &[TIRBlock], num_individuals: u32) -> Option<Self> {
        let (device, queue) = super::try_create_device()?;

        // Compile shader
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("neural_forward"),
            source: wgpu::ShaderSource::Wgsl(shaders::NEURAL_SHADER.into()),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("neural_forward_pipeline"),
            layout: None,
            module: &shader_module,
            entry_point: Some("neural_forward"),
            compilation_options: Default::default(),
            cache: None,
        });

        let num_blocks = blocks.len() as u32;

        // Upload blocks: each block's nodes as vec2<u32> (lo, hi) pairs
        let block_data = encode_blocks_for_gpu(blocks);
        let block_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blocks"),
            contents: bytemuck::cast_slice(&block_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Upload block metadata (node counts)
        let meta_data: Vec<u32> = blocks.iter().map(|b| b.node_count as u32).collect();
        let meta_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("block_meta"),
            contents: bytemuck::cast_slice(&meta_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Compute fixed-point constants
        let inv_scale_raw = Goldilocks::from_u64(SCALE)
            .inv()
            .expect("SCALE is nonzero")
            .to_u64();
        let half_p = (MODULUS - 1) / 2;

        let params = GpuParams {
            num_individuals,
            num_blocks,
            inv_scale_lo: inv_scale_raw as u32,
            inv_scale_hi: (inv_scale_raw >> 32) as u32,
            half_p_lo: half_p as u32,
            half_p_hi: (half_p >> 32) as u32,
            _pad0: 0,
            _pad1: 0,
        };

        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // Scratch buffer: per-thread working memory in global GPU memory
        let total_passes = num_individuals * num_blocks;
        let scratch_size = (total_passes as u64) * (SCRATCH_PER_THREAD as u64) * 8; // 8 bytes per vec2<u32>
        let scratch_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scratch"),
            size: scratch_size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        Some(Self {
            device,
            queue,
            pipeline,
            block_buf,
            meta_buf,
            params_buf,
            scratch_buf,
            num_blocks,
            num_individuals,
        })
    }

    /// Run batch forward passes for all individuals on all blocks.
    /// `weight_vecs`: one raw u64 weight vector per individual.
    /// Returns `[num_individuals][num_blocks]` where each entry is up to MAX_OUTPUT codes.
    pub fn batch_forward(&self, weight_vecs: &[Vec<u64>]) -> Vec<Vec<Vec<u32>>> {
        let total_passes = self.num_individuals * self.num_blocks;

        // Encode weights as vec2<u32> pairs (lo, hi for each u64)
        let mut weight_data: Vec<u32> =
            Vec::with_capacity(weight_vecs.len() * WEIGHT_COUNT as usize * 2);
        for wv in weight_vecs {
            for &val in wv {
                weight_data.push(val as u32);
                weight_data.push((val >> 32) as u32);
            }
        }

        let weight_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weights"),
                contents: bytemuck::cast_slice(&weight_data),
                usage: wgpu::BufferUsages::STORAGE,
            });

        // Output buffer
        let output_size = (total_passes * MAX_OUTPUT as u32) as u64 * 4;
        let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outputs"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Staging buffer for readback
        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Bind group
        let bind_group_layout = self.pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("neural_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: weight_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.block_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.meta_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.scratch_buf.as_entire_binding(),
                },
            ],
        });

        // Dispatch
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("neural_encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("neural_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let workgroups = (total_passes + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            pass.dispatch_workgroups(workgroups, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);
        self.queue.submit(std::iter::once(encoder.finish()));

        // Readback
        let slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .expect("GPU readback channel closed")
            .expect("GPU readback failed");

        let data = slice.get_mapped_range();
        let output_codes: &[u32] = bytemuck::cast_slice(&data);

        // Reshape into [individuals][blocks][codes]
        let mut result = Vec::with_capacity(self.num_individuals as usize);
        for i in 0..self.num_individuals {
            let mut blocks_out = Vec::with_capacity(self.num_blocks as usize);
            for b in 0..self.num_blocks {
                let pass_id = i * self.num_blocks + b;
                let base = (pass_id * MAX_OUTPUT as u32) as usize;
                let codes: Vec<u32> = output_codes[base..base + MAX_OUTPUT]
                    .iter()
                    .copied()
                    .collect();
                blocks_out.push(codes);
            }
            result.push(blocks_out);
        }

        drop(data);
        staging_buf.unmap();

        result
    }
}

/// Encode TIR blocks as flat u32 pairs for GPU upload.
/// Each block occupies MAX_NODES * WORDS_PER_NODE slots (128 vec2<u32> entries).
fn encode_blocks_for_gpu(blocks: &[TIRBlock]) -> Vec<u32> {
    let slots_per_block = MAX_NODES * WORDS_PER_NODE;
    let mut data = Vec::with_capacity(blocks.len() * slots_per_block * 2);
    for block in blocks {
        for i in 0..slots_per_block {
            let val = block.nodes[i];
            data.push(val as u32);
            data.push((val >> 32) as u32);
        }
    }
    data
}

use wgpu::util::DeviceExt;
