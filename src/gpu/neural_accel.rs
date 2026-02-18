//! GPU-accelerated batch forward pass for neural optimizer training.
//!
//! Batched dispatch: ALL individuals × a CHUNK of blocks per dispatch.
//! 2D workgroup: gid.x = block index (within chunk), gid.y = individual index.
//! Weights uploaded once per generation. Blocks dispatched in chunks that fit
//! within the device's max buffer size limit.
//!
//! The accelerator is created once with `try_create(max_blocks, max_individuals)`
//! and reused across files via `upload_blocks()`.

use crate::field::fixed::SCALE;
use crate::field::goldilocks::{Goldilocks, MODULUS};
use crate::field::PrimeField;
use crate::ir::tir::encode::{TIRBlock, MAX_NODES, WORDS_PER_NODE};
use crate::ir::tir::neural::model::{MAX_OUTPUT, PARAM_COUNT};

use super::shaders;

const WORKGROUP_SIZE: u32 = 64;
/// Total flat weight count per individual including input_proj.
const WEIGHT_COUNT: u32 =
    (PARAM_COUNT + WORDS_PER_NODE * crate::ir::tir::neural::model::DIM) as u32;
/// Per-thread scratch size in vec2<u32> units (must match shader SCRATCH_PER_THREAD).
const SCRATCH_PER_THREAD: u32 = 11264;

/// GPU params struct matching the WGSL Params layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    num_blocks: u32,
    inv_scale_lo: u32,
    inv_scale_hi: u32,
    half_p_lo: u32,
    half_p_hi: u32,
    num_individuals: u32,
    _pad1: u32,
    _pad2: u32,
}

#[allow(dead_code)] // Buffers held alive for GPU bind group
pub struct NeuralAccelerator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    block_buf: wgpu::Buffer,
    meta_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    scratch_buf: wgpu::Buffer,
    weight_buf: wgpu::Buffer,
    output_buf: wgpu::Buffer,
    staging_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Current file's full block set (uploaded via upload_blocks).
    all_blocks_data: Vec<u32>,
    all_meta_data: Vec<u32>,
    num_blocks_total: u32,
    blocks_per_chunk: u32,
    max_individuals: u32,
    inv_scale_raw: u64,
    half_p: u64,
}

impl NeuralAccelerator {
    /// Backward-compatible constructor: create accelerator and upload blocks.
    pub fn try_new(blocks: &[TIRBlock], num_individuals: u32) -> Option<Self> {
        let mut accel = Self::try_create(blocks.len() as u32, num_individuals.max(1))?;
        accel.upload_blocks(blocks);
        Some(accel)
    }

    /// Create the accelerator with capacity for `max_blocks` × `max_individuals`.
    /// Buffers are sized to fit within device limits by chunking blocks.
    pub fn try_create(max_blocks: u32, max_individuals: u32) -> Option<Self> {
        let max_blocks = max_blocks.max(1);
        let max_individuals = max_individuals.max(1);
        let (device, queue) = super::try_create_device()?;

        let shader_src = shaders::neural_shader();
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("neural_forward"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("neural_forward_pipeline"),
            layout: None,
            module: &shader_module,
            entry_point: Some("neural_forward"),
            compilation_options: Default::default(),
            cache: None,
        });

        let inv_scale_raw = Goldilocks::from_u64(SCALE)
            .inv()
            .expect("SCALE is nonzero")
            .to_u64();
        let half_p = (MODULUS - 1) / 2;

        // Compute blocks_per_chunk from device limits.
        // Scratch is the largest buffer: individuals * blocks * SCRATCH_PER_THREAD * 8 bytes.
        // Must respect BOTH max_buffer_size AND max_storage_buffer_binding_size.
        let limits = device.limits();
        let max_buf = limits
            .max_buffer_size
            .min(limits.max_storage_buffer_binding_size as u64);
        let bytes_per_block_per_ind = SCRATCH_PER_THREAD as u64 * 8;
        let blocks_per_chunk = (max_buf / (max_individuals as u64 * bytes_per_block_per_ind))
            .min(max_blocks as u64)
            .max(1) as u32;

