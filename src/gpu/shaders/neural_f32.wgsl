// Neural forward pass — float32, batched dispatch.
//
// Pure f32 arithmetic. No field emulation. Native GPU math.
// 2D dispatch: gid.x = block index, gid.y = individual index.
//
// Model: input_proj(4×64) → 2× encoder(attn+FFN, dim=64) → mean pool → decoder(16 steps) → argmax
// Weight layout per individual: [input_proj, layer0, layer1, decoder] = 62080 f32 values.

const DIM: u32 = 64u;
const HEADS: u32 = 2u;
const HEAD_DIM: u32 = 32u;
const LAYERS: u32 = 2u;
const FFN_HIDDEN: u32 = 64u;
const MAX_OUTPUT: u32 = 16u;
const WORDS_PER_NODE: u32 = 4u;
const MAX_NODES: u32 = 32u;

// Weight offsets within a single individual
const INPUT_PROJ_SIZE: u32 = 256u;   // 4 * 64
const QKV_SIZE: u32 = 12288u;        // 3 * 64 * 64
const OUT_PROJ_SIZE: u32 = 4096u;    // 64 * 64
const FFN1_SIZE: u32 = 4096u;        // 64 * 64
const FFN2_SIZE: u32 = 4096u;        // 64 * 64
const LN_SIZE: u32 = 64u;
const LAYER_SIZE: u32 = 24704u;      // QKV + out + ffn1 + ffn2 + ln_s + ln_b

const DEC_HIDDEN_SIZE: u32 = 8192u;  // 128 * 64
const DEC_HBIAS_SIZE: u32 = 64u;
const DEC_OUTPUT_SIZE: u32 = 4096u;  // 64 * 64
const DEC_OBIAS_SIZE: u32 = 64u;

const WEIGHT_COUNT: u32 = 62080u;

