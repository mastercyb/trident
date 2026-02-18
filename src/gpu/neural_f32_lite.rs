//! Lite float32 GPU-accelerated neural forward pass.
//!
//! MLP-only model with 10,400 parameters. ~1KB private memory per GPU thread
//! vs 17KB for the full transformer model. Higher GPU occupancy.

use crate::field::fixed::Fixed;
use crate::field::goldilocks::Goldilocks;
use crate::field::PrimeField;
use crate::ir::tir::encode::{TIRBlock, MAX_NODES, WORDS_PER_NODE};
use crate::ir::tir::neural::model::MAX_OUTPUT;

use super::shaders;

const WORKGROUP_SIZE: u32 = 64;
const LITE_WEIGHT_COUNT: u32 = 10_400;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct F32Params {
    num_blocks: u32,
    num_individuals: u32,
    _pad0: u32,
    _pad1: u32,
}

pub struct F32LiteAccelerator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    block_buf: wgpu::Buffer,
    meta_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    weight_buf: wgpu::Buffer,
    output_buf: wgpu::Buffer,
    staging_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    all_block_data: Vec<f32>,
    all_meta_data: Vec<u32>,
    num_blocks_total: u32,
    blocks_per_chunk: u32,
    max_individuals: u32,
}

impl F32LiteAccelerator {
    pub fn try_create(max_blocks: u32, max_individuals: u32) -> Option<Self> {
        let max_blocks = max_blocks.max(1);
        let max_individuals = max_individuals.max(1);
        let (device, queue) = super::try_create_device()?;

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("neural_f32_lite"),
            source: wgpu::ShaderSource::Wgsl(shaders::NEURAL_F32_LITE.into()),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("neural_f32_lite_pipeline"),
            layout: None,
            module: &shader_module,
            entry_point: Some("neural_f32_lite"),
            compilation_options: Default::default(),
            cache: None,
        });

        let limits = device.limits();
        let max_buf = limits
            .max_buffer_size
            .min(limits.max_storage_buffer_binding_size as u64);

        let output_bytes_per_block = max_individuals as u64 * MAX_OUTPUT as u64 * 4;
        let blocks_by_output = (max_buf / output_bytes_per_block).max(1) as u32;
        let blocks_per_chunk = max_blocks.min(blocks_by_output);

        let slots_per_block = (MAX_NODES * WORDS_PER_NODE) as u32;
        let block_buf_size = blocks_per_chunk as u64 * slots_per_block as u64 * 4;
        let block_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lite_blocks"),
            size: block_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let meta_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lite_meta"),
            size: blocks_per_chunk as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params = F32Params {
            num_blocks: 0,
            num_individuals: 0,
            _pad0: 0,
            _pad1: 0,
        };
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lite_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let weight_buf_size = max_individuals as u64 * LITE_WEIGHT_COUNT as u64 * 4;
        let weight_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lite_weights"),
            size: weight_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let output_buf_size =
            max_individuals as u64 * blocks_per_chunk as u64 * MAX_OUTPUT as u64 * 4;
        let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lite_outputs"),
            size: output_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lite_staging"),
            size: output_buf_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lite_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: weight_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: block_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: meta_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        Some(Self {
            device,
            queue,
            pipeline,
            block_buf,
            meta_buf,
            params_buf,
            weight_buf,
            output_buf,
            staging_buf,
            bind_group,
            all_block_data: Vec::new(),
            all_meta_data: Vec::new(),
            num_blocks_total: 0,
            blocks_per_chunk,
            max_individuals,
        })
    }

    pub fn upload_blocks(&mut self, blocks: &[TIRBlock]) {
        self.num_blocks_total = blocks.len() as u32;
        self.all_meta_data = blocks.iter().map(|b| b.node_count as u32).collect();

        let slots_per_block = MAX_NODES * WORDS_PER_NODE;
        let mut data = Vec::with_capacity(blocks.len() * slots_per_block);
        for block in blocks {
            for i in 0..slots_per_block {
                let raw = block.nodes[i];
                data.push(Fixed::from_raw(Goldilocks::from_u64(raw)).to_f64() as f32);
            }
        }
        self.all_block_data = data;
    }

    pub fn batch_forward(&self, weight_vecs: &[Vec<Fixed>]) -> Vec<Vec<Vec<u32>>> {
        let num_ind = (weight_vecs.len() as u32).min(self.max_individuals);
        if num_ind == 0 || self.num_blocks_total == 0 {
            return vec![];
        }

        let mut weight_data: Vec<f32> =
            Vec::with_capacity(num_ind as usize * LITE_WEIGHT_COUNT as usize);
        for wv in &weight_vecs[..num_ind as usize] {
            for w in wv {
                weight_data.push(w.to_f64() as f32);
            }
            for _ in wv.len()..LITE_WEIGHT_COUNT as usize {
                weight_data.push(0.0);
            }
        }
        self.queue
            .write_buffer(&self.weight_buf, 0, bytemuck::cast_slice(&weight_data));

        let slots_per_block = MAX_NODES * WORDS_PER_NODE;
        let nb = self.num_blocks_total as usize;
        let chunk = self.blocks_per_chunk as usize;

        let mut results: Vec<Vec<Vec<u32>>> = (0..num_ind as usize)
            .map(|_| Vec::with_capacity(nb))
            .collect();

        let mut block_offset = 0usize;
        while block_offset < nb {
            let chunk_size = chunk.min(nb - block_offset);
            let chunk_blocks = chunk_size as u32;

            let data_start = block_offset * slots_per_block;
            let data_end = (block_offset + chunk_size) * slots_per_block;
            self.queue.write_buffer(
                &self.block_buf,
                0,
                bytemuck::cast_slice(&self.all_block_data[data_start..data_end]),
            );

            self.queue.write_buffer(
                &self.meta_buf,
                0,
                bytemuck::cast_slice(&self.all_meta_data[block_offset..block_offset + chunk_size]),
            );

            let params = F32Params {
                num_blocks: chunk_blocks,
                num_individuals: num_ind,
                _pad0: 0,
                _pad1: 0,
            };
            self.queue
                .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

            let workgroups_x = (chunk_blocks + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            let output_size = num_ind as u64 * chunk_blocks as u64 * MAX_OUTPUT as u64 * 4;

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("lite_encoder"),
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("lite_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.dispatch_workgroups(workgroups_x, num_ind, 1);
            }
            encoder.copy_buffer_to_buffer(&self.output_buf, 0, &self.staging_buf, 0, output_size);
            self.queue.submit(std::iter::once(encoder.finish()));

            let slice = self.staging_buf.slice(..output_size);
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

            let cb = chunk_size;
            for i in 0..num_ind as usize {
                for b in 0..cb {
                    let base = (i * cb + b) * MAX_OUTPUT;
                    results[i].push(output_codes[base..base + MAX_OUTPUT].to_vec());
                }
            }

            drop(data);
            self.staging_buf.unmap();

            block_offset += chunk_size;
        }

        results
    }
}

use wgpu::util::DeviceExt;
