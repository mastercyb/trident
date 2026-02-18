// Neural forward pass — batched dispatch.
//
// 2D dispatch: gid.x = block index, gid.y = individual index.
// All individuals × all blocks run in a SINGLE dispatch.
// Weights are laid out as [num_individuals][WEIGHT_COUNT] in a flat buffer.
// Scratch is [num_individuals * num_blocks][SCRATCH_PER_THREAD].
// Outputs are [num_individuals][num_blocks][MAX_OUTPUT].
//
// Dispatch: (ceil(num_blocks / 64), num_individuals, 1)
//
// Requires goldilocks.wgsl and fixed_point.wgsl to be prepended.

// ============================================================
// Model constants
// ============================================================

const DIM: u32 = 64u;
const HEADS: u32 = 2u;
const HEAD_DIM: u32 = 32u;
const LAYERS: u32 = 2u;
const FFN_HIDDEN: u32 = 64u;
const MAX_OUTPUT: u32 = 16u;
const WORDS_PER_NODE: u32 = 4u;
const MAX_NODES: u32 = 32u;

const QKV_SIZE: u32 = 12288u;
const OUT_PROJ_SIZE: u32 = 4096u;
const FFN1_SIZE: u32 = 4096u;
const FFN2_SIZE: u32 = 4096u;
const LN_SIZE: u32 = 64u;
const LAYER_SIZE: u32 = 24704u;
const INPUT_PROJ_SIZE: u32 = 256u;

const DEC_HIDDEN_SIZE: u32 = 8192u;
const DEC_HBIAS_SIZE: u32 = 64u;
const DEC_OUTPUT_SIZE: u32 = 4096u;
const DEC_OBIAS_SIZE: u32 = 64u;

const WEIGHT_COUNT: u32 = 62080u;

// Scratch layout per thread (in vec2<u32> units):
const SCRATCH_PER_THREAD: u32 = 11264u;
const S_EMB: u32 = 0u;
const S_ATTN: u32 = 2048u;
const S_PROJ: u32 = 4096u;
const S_FFN_OUT: u32 = 6144u;
const S_Q: u32 = 8192u;
const S_K: u32 = 9216u;
const S_V: u32 = 10240u;

// ============================================================
// Buffers — all individuals' weights in one buffer
// ============================================================