        // Block data buffer: sized for one chunk
        let slots_per_block = MAX_NODES * WORDS_PER_NODE;
        let block_buf_size = (blocks_per_chunk as u64) * (slots_per_block as u64) * 8;
        let block_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blocks"),
            size: block_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let meta_buf_size = (blocks_per_chunk as u64) * 4;
        let meta_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("block_meta"),
            size: meta_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params = GpuParams {
            num_blocks: 0,
            inv_scale_lo: inv_scale_raw as u32,
            inv_scale_hi: (inv_scale_raw >> 32) as u32,
            half_p_lo: half_p as u32,
            half_p_hi: (half_p >> 32) as u32,
            num_individuals: 0,
            _pad1: 0,
            _pad2: 0,
        };
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Scratch: max_individuals * blocks_per_chunk * SCRATCH_PER_THREAD * 8
        let scratch_size =
            (max_individuals as u64) * (blocks_per_chunk as u64) * (SCRATCH_PER_THREAD as u64) * 8;
        let scratch_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scratch"),
            size: scratch_size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        // Weights: max_individuals * WEIGHT_COUNT * 8
        let weight_size = (max_individuals as u64) * (WEIGHT_COUNT as u64) * 8;
        let weight_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weights"),
            size: weight_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Output: max_individuals * blocks_per_chunk * MAX_OUTPUT * 4
        let output_size =
            (max_individuals as u64) * (blocks_per_chunk as u64) * (MAX_OUTPUT as u64) * 4;
        let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outputs"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("neural_bind_group"),
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
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: scratch_buf.as_entire_binding(),
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
            scratch_buf,
            weight_buf,
            output_buf,
            staging_buf,
            bind_group,
            all_blocks_data: Vec::new(),
            all_meta_data: Vec::new(),
            num_blocks_total: 0,
            blocks_per_chunk,
            max_individuals,
            inv_scale_raw,
            half_p,
        })
    }

    /// Upload a new set of blocks for the next training file.
    /// Stores the encoded data; actual GPU upload happens per-chunk in batch_forward.
    pub fn upload_blocks(&mut self, blocks: &[TIRBlock]) {
        self.num_blocks_total = blocks.len() as u32;
        self.all_blocks_data = encode_blocks_for_gpu(blocks);
        self.all_meta_data = blocks.iter().map(|b| b.node_count as u32).collect();
    }

    /// Run forward pass for ALL individuals on all blocks.
    /// Blocks are dispatched in chunks that fit within GPU buffer limits.
    /// Returns `[num_individuals][num_blocks_total][output_codes]`.
    pub fn batch_forward(&self, weight_vecs: &[Vec<u64>]) -> Vec<Vec<Vec<u32>>> {
        let num_ind = (weight_vecs.len() as u32).min(self.max_individuals);
        if num_ind == 0 || self.num_blocks_total == 0 {
            return vec![];
        }

        // Upload ALL individuals' weights once
        let mut weight_data: Vec<u32> =
            Vec::with_capacity(num_ind as usize * WEIGHT_COUNT as usize * 2);
        for wv in &weight_vecs[..num_ind as usize] {
            for &val in wv {
                weight_data.push(val as u32);
                weight_data.push((val >> 32) as u32);
            }
            // Pad if individual has fewer weights than expected
            for _ in wv.len()..WEIGHT_COUNT as usize {
                weight_data.push(0);
                weight_data.push(0);
            }
        }
        self.queue
            .write_buffer(&self.weight_buf, 0, bytemuck::cast_slice(&weight_data));

        // Process blocks in chunks
        let slots_per_block = MAX_NODES * WORDS_PER_NODE;
        let nb = self.num_blocks_total as usize;
        let chunk = self.blocks_per_chunk as usize;

        // Preallocate result: [individuals][blocks]
        let mut results: Vec<Vec<Vec<u32>>> = (0..num_ind as usize)
            .map(|_| Vec::with_capacity(nb))
            .collect();

        let mut block_offset = 0usize;
        while block_offset < nb {
            let chunk_size = chunk.min(nb - block_offset);
            let chunk_blocks = chunk_size as u32;

            // Upload this chunk's block data
            let data_start = block_offset * slots_per_block * 2;
            let data_end = (block_offset + chunk_size) * slots_per_block * 2;
            self.queue.write_buffer(
                &self.block_buf,
                0,
                bytemuck::cast_slice(&self.all_blocks_data[data_start..data_end]),
            );

            // Upload this chunk's metadata
            self.queue.write_buffer(
                &self.meta_buf,
                0,
                bytemuck::cast_slice(&self.all_meta_data[block_offset..block_offset + chunk_size]),
            );

            // Update params
            let params = GpuParams {
                num_blocks: chunk_blocks,
                inv_scale_lo: self.inv_scale_raw as u32,
                inv_scale_hi: (self.inv_scale_raw >> 32) as u32,
                half_p_lo: self.half_p as u32,
                half_p_hi: (self.half_p >> 32) as u32,
                num_individuals: num_ind,
                _pad1: 0,
                _pad2: 0,
            };
            self.queue
                .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

            // Dispatch
            let workgroups_x = (chunk_blocks + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            let output_size = (num_ind as u64) * (chunk_blocks as u64) * (MAX_OUTPUT as u64) * 4;

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("neural_chunk_encoder"),
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("neural_chunk_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.dispatch_workgroups(workgroups_x, num_ind, 1);
            }
            encoder.copy_buffer_to_buffer(&self.output_buf, 0, &self.staging_buf, 0, output_size);
            self.queue.submit(std::iter::once(encoder.finish()));

            // Readback
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

            // Parse chunk results into per-individual block outputs
            let cb = chunk_size;
            for i in 0..num_ind as usize {
                for b in 0..cb {
                    let base = (i * cb + b) * MAX_OUTPUT;
                    let codes: Vec<u32> = output_codes[base..base + MAX_OUTPUT]
                        .iter()
                        .copied()
                        .collect();
                    results[i].push(codes);
                }
            }

            drop(data);
            self.staging_buf.unmap();

            block_offset += chunk_size;
        }

        results
    }
}

