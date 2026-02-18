//! GPU-accelerated batch forward pass for neural optimizer training.
//!
//! Sequential-individual, parallel-block dispatch: upload ONE individual's
//! weights per dispatch, run all blocks in parallel. All threads read the
//! same 484KB weight buffer from L2 cache. 16 dispatches per generation.
//!
//! The accelerator is created once with a `max_blocks` capacity and reused
//! across files via `upload_blocks()`. GPU init (device, shader, pipeline)
//! happens only once.

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

/// GPU params struct matching the WGSL Params layout (block-parallel).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    num_blocks: u32,
    inv_scale_lo: u32,
    inv_scale_hi: u32,
    half_p_lo: u32,
    half_p_hi: u32,
    _pad0: u32,
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
    num_blocks: u32,
    max_blocks: u32,
    output_size: u64,
    inv_scale_raw: u64,
    half_p: u64,
}

impl NeuralAccelerator {
    /// Create a GPU accelerator sized for up to `max_blocks` blocks.
    /// Initializes the GPU device, compiles shaders, and allocates buffers once.
    /// Use `upload_blocks()` to load a specific file's blocks before `batch_forward()`.
    ///
    /// The old `try_new(blocks, _num_individuals)` API still works for backward
    /// compatibility (tests, single-file build).
    pub fn try_new(blocks: &[TIRBlock], _num_individuals: u32) -> Option<Self> {
        let mut accel = Self::try_create(blocks.len() as u32)?;
        accel.upload_blocks(blocks);
        Some(accel)
    }

    /// Create the accelerator with capacity for `max_blocks`. No blocks loaded yet.
    pub fn try_create(max_blocks: u32) -> Option<Self> {
        let max_blocks = max_blocks.max(1);
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

        // All buffers sized for max_blocks capacity
        let slots_per_block = MAX_NODES * WORDS_PER_NODE;
        let block_buf_size = (max_blocks as u64) * (slots_per_block as u64) * 8;
        let block_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blocks"),
            size: block_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let meta_buf_size = (max_blocks as u64) * 4;
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
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let scratch_size = (max_blocks as u64) * (SCRATCH_PER_THREAD as u64) * 8;
        let scratch_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scratch"),
            size: scratch_size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let weight_size = (WEIGHT_COUNT as u64) * 8;
        let weight_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weights"),
            size: weight_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let output_size = (max_blocks * MAX_OUTPUT as u32) as u64 * 4;
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
            num_blocks: 0,
            max_blocks,
            output_size,
            inv_scale_raw,
            half_p,
        })
    }

    /// Upload a new set of blocks for the next training file.
    /// Reuses existing buffers — only writes data, no allocation.
    pub fn upload_blocks(&mut self, blocks: &[TIRBlock]) {
        let n = (blocks.len() as u32).min(self.max_blocks);
        self.num_blocks = n;

        // Upload block node data
        let block_data = encode_blocks_for_gpu(&blocks[..n as usize]);
        self.queue
            .write_buffer(&self.block_buf, 0, bytemuck::cast_slice(&block_data));

        // Upload block metadata
        let meta_data: Vec<u32> = blocks[..n as usize]
            .iter()
            .map(|b| b.node_count as u32)
            .collect();
        self.queue
            .write_buffer(&self.meta_buf, 0, bytemuck::cast_slice(&meta_data));

        // Update params with new block count
        let params = GpuParams {
            num_blocks: n,
            inv_scale_lo: self.inv_scale_raw as u32,
            inv_scale_hi: (self.inv_scale_raw >> 32) as u32,
            half_p_lo: self.half_p as u32,
            half_p_hi: (self.half_p >> 32) as u32,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        self.queue
            .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

        // Update output size for readback
        self.output_size = (n * MAX_OUTPUT as u32) as u64 * 4;
    }

    /// Run forward pass for ONE individual on all blocks.
    /// Returns `Vec<Vec<u32>>` — one output sequence per block.
    fn forward_one(&self, weights: &[u64]) -> Vec<Vec<u32>> {
        // Upload this individual's weights
        let mut weight_data: Vec<u32> = Vec::with_capacity(weights.len() * 2);
        for &val in weights {
            weight_data.push(val as u32);
            weight_data.push((val >> 32) as u32);
        }
        self.queue
            .write_buffer(&self.weight_buf, 0, bytemuck::cast_slice(&weight_data));

        // Dispatch: ceil(num_blocks / 64) workgroups
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
            pass.set_bind_group(0, &self.bind_group, &[]);
            let workgroups = (self.num_blocks + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.output_buf, 0, &self.staging_buf, 0, self.output_size);
        self.queue.submit(std::iter::once(encoder.finish()));

        // Readback
        let slice = self.staging_buf.slice(..);
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

        let mut blocks_out = Vec::with_capacity(self.num_blocks as usize);
        for b in 0..self.num_blocks {
            let base = (b * MAX_OUTPUT as u32) as usize;
            let codes: Vec<u32> = output_codes[base..base + MAX_OUTPUT]
                .iter()
                .copied()
                .collect();
            blocks_out.push(codes);
        }

        drop(data);
        self.staging_buf.unmap();

        blocks_out
    }

    /// Run forward pass for all individuals (sequential dispatch, parallel blocks).
    /// Returns `[num_individuals][num_blocks][output_codes]`.
    pub fn batch_forward(&self, weight_vecs: &[Vec<u64>]) -> Vec<Vec<Vec<u32>>> {
        let mut results = Vec::with_capacity(weight_vecs.len());
        for wv in weight_vecs {
            results.push(self.forward_one(wv));
        }
        results
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::fixed::Fixed;
    use crate::field::goldilocks::Goldilocks;
    use crate::ir::tir::encode::{CONTEXT_SIZE, MAX_NODES, WORDS_PER_NODE};
    use crate::ir::tir::neural::model::NeuralModel;

    /// Test GPU field arithmetic by running a simple shader that multiplies pairs.
    #[test]
    fn gpu_field_arithmetic_matches_cpu() {
        let (device, queue) = match super::super::try_create_device() {
            Some(dq) => dq,
            None => {
                eprintln!("No GPU available, skipping test");
                return;
            }
        };

        // Shader that reads pairs from input, multiplies them, writes to output
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

        // Test cases: pairs of (a, b) as u64
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

    /// Compare GPU and CPU forward pass outputs for identical weights and blocks.
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

    /// Test with 2 nodes and full weights — minimal case triggering attention.
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