struct Params {
    num_blocks: u32,
    inv_scale_lo: u32,
    inv_scale_hi: u32,
    half_p_lo: u32,
    half_p_hi: u32,
    num_individuals: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read>       weights: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read>       blocks: array<vec2<u32>>;
@group(0) @binding(2) var<storage, read>       block_meta: array<u32>;
@group(0) @binding(3) var<storage, read_write> outputs: array<u32>;
@group(0) @binding(4) var<uniform>             params: Params;
@group(0) @binding(5) var<storage, read_write> scratch: array<vec2<u32>>;

// ============================================================
// Weight access — offset by individual
// ============================================================

fn w_get(ind: u32, idx: u32) -> vec2<u32> {
    return weights[ind * WEIGHT_COUNT + idx];
}

// ============================================================
// Scratch buffer access — offset by (individual, block)
// ============================================================

fn s_get(base: u32, idx: u32) -> vec2<u32> {
    return scratch[base + idx];
}

fn s_set(base: u32, idx: u32, val: vec2<u32>) {
    scratch[base + idx] = val;
}

// ============================================================
// Main entry point — one thread per (block, individual) pair
// ============================================================

@compute @workgroup_size(64)
fn neural_forward(@builtin(global_invocation_id) gid: vec3<u32>) {
    let blk = gid.x;
    let ind = gid.y;
    if blk >= params.num_blocks { return; }
    if ind >= params.num_individuals { return; }

    let seq_len = max(block_meta[blk], 1u);
    let b_base = blk * MAX_NODES * WORDS_PER_NODE;

    // Per-(individual, block) scratch region
    let sb = (ind * params.num_blocks + blk) * SCRATCH_PER_THREAD;

    // Per-(individual, block) output region
    let out_base = (ind * params.num_blocks + blk) * MAX_OUTPUT;

    // Zero-weight fast path
    var all_zero = true;
    for (var i = 0u; i < INPUT_PROJ_SIZE; i++) {
        let w = w_get(ind, i);
        if w.x != 0u || w.y != 0u { all_zero = false; break; }
    }
    if all_zero {
        for (var s = 0u; s < MAX_OUTPUT; s++) { outputs[out_base + s] = 0u; }
        return;
    }

    // 1. Input projection -> scratch[emb]
    for (var n = 0u; n < seq_len; n++) {
        for (var d = 0u; d < DIM; d++) {
            var acc = FP_ZERO;
            for (var w = 0u; w < WORDS_PER_NODE; w++) {
                let weight = w_get(ind, w * DIM + d);
                let input = blocks[b_base + n * WORDS_PER_NODE + w];
                acc = gl_add(acc, canon_mul(input, weight));
            }
            s_set(sb + S_EMB, n * DIM + d, canon_mul(acc, inv_scale()));
        }
    }

    // 2. Encoder layers
    var layer_w = INPUT_PROJ_SIZE;
    for (var layer = 0u; layer < LAYERS; layer++) {
        let qkv_base = layer_w;
        let op_base = layer_w + QKV_SIZE;

        // Zero attn result
        for (var i = 0u; i < seq_len * DIM; i++) { s_set(sb + S_ATTN, i, FP_ZERO); }

        for (var h = 0u; h < HEADS; h++) {
            let head_off = h * HEAD_DIM;

            // QKV projection
            for (var n = 0u; n < seq_len; n++) {
                for (var d = 0u; d < HEAD_DIM; d++) {
                    let out_d = head_off + d;
                    var q_acc = FP_ZERO;
                    var k_acc = FP_ZERO;
                    var v_acc = FP_ZERO;
                    for (var j = 0u; j < DIM; j++) {
                        let inp = s_get(sb + S_EMB, n * DIM + j);
                        q_acc = gl_add(q_acc, canon_mul(inp, w_get(ind, qkv_base + j * DIM + out_d)));
                        k_acc = gl_add(k_acc, canon_mul(inp, w_get(ind, qkv_base + DIM * DIM + j * DIM + out_d)));
                        v_acc = gl_add(v_acc, canon_mul(inp, w_get(ind, qkv_base + 2u * DIM * DIM + j * DIM + out_d)));
                    }
                    s_set(sb + S_Q, n * HEAD_DIM + d, canon_mul(q_acc, inv_scale()));
                    s_set(sb + S_K, n * HEAD_DIM + d, canon_mul(k_acc, inv_scale()));
                    s_set(sb + S_V, n * HEAD_DIM + d, canon_mul(v_acc, inv_scale()));
                }
            }

            // Attention scores + softmax
            let scale_inv = vec2<u32>(11585u, 0u);
            var scores: array<vec2<u32>, 32>;
            var exp_sc: array<vec2<u32>, 32>;

            for (var i = 0u; i < seq_len; i++) {
                var max_s = vec2<u32>(0xFC180001u, 0xFFFFFFFEu);
                for (var j = 0u; j < seq_len; j++) {
                    var dot_acc = FP_ZERO;
                    for (var d = 0u; d < HEAD_DIM; d++) {
                        dot_acc = gl_add(dot_acc, canon_mul(
                            s_get(sb + S_Q, i * HEAD_DIM + d),
                            s_get(sb + S_K, j * HEAD_DIM + d)));
                    }
                    let score = fp_mul(canon_mul(dot_acc, inv_scale()), scale_inv);
                    scores[j] = score;
                    if fp_gt(score, max_s) { max_s = score; }
                }

                let half = vec2<u32>(32768u, 0u);
                var exp_sum = FP_ZERO;
                for (var j = 0u; j < seq_len; j++) {
                    let x = gl_sub(scores[j], max_s);
                    let x2 = fp_mul(x, x);
                    var ex = gl_add(gl_add(fp_one(), x), fp_mul(x2, half));
                    let hp = half_p();
                    if ex.y > hp.y || (ex.y == hp.y && ex.x > hp.x) { ex = vec2<u32>(66u, 0u); }
                    exp_sc[j] = ex;
                    exp_sum = gl_add(exp_sum, ex);
                }

                let sum_inv = fp_inv(exp_sum);
                for (var j = 0u; j < seq_len; j++) { exp_sc[j] = fp_mul(exp_sc[j], sum_inv); }

                for (var d = 0u; d < HEAD_DIM; d++) {
                    var acc = FP_ZERO;
                    for (var j = 0u; j < seq_len; j++) {
                        acc = gl_add(acc, canon_mul(exp_sc[j], s_get(sb + S_V, j * HEAD_DIM + d)));
                    }
                    let idx = i * DIM + head_off + d;
                    s_set(sb + S_ATTN, idx, gl_add(s_get(sb + S_ATTN, idx), canon_mul(acc, inv_scale())));
                }
            }
        }

        // Output projection -> scratch[proj]
        for (var n = 0u; n < seq_len; n++) {
            for (var d = 0u; d < DIM; d++) {
                var acc = FP_ZERO;
                for (var j = 0u; j < DIM; j++) {
                    acc = gl_add(acc, canon_mul(s_get(sb + S_ATTN, n * DIM + j), w_get(ind, op_base + j * DIM + d)));
                }
                s_set(sb + S_PROJ, n * DIM + d, canon_mul(acc, inv_scale()));
            }
        }

        // Residual: emb += proj
        for (var i = 0u; i < seq_len * DIM; i++) {
            s_set(sb + S_EMB, i, gl_add(s_get(sb + S_EMB, i), s_get(sb + S_PROJ, i)));
        }

        // Layer norm + scale/bias
        let ln_s_base = layer_w + QKV_SIZE + OUT_PROJ_SIZE + FFN1_SIZE + FFN2_SIZE;
        let ln_b_base = ln_s_base + LN_SIZE;
        let inv_dim = vec2<u32>(1024u, 0u);
        for (var n = 0u; n < seq_len; n++) {
            let start = n * DIM;
            var mean = FP_ZERO;
            for (var d = 0u; d < DIM; d++) { mean = gl_add(mean, s_get(sb + S_EMB, start + d)); }
            mean = fp_mul(mean, inv_dim);
            var var_sum = FP_ZERO;
            for (var d = 0u; d < DIM; d++) {
                let diff = gl_sub(s_get(sb + S_EMB, start + d), mean);
                var_sum = gl_add(var_sum, fp_mul(diff, diff));
            }
            let variance = fp_mul(var_sum, inv_dim);
            let eps = vec2<u32>(1u, 0u);
            var var_scale = fp_one();
            if variance.y > 0u || variance.x > eps.x {
                var_scale = fp_inv(variance);
            }
            for (var d = 0u; d < DIM; d++) {
                let normed = fp_mul(gl_sub(s_get(sb + S_EMB, start + d), mean), var_scale);
                s_set(sb + S_EMB, start + d,
                    gl_add(fp_mul(normed, w_get(ind, ln_s_base + d)), w_get(ind, ln_b_base + d)));
            }
        }

        // FFN with residual
        let ffn1_base = layer_w + QKV_SIZE + OUT_PROJ_SIZE;
        let ffn2_base = ffn1_base + FFN1_SIZE;
        var ffn_h: array<vec2<u32>, 64>;
        for (var n = 0u; n < seq_len; n++) {
            for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                var acc = FP_ZERO;
                for (var d = 0u; d < DIM; d++) {
                    acc = gl_add(acc, canon_mul(s_get(sb + S_EMB, n * DIM + d), w_get(ind, ffn1_base + d * FFN_HIDDEN + fh)));
                }
                ffn_h[fh] = fp_relu(canon_mul(acc, inv_scale()));
            }
            for (var d = 0u; d < DIM; d++) {
                var acc = FP_ZERO;
                for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                    acc = gl_add(acc, canon_mul(ffn_h[fh], w_get(ind, ffn2_base + fh * DIM + d)));
                }
                s_set(sb + S_FFN_OUT, n * DIM + d, canon_mul(acc, inv_scale()));
            }
        }
        for (var i = 0u; i < seq_len * DIM; i++) {
            s_set(sb + S_EMB, i, gl_add(s_get(sb + S_EMB, i), s_get(sb + S_FFN_OUT, i)));
        }