/// Encode TIR blocks as flat u32 pairs for GPU upload.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::fixed::Fixed;
    use crate::field::goldilocks::Goldilocks;
    use crate::ir::tir::encode::{CONTEXT_SIZE, MAX_NODES, WORDS_PER_NODE};
    use crate::ir::tir::neural::model::NeuralModel;

    #[test]
    fn gpu_field_arithmetic_matches_cpu() {
        let (device, queue) = match super::super::try_create_device() {
            Some(dq) => dq,
            None => {
                eprintln!("No GPU available, skipping test");
                return;
            }
        };

        let shader_src = format!(
            "{}\n{}",
            crate::gpu::shaders::GOLDILOCKS,
            r#"
@group(0) @binding(0) var<storage, read> input: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec2<u32>>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let a = input[idx * 2u];
    let b = input[idx * 2u + 1u];
    output[idx] = canon_mul(a, b);
}
"#
        );

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("arith_test"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("arith_test_pipeline"),
            layout: None,
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let test_pairs: Vec<(u64, u64)> = vec![
            (3, 7),
            (65536, 65536),
            (1000, 2000),
            (0xFFFFFFFF_00000000, 2),
            (0xDEADBEEF_12345678, 0xCAFEBABE_87654321),
            (1, 0xFFFFFFFF_00000000),
            (0, 42),
            (65536, 0xFFFFFFFF_0000FFFF),
        ];

        let mut input_data: Vec<u32> = Vec::new();
        for (a, b) in &test_pairs {
            input_data.push(*a as u32);
            input_data.push((*a >> 32) as u32);
            input_data.push(*b as u32);
            input_data.push((*b >> 32) as u32);
        }

        let input_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("input"),
            contents: bytemuck::cast_slice(&input_data),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output_size = (test_pairs.len() * 8) as u64;
        let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(test_pairs.len() as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();

        let data = slice.get_mapped_range();
        let results: &[u32] = bytemuck::cast_slice(&data);

        let mut all_pass = true;
        for (i, (a, b)) in test_pairs.iter().enumerate() {
            let gpu_lo = results[i * 2];
            let gpu_hi = results[i * 2 + 1];
            let gpu_val = (gpu_lo as u64) | ((gpu_hi as u64) << 32);

            let cpu_val = Goldilocks::from_u64(*a)
                .mul(Goldilocks::from_u64(*b))
                .to_u64();

            if gpu_val != cpu_val {
                eprintln!(
                    "MISMATCH test {}: {} * {} = GPU:{} vs CPU:{}",
                    i, a, b, gpu_val, cpu_val
                );
                all_pass = false;
            }
        }

        drop(data);
        staging_buf.unmap();

        assert!(all_pass, "GPU field arithmetic does not match CPU");
    }

    #[test]
    fn gpu_matches_cpu_forward() {
        let weight_count = NeuralModel::zeros().weight_count();
        let weights: Vec<Fixed> = (0..weight_count)
            .map(|i| Fixed::from_f64(0.001 * ((i % 97) as f64 - 48.0)))
            .collect();

        let mut cpu_model = NeuralModel::from_weight_vec(&weights);

        let mut nodes = [0u64; MAX_NODES * WORDS_PER_NODE];
        for i in 0..12 {
            nodes[i] = (i as u64 + 1) * 1000;
        }
        let block = TIRBlock {
            nodes,
            context: [0u64; CONTEXT_SIZE],
            node_count: 3,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 3,
        };

        let cpu_output = cpu_model.forward(&block);

        let accel = match NeuralAccelerator::try_new(&[block.clone()], 1) {
            Some(a) => a,
            None => {
                eprintln!("No GPU available, skipping test");
                return;
            }
        };

        let raw_weights: Vec<u64> = weights.iter().map(|w| w.raw().to_u64()).collect();
        let gpu_results = accel.batch_forward(&[raw_weights]);
        let gpu_codes: Vec<u64> = gpu_results[0][0]
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u64)
            .collect();

        eprintln!("CPU output ({} codes): {:?}", cpu_output.len(), cpu_output);
        eprintln!("GPU output ({} codes): {:?}", gpu_codes.len(), gpu_codes);

        assert_eq!(
            cpu_output, gpu_codes,
            "GPU forward pass produces different output than CPU"
        );
    }

    #[test]
    fn gpu_two_nodes_full_weights() {
        let weight_count = NeuralModel::zeros().weight_count();
        let weights: Vec<Fixed> = (0..weight_count)
            .map(|i| Fixed::from_f64(0.001 * ((i % 97) as f64 - 48.0)))
            .collect();

        let mut cpu_model = NeuralModel::from_weight_vec(&weights);

        let mut nodes = [0u64; MAX_NODES * WORDS_PER_NODE];
        for i in 0..8 {
            nodes[i] = (i as u64 + 1) * 100;
        }
        let block = TIRBlock {
            nodes,
            context: [0u64; CONTEXT_SIZE],
            node_count: 2,
            fn_name: "test".into(),
            start_idx: 0,
            end_idx: 2,
        };

        let cpu_output = cpu_model.forward(&block);

        let accel = match NeuralAccelerator::try_new(&[block.clone()], 1) {
            Some(a) => a,
            None => {
                eprintln!("No GPU, skipping");
                return;
            }
        };

        let raw_weights: Vec<u64> = weights.iter().map(|w| w.raw().to_u64()).collect();
        let gpu_results = accel.batch_forward(&[raw_weights]);
        let gpu_codes: Vec<u64> = gpu_results[0][0]
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u64)
            .collect();

        eprintln!("2-node full test:");
        eprintln!("  CPU: {:?}", cpu_output);
        eprintln!("  GPU: {:?}", gpu_codes);

        assert_eq!(cpu_output, gpu_codes, "2-node full: GPU != CPU");
    }
}