struct Params {
    num_blocks: u32,
    num_individuals: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read>       weights: array<f32>;
@group(0) @binding(1) var<storage, read>       block_nodes: array<f32>;
@group(0) @binding(2) var<storage, read>       block_meta: array<u32>;
@group(0) @binding(3) var<storage, read_write> outputs: array<u32>;
@group(0) @binding(4) var<uniform>             params: Params;

// Per-thread private arrays (registers, not global memory)
var<private> emb: array<f32, 2048>;      // MAX_NODES * DIM
var<private> attn: array<f32, 2048>;     // MAX_NODES * DIM
var<private> proj: array<f32, 2048>;     // MAX_NODES * DIM
var<private> q_buf: array<f32, 1024>;    // MAX_NODES * HEAD_DIM
var<private> k_buf: array<f32, 1024>;    // MAX_NODES * HEAD_DIM
var<private> v_buf: array<f32, 1024>;    // MAX_NODES * HEAD_DIM
var<private> latent: array<f32, 64>;     // DIM
var<private> prev_out: array<f32, 64>;   // DIM
var<private> dec_h: array<f32, 64>;      // FFN_HIDDEN
var<private> dec_o: array<f32, 64>;      // DIM

fn w(ind: u32, idx: u32) -> f32 {
    return weights[ind * WEIGHT_COUNT + idx];
}

@compute @workgroup_size(64)
fn neural_f32(@builtin(global_invocation_id) gid: vec3<u32>) {
    let blk = gid.x;
    let ind = gid.y;
    if blk >= params.num_blocks || ind >= params.num_individuals { return; }

    let seq_len = max(block_meta[blk], 1u);
    let b_base = blk * MAX_NODES * WORDS_PER_NODE;
    let out_base = (ind * params.num_blocks + blk) * MAX_OUTPUT;

    // Zero-weight fast path
    var all_zero = true;
    for (var i = 0u; i < 16u; i++) {
        if w(ind, i) != 0.0 { all_zero = false; break; }
    }
    if all_zero {
        for (var s = 0u; s < MAX_OUTPUT; s++) { outputs[out_base + s] = 0u; }
        return;
    }

    // 1. Input projection: [seq_len, 4] × [4, 64] → [seq_len, 64]
    for (var n = 0u; n < seq_len; n++) {
        for (var d = 0u; d < DIM; d++) {
            var acc = 0.0;
            for (var ww = 0u; ww < WORDS_PER_NODE; ww++) {
                acc += block_nodes[b_base + n * WORDS_PER_NODE + ww] * w(ind, ww * DIM + d);
            }
            emb[n * DIM + d] = acc;
        }
    }

    // 2. Encoder layers
    var layer_w = INPUT_PROJ_SIZE;
    for (var layer = 0u; layer < LAYERS; layer++) {
        let qkv_base = layer_w;
        let op_base = layer_w + QKV_SIZE;

        // Zero attention result
        for (var i = 0u; i < seq_len * DIM; i++) { attn[i] = 0.0; }

        for (var h = 0u; h < HEADS; h++) {
            let head_off = h * HEAD_DIM;

            // QKV projection
            for (var n = 0u; n < seq_len; n++) {
                for (var d = 0u; d < HEAD_DIM; d++) {
                    let out_d = head_off + d;
                    var qa = 0.0; var ka = 0.0; var va = 0.0;
                    for (var j = 0u; j < DIM; j++) {
                        let inp = emb[n * DIM + j];
                        qa += inp * w(ind, qkv_base + j * DIM + out_d);
                        ka += inp * w(ind, qkv_base + DIM * DIM + j * DIM + out_d);
                        va += inp * w(ind, qkv_base + 2u * DIM * DIM + j * DIM + out_d);
                    }
                    q_buf[n * HEAD_DIM + d] = qa;
                    k_buf[n * HEAD_DIM + d] = ka;
                    v_buf[n * HEAD_DIM + d] = va;
                }
            }

            // Attention scores + softmax
            let scale_inv = 1.0 / sqrt(f32(HEAD_DIM));
            for (var i = 0u; i < seq_len; i++) {
                // Compute scores and find max
                var max_s = -1e9;
                var scores: array<f32, 32>;  // MAX_NODES
                for (var j = 0u; j < seq_len; j++) {
                    var dot = 0.0;
                    for (var d = 0u; d < HEAD_DIM; d++) {
                        dot += q_buf[i * HEAD_DIM + d] * k_buf[j * HEAD_DIM + d];
                    }
                    let sc = dot * scale_inv;
                    scores[j] = sc;
                    max_s = max(max_s, sc);
                }

                // Exp + normalize
                var exp_sum = 0.0;
                var exp_sc: array<f32, 32>;
                for (var j = 0u; j < seq_len; j++) {
                    let e = exp(scores[j] - max_s);
                    exp_sc[j] = e;
                    exp_sum += e;
                }
                let inv_sum = 1.0 / max(exp_sum, 1e-10);
                for (var j = 0u; j < seq_len; j++) { exp_sc[j] *= inv_sum; }

                // Weighted sum of values
                for (var d = 0u; d < HEAD_DIM; d++) {
                    var acc = 0.0;
                    for (var j = 0u; j < seq_len; j++) {
                        acc += exp_sc[j] * v_buf[j * HEAD_DIM + d];
                    }
                    attn[i * DIM + head_off + d] += acc;
                }
            }
        }

        // Output projection
        for (var n = 0u; n < seq_len; n++) {
            for (var d = 0u; d < DIM; d++) {
                var acc = 0.0;
                for (var j = 0u; j < DIM; j++) {
                    acc += attn[n * DIM + j] * w(ind, op_base + j * DIM + d);
                }
                proj[n * DIM + d] = acc;
            }
        }

        // Residual
        for (var i = 0u; i < seq_len * DIM; i++) { emb[i] += proj[i]; }

        // Layer norm
        let ln_s_base = layer_w + QKV_SIZE + OUT_PROJ_SIZE + FFN1_SIZE + FFN2_SIZE;
        let ln_b_base = ln_s_base + LN_SIZE;
        let inv_dim = 1.0 / f32(DIM);
        for (var n = 0u; n < seq_len; n++) {
            let start = n * DIM;
            var mean = 0.0;
            for (var d = 0u; d < DIM; d++) { mean += emb[start + d]; }
            mean *= inv_dim;
            var var_sum = 0.0;
            for (var d = 0u; d < DIM; d++) {
                let diff = emb[start + d] - mean;
                var_sum += diff * diff;
            }
            let variance = var_sum * inv_dim;
            let inv_std = inverseSqrt(variance + 1e-5);
            for (var d = 0u; d < DIM; d++) {
                let normed = (emb[start + d] - mean) * inv_std;
                emb[start + d] = normed * w(ind, ln_s_base + d) + w(ind, ln_b_base + d);
            }
        }

        // FFN with residual
        let ffn1_base = layer_w + QKV_SIZE + OUT_PROJ_SIZE;
        let ffn2_base = ffn1_base + FFN1_SIZE;
        var ffn_h: array<f32, 64>;
        for (var n = 0u; n < seq_len; n++) {
            for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                var acc = 0.0;
                for (var d = 0u; d < DIM; d++) {
                    acc += emb[n * DIM + d] * w(ind, ffn1_base + d * FFN_HIDDEN + fh);
                }
                ffn_h[fh] = max(acc, 0.0);  // ReLU
            }
            for (var d = 0u; d < DIM; d++) {
                var acc = 0.0;
                for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                    acc += ffn_h[fh] * w(ind, ffn2_base + fh * DIM + d);
                }
                emb[n * DIM + d] += acc;  // residual
            }
        }

