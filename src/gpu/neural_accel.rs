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
    weight_buf: wgpu::Buffer,
    output_buf: wgpu::Buffer,
    staging_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    num_blocks: u32,
    num_individuals: u32,
    output_size: u64,
    total_passes: u32,
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

        // Pre-allocate weight buffer (updated each generation via write_buffer)
        let weight_size = (num_individuals as u64) * (WEIGHT_COUNT as u64) * 8; // 8 bytes per u64
        let weight_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weights"),
            size: weight_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Pre-allocate output and staging buffers
        let output_size = (total_passes * MAX_OUTPUT as u32) as u64 * 4;
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

        // Pre-build bind group (all buffer references are stable)
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
            num_blocks,
            num_individuals,
            output_size,
            total_passes,
        })
    }

    /// Run batch forward passes for all individuals on all blocks.
    /// `weight_vecs`: one raw u64 weight vector per individual.
    /// Returns `[num_individuals][num_blocks]` where each entry is up to MAX_OUTPUT codes.
    pub fn batch_forward(&self, weight_vecs: &[Vec<u64>]) -> Vec<Vec<Vec<u32>>> {
        // Upload weights to pre-allocated buffer
        let mut weight_data: Vec<u32> =
            Vec::with_capacity(weight_vecs.len() * WEIGHT_COUNT as usize * 2);
        for wv in weight_vecs {
            for &val in wv {
                weight_data.push(val as u32);
                weight_data.push((val >> 32) as u32);
            }
        }
        self.queue
            .write_buffer(&self.weight_buf, 0, bytemuck::cast_slice(&weight_data));

        // Dispatch compute + copy in one submission
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
            let workgroups = (self.total_passes + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
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
        self.staging_buf.unmap();

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
        let shader_src = r#"
const GL_P_LO: u32 = 0x00000001u;
const GL_P_HI: u32 = 0xFFFFFFFFu;

@group(0) @binding(0) var<storage, read> input: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec2<u32>>;

fn mul32(a: u32, b: u32) -> vec2<u32> {
    let a_lo = a & 0xFFFFu;
    let a_hi = a >> 16u;
    let b_lo = b & 0xFFFFu;
    let b_hi = b >> 16u;
    let p0 = a_lo * b_lo;
    let p1 = a_lo * b_hi;
    let p2 = a_hi * b_lo;
    let p3 = a_hi * b_hi;
    let mid = p1 + (p0 >> 16u);
    let mid2 = (mid & 0xFFFFu) + p2;
    let lo = ((mid2 & 0xFFFFu) << 16u) | (p0 & 0xFFFFu);
    let hi = p3 + (mid >> 16u) + (mid2 >> 16u);
    return vec2<u32>(lo, hi);
}

fn gl_reduce(v: vec2<u32>) -> vec2<u32> {
    if v.y > GL_P_HI || (v.y == GL_P_HI && v.x >= GL_P_LO) {
        let lo = v.x - GL_P_LO;
        let borrow = select(0u, 1u, v.x < GL_P_LO);
        let hi = v.y - GL_P_HI - borrow;
        return vec2<u32>(lo, hi);
    }
    return v;
}

fn canon_reduce128(lo: vec2<u32>, hi: vec2<u32>) -> vec2<u32> {
    let m0 = mul32(hi.x, 0xFFFFFFFFu);
    let m1 = mul32(hi.y, 0xFFFFFFFFu);
    var r0 = m0.x;
    var r1 = m0.y;
    var r2 = 0u;
    let t1 = r1 + m1.x;
    let c1 = select(0u, 1u, t1 < r1);
    r1 = t1;
    r2 = m1.y + c1;
    let hs_lo = vec2<u32>(r0, r1);
    let hs_hi = vec2<u32>(r2, 0u);
    let sum_lo_x = lo.x + hs_lo.x;
    let c2 = select(0u, 1u, sum_lo_x < lo.x);
    let sum_lo_y = lo.y + hs_lo.y + c2;
    let c3 = select(0u, 1u, sum_lo_y < lo.y || (c2 == 1u && sum_lo_y == lo.y));
    let sum_hi_x = hs_hi.x + c3;
    let c4 = select(0u, 1u, sum_hi_x < hs_hi.x);
    let sum_hi_y = hs_hi.y + c4;
    let s_lo = vec2<u32>(sum_lo_x, sum_lo_y);
    let s_hi = vec2<u32>(sum_hi_x, sum_hi_y);
    if s_hi.x == 0u && s_hi.y == 0u {
        return gl_reduce(s_lo);
    }
    let m2 = mul32(s_hi.x, 0xFFFFFFFFu);
    let m3 = mul32(s_hi.y, 0xFFFFFFFFu);
    var q0 = m2.x;
    var q1 = m2.y;
    var q2 = 0u;
    let t2 = q1 + m3.x;
    let c5 = select(0u, 1u, t2 < q1);
    q1 = t2;
    q2 = m3.y + c5;
    let ss_lo_x = s_lo.x + q0;
    let c6 = select(0u, 1u, ss_lo_x < s_lo.x);
    let ss_lo_y = s_lo.y + q1 + c6;
    let c7 = select(0u, 1u, ss_lo_y < s_lo.y || (c6 == 1u && ss_lo_y == s_lo.y));
    let rem_hi = q2 + c7;
    var result = vec2<u32>(ss_lo_x, ss_lo_y);
    if rem_hi > 0u {
        let corr = rem_hi * 0xFFFFFFFFu;
        let rc_lo = result.x + corr;
        let rc_carry = select(0u, 1u, rc_lo < result.x);
        let rc_hi = result.y + rc_carry;
        result = vec2<u32>(rc_lo, rc_hi);
    }
    return gl_reduce(result);
}

fn canon_mul(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let ll = mul32(a.x, b.x);
    let lh = mul32(a.x, b.y);
    let hl = mul32(a.y, b.x);
    let hh = mul32(a.y, b.y);
    var r0 = ll.x;
    var r1 = ll.y;
    var r2 = hh.x;
    var r3 = hh.y;
    let t1 = r1 + lh.x;
    let c1 = select(0u, 1u, t1 < r1);
    r1 = t1;
    let t2 = r2 + lh.y + c1;
    let c2 = select(0u, 1u, t2 < r2 || (c1 == 1u && t2 == r2));
    r2 = t2;
    r3 = r3 + c2;
    let t3 = r1 + hl.x;
    let c3 = select(0u, 1u, t3 < r1);
    r1 = t3;
    let t4 = r2 + hl.y + c3;
    let c4 = select(0u, 1u, t4 < r2 || (c3 == 1u && t4 == r2));
    r2 = t4;
    r3 = r3 + c4;
    return canon_reduce128(vec2<u32>(r0, r1), vec2<u32>(r2, r3));
}

fn gl_add(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let lo = a.x + b.x;
    let carry_lo = select(0u, 1u, lo < a.x);
    let hi = a.y + b.y + carry_lo;
    let carry_hi = select(0u, 1u, hi < a.y || (carry_lo == 1u && hi == a.y));
    var r = vec2<u32>(lo, hi);
    if carry_hi == 1u || hi > GL_P_HI || (hi == GL_P_HI && lo >= GL_P_LO) {
        let sub_lo = r.x - GL_P_LO;
        let borrow = select(0u, 1u, r.x < GL_P_LO);
        let sub_hi = r.y - GL_P_HI - borrow;
        r = vec2<u32>(sub_lo, sub_hi);
    }
    return r;
}

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let a = input[idx * 2u];
    let b = input[idx * 2u + 1u];
    // Test 0-3: canon_mul(a, b)
    // Test 4-7: gl_add(a, b)
    // Test 8-11: fused dot pattern: gl_add(canon_mul(a,b), canon_mul(a,b)) then * inv_scale
    output[idx] = canon_mul(a, b);
}
"#;

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
            (3, 7),                                     // small
            (65536, 65536),                             // SCALE * SCALE
            (1000, 2000),                               // moderate
            (0xFFFFFFFF_00000000, 2),                   // near p
            (0xDEADBEEF_12345678, 0xCAFEBABE_87654321), // large
            (1, 0xFFFFFFFF_00000000),                   // 1 * (p-1)
            (0, 42),                                    // zero
            (65536, 0xFFFFFFFF_0000FFFF),               // SCALE * -SCALE (neg value)
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
            } else {
                eprintln!("  OK test {}: {} * {} = {}", i, a, b, cpu_val);
            }
        }

        drop(data);
        staging_buf.unmap();

        assert!(all_pass, "GPU field arithmetic does not match CPU");
    }

    /// Compare GPU and CPU forward pass outputs for identical weights and blocks.
    #[test]
    fn gpu_matches_cpu_forward() {
        // Create a model with small deterministic weights
        let weight_count = NeuralModel::zeros().weight_count();
        let weights: Vec<Fixed> = (0..weight_count)
            .map(|i| Fixed::from_f64(0.001 * ((i % 97) as f64 - 48.0)))
            .collect();

        let mut cpu_model = NeuralModel::from_weight_vec(&weights);

        // Create a test block with some nonzero data
        let mut nodes = [0u64; MAX_NODES * WORDS_PER_NODE];
        for i in 0..12 {
            // 3 nodes * 4 words
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

        // CPU forward pass
        let cpu_output = cpu_model.forward(&block);

        // GPU forward pass
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

    /// Build a custom GPU pipeline with a patched shader and run it.
    /// The patched shader outputs embedding raw values instead of instruction codes.
    #[allow(dead_code)]
    fn run_debug_shader(
        blocks: &[TIRBlock],
        weight_vecs: &[Vec<u64>],
        shader_src: &str,
        output_count: usize,
    ) -> Vec<u32> {
        let (device, queue) = super::super::try_create_device().unwrap();
        let num_individuals = weight_vecs.len() as u32;
        let num_blocks = blocks.len() as u32;
        let total_passes = num_individuals * num_blocks;

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("debug_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader_module,
            entry_point: Some("neural_forward"),
            compilation_options: Default::default(),
            cache: None,
        });

        let block_data = encode_blocks_for_gpu(blocks);
        let block_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blocks"),
            contents: bytemuck::cast_slice(&block_data),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let meta_data: Vec<u32> = blocks.iter().map(|b| b.node_count as u32).collect();
        let meta_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("meta"),
            contents: bytemuck::cast_slice(&meta_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let inv_scale_raw = Goldilocks::from_u64(crate::field::fixed::SCALE)
            .inv()
            .unwrap()
            .to_u64();
        let half_p = (crate::field::goldilocks::MODULUS - 1) / 2;
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

        let scratch_size = (total_passes as u64) * (SCRATCH_PER_THREAD as u64) * 8;
        let scratch_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scratch"),
            size: scratch_size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let mut weight_data: Vec<u32> = Vec::new();
        for wv in weight_vecs {
            for &val in wv {
                weight_data.push(val as u32);
                weight_data.push((val >> 32) as u32);
            }
        }
        let weight_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("weights"),
            contents: bytemuck::cast_slice(&weight_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let output_size = (output_count * 4) as u64;
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
            label: None,
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

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let wg = (total_passes + 63) / 64;
            pass.dispatch_workgroups(wg, 1, 1);
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
        let results: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging_buf.unmap();
        results
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
        // 2 nodes
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