        layer_w += LAYER_SIZE;
    }

    // 3. Mean pooling -> latent
    var latent: array<vec2<u32>, 64>;
    for (var d = 0u; d < DIM; d++) {
        var sum = FP_ZERO;
        for (var n = 0u; n < seq_len; n++) { sum = gl_add(sum, s_get(sb + S_EMB, n * DIM + d)); }
        let inv_n = fp_inv_u32(seq_len);
        latent[d] = fp_mul(sum, inv_n);
    }

    // 4. Autoregressive decoder
    var prev: array<vec2<u32>, 64>;
    var dec_h: array<vec2<u32>, 64>;
    var dec_out: array<vec2<u32>, 64>;
    for (var d = 0u; d < DIM; d++) { prev[d] = FP_ZERO; }

    let dec_w = layer_w;
    let dh_base = dec_w;
    let dhb_base = dec_w + DEC_HIDDEN_SIZE;
    let do_base = dec_w + DEC_HIDDEN_SIZE + DEC_HBIAS_SIZE;
    let dob_base = dec_w + DEC_HIDDEN_SIZE + DEC_HBIAS_SIZE + DEC_OUTPUT_SIZE;

    for (var step = 0u; step < MAX_OUTPUT; step++) {
        for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
            let bias = w_get(ind, dhb_base + fh);
            var acc = canon_mul(bias, vec2<u32>(FP_ONE_LO, FP_ONE_HI));
            for (var d = 0u; d < DIM; d++) {
                acc = gl_add(acc, canon_mul(latent[d], w_get(ind, dh_base + d * FFN_HIDDEN + fh)));
            }
            for (var d = 0u; d < DIM; d++) {
                acc = gl_add(acc, canon_mul(prev[d], w_get(ind, dh_base + (DIM + d) * FFN_HIDDEN + fh)));
            }
            dec_h[fh] = fp_relu(canon_mul(acc, inv_scale()));
        }

        for (var d = 0u; d < DIM; d++) {
            let bias = w_get(ind, dob_base + d);
            var acc = canon_mul(bias, vec2<u32>(FP_ONE_LO, FP_ONE_HI));
            for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                acc = gl_add(acc, canon_mul(dec_h[fh], w_get(ind, do_base + fh * DIM + d)));
            }
            dec_out[d] = canon_mul(acc, inv_scale());
        }

        var best_idx = 0u;
        var best_val = dec_out[0];
        for (var di = 1u; di < DIM; di++) {
            if fp_gt(dec_out[di], best_val) { best_val = dec_out[di]; best_idx = di; }
        }
        outputs[out_base + step] = best_idx;
        if best_idx == 0u {
            for (var ss = step + 1u; ss < MAX_OUTPUT; ss++) { outputs[out_base + ss] = 0u; }
            return;
        }
        for (var d = 0u; d < DIM; d++) { prev[d] = dec_out[d]; }
    }
}