        layer_w += LAYER_SIZE;
    }

    // 3. Mean pooling
    let inv_n = 1.0 / f32(seq_len);
    for (var d = 0u; d < DIM; d++) {
        var sum = 0.0;
        for (var n = 0u; n < seq_len; n++) { sum += emb[n * DIM + d]; }
        latent[d] = sum * inv_n;
    }

    // 4. Autoregressive decoder
    for (var d = 0u; d < DIM; d++) { prev_out[d] = 0.0; }

    let dec_w = layer_w;
    let dh_base = dec_w;
    let dhb_base = dec_w + DEC_HIDDEN_SIZE;
    let do_base = dec_w + DEC_HIDDEN_SIZE + DEC_HBIAS_SIZE;
    let dob_base = dec_w + DEC_HIDDEN_SIZE + DEC_HBIAS_SIZE + DEC_OUTPUT_SIZE;

    for (var step = 0u; step < MAX_OUTPUT; step++) {
        // Hidden layer + ReLU
        for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
            var acc = w(ind, dhb_base + fh);
            for (var d = 0u; d < DIM; d++) {
                acc += latent[d] * w(ind, dh_base + d * FFN_HIDDEN + fh);
            }
            for (var d = 0u; d < DIM; d++) {
                acc += prev_out[d] * w(ind, dh_base + (DIM + d) * FFN_HIDDEN + fh);
            }
            dec_h[fh] = max(acc, 0.0);
        }

        // Output layer
        for (var d = 0u; d < DIM; d++) {
            var acc = w(ind, dob_base + d);
            for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                acc += dec_h[fh] * w(ind, do_base + fh * DIM + d);
            }
            dec_o[d] = acc;
        }

        // Argmax
        var best_idx = 0u;
        var best_val = dec_o[0];
        for (var di = 1u; di < DIM; di++) {
            if dec_o[di] > best_val { best_val = dec_o[di]; best_idx = di; }
        }
        outputs[out_base + step] = best_idx;
        if best_idx == 0u {
            for (var ss = step + 1u; ss < MAX_OUTPUT; ss++) { outputs[out_base + ss] = 0u; }
            return;
        }
        for (var d = 0u; d < DIM; d++) { prev_out[d] = dec_o[d]; }
    }
}
