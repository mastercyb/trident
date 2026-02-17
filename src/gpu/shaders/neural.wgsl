// Neural forward pass in canonical Goldilocks fixed-point arithmetic.
//
// Each thread runs one complete forward pass for one (individual, block) pair.
// 928 threads = 16 individuals x 58 blocks, dispatched as ceil(928/64) = 15 workgroups.
//
// All field elements are canonical Goldilocks (NOT Montgomery):
//   element = raw u64 mod p, stored as vec2<u32>(lo, hi).
//   p = 2^64 - 2^32 + 1 = 0xFFFFFFFF00000001.
//
// Large per-thread arrays live in the global `scratch` buffer to avoid
// exceeding GPU stack/private-memory limits. Small arrays (<=64 elements)
// stay in function-scope vars.

// ============================================================
// Constants
// ============================================================

const GL_P_LO: u32 = 0x00000001u;
const GL_P_HI: u32 = 0xFFFFFFFFu;

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
// emb:     0..2048        (MAX_NODES * DIM)
// attn:    2048..4096     (MAX_NODES * DIM)
// proj:    4096..6144     (MAX_NODES * DIM)
// ffn_out: 6144..8192     (MAX_NODES * DIM)
// q:       8192..9216     (MAX_NODES * HEAD_DIM)
// k:       9216..10240    (MAX_NODES * HEAD_DIM)
// v:       10240..11264   (MAX_NODES * HEAD_DIM)
const SCRATCH_PER_THREAD: u32 = 11264u;
const S_EMB: u32 = 0u;
const S_ATTN: u32 = 2048u;
const S_PROJ: u32 = 4096u;
const S_FFN_OUT: u32 = 6144u;
const S_Q: u32 = 8192u;
const S_K: u32 = 9216u;
const S_V: u32 = 10240u;

// ============================================================
// Buffers
// ============================================================

struct Params {
    num_individuals: u32,
    num_blocks: u32,
    inv_scale_lo: u32,
    inv_scale_hi: u32,
    half_p_lo: u32,
    half_p_hi: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read>       weights: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read>       blocks: array<vec2<u32>>;
@group(0) @binding(2) var<storage, read>       block_meta: array<u32>;
@group(0) @binding(3) var<storage, read_write> outputs: array<u32>;
@group(0) @binding(4) var<uniform>             params: Params;
@group(0) @binding(5) var<storage, read_write> scratch: array<vec2<u32>>;

// ============================================================
// Canonical Goldilocks field arithmetic
// ============================================================

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

fn gl_sub(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    if a.y > b.y || (a.y == b.y && a.x >= b.x) {
        let lo = a.x - b.x;
        let borrow = select(0u, 1u, a.x < b.x);
        let hi = a.y - b.y - borrow;
        return vec2<u32>(lo, hi);
    }
    let ap_lo = a.x + GL_P_LO;
    let carry = select(0u, 1u, ap_lo < a.x);
    let ap_hi = a.y + GL_P_HI + carry;
    let lo = ap_lo - b.x;
    let borrow = select(0u, 1u, ap_lo < b.x);
    let hi = ap_hi - b.y - borrow;
    return vec2<u32>(lo, hi);
}

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

// ============================================================
// Fixed-point arithmetic (scale = 65536)
// ============================================================

fn inv_scale() -> vec2<u32> {
    return vec2<u32>(params.inv_scale_lo, params.inv_scale_hi);
}

fn half_p() -> vec2<u32> {
    return vec2<u32>(params.half_p_lo, params.half_p_hi);
}

const FP_ZERO: vec2<u32> = vec2<u32>(0u, 0u);
const FP_ONE_LO: u32 = 65536u;
const FP_ONE_HI: u32 = 0u;

fn fp_one() -> vec2<u32> { return vec2<u32>(FP_ONE_LO, FP_ONE_HI); }

fn fp_mul(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return canon_mul(canon_mul(a, b), inv_scale());
}

fn fp_relu(x: vec2<u32>) -> vec2<u32> {
    let hp = half_p();
    if x.y > hp.y || (x.y == hp.y && x.x > hp.x) { return FP_ZERO; }
    return x;
}

fn fp_gt(a: vec2<u32>, b: vec2<u32>) -> bool {
    let hp = half_p();
    let a_neg = a.y > hp.y || (a.y == hp.y && a.x > hp.x);
    let b_neg = b.y > hp.y || (b.y == hp.y && b.x > hp.x);
    if !a_neg && b_neg { return true; }
    if a_neg && !b_neg { return false; }
    if !a_neg { return a.y > b.y || (a.y == b.y && a.x > b.x); }
    return a.y < b.y || (a.y == b.y && a.x < b.x);
}

fn fp_inv(x: vec2<u32>) -> vec2<u32> {
    if x.x == 0u && x.y == 0u { return fp_one(); }
    let x_inv = gl_field_inv(x);
    let scale = vec2<u32>(FP_ONE_LO, FP_ONE_HI);
    let scale_sq = canon_mul(scale, scale);
    return canon_mul(scale_sq, x_inv);
}

fn gl_field_inv(a: vec2<u32>) -> vec2<u32> {
    var result = vec2<u32>(1u, 0u);
    var base = a;
    for (var i = 0u; i < 32u; i++) {
        result = canon_mul(result, base);
        base = canon_mul(base, base);
    }
    base = canon_mul(base, base);
    for (var i = 1u; i < 32u; i++) {
        result = canon_mul(result, base);
        base = canon_mul(base, base);
    }
    return result;
}

fn fp_inv_sqrt(x: vec2<u32>) -> vec2<u32> {
    if x.x == 0u && x.y == 0u { return fp_one(); }
    var y = fp_one();
    let three = vec2<u32>(196608u, 0u);
    let half = vec2<u32>(32768u, 0u);
    for (var iter = 0u; iter < 8u; iter++) {
        let y2 = fp_mul(y, y);
        let xy2 = fp_mul(x, y2);
        let diff = gl_sub(three, xy2);
        y = fp_mul(fp_mul(y, diff), half);
    }
    return y;
}

fn fp_inv_u32(n: u32) -> vec2<u32> {
    let val = 65536u / n;
    return vec2<u32>(val, 0u);
}

// ============================================================
// Scratch buffer access helpers
// ============================================================

fn s_get(base: u32, idx: u32) -> vec2<u32> {
    return scratch[base + idx];
}

fn s_set(base: u32, idx: u32, val: vec2<u32>) {
    scratch[base + idx] = val;
}

// ============================================================
// Main entry point
// ============================================================

@compute @workgroup_size(64)
fn neural_forward(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pass_id = gid.x;
    let total_passes = params.num_individuals * params.num_blocks;
    if pass_id >= total_passes { return; }

    let ind = pass_id / params.num_blocks;
    let blk = pass_id % params.num_blocks;
    let seq_len = max(block_meta[blk], 1u);
    let w_base = ind * WEIGHT_COUNT;
    let b_base = blk * MAX_NODES * WORDS_PER_NODE;

    // Per-thread scratch region
    let sb = pass_id * SCRATCH_PER_THREAD;

    // Zero-weight fast path
    let out_base = pass_id * MAX_OUTPUT;
    var all_zero = true;
    for (var i = 0u; i < INPUT_PROJ_SIZE; i++) {
        let w = weights[w_base + i];
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
                let weight = weights[w_base + w * DIM + d];
                let input = blocks[b_base + n * WORDS_PER_NODE + w];
                acc = gl_add(acc, canon_mul(input, weight));
            }
            s_set(sb + S_EMB, n * DIM + d, canon_mul(acc, inv_scale()));
        }
    }

    // 2. Encoder layers
    var layer_w = w_base + INPUT_PROJ_SIZE;
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
                        q_acc = gl_add(q_acc, canon_mul(inp, weights[qkv_base + j * DIM + out_d]));
                        k_acc = gl_add(k_acc, canon_mul(inp, weights[qkv_base + DIM * DIM + j * DIM + out_d]));
                        v_acc = gl_add(v_acc, canon_mul(inp, weights[qkv_base + 2u * DIM * DIM + j * DIM + out_d]));
                    }
                    s_set(sb + S_Q, n * HEAD_DIM + d, canon_mul(q_acc, inv_scale()));
                    s_set(sb + S_K, n * HEAD_DIM + d, canon_mul(k_acc, inv_scale()));
                    s_set(sb + S_V, n * HEAD_DIM + d, canon_mul(v_acc, inv_scale()));
                }
            }

            // Attention scores + softmax (small arrays stay on stack)
            let scale_inv = vec2<u32>(11585u, 0u);  // round(1/sqrt(32) * 65536)
            var scores: array<vec2<u32>, 32>;
            var exp_sc: array<vec2<u32>, 32>;

            for (var i = 0u; i < seq_len; i++) {
                var max_s = FP_ZERO;
                var max_neg = true;
                for (var j = 0u; j < seq_len; j++) {
                    var dot_acc = FP_ZERO;
                    for (var d = 0u; d < HEAD_DIM; d++) {
                        dot_acc = gl_add(dot_acc, canon_mul(
                            s_get(sb + S_Q, i * HEAD_DIM + d),
                            s_get(sb + S_K, j * HEAD_DIM + d)));
                    }
                    let score = fp_mul(canon_mul(dot_acc, inv_scale()), scale_inv);
                    scores[j] = score;
                    if max_neg || fp_gt(score, max_s) { max_s = score; max_neg = false; }
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
                    acc = gl_add(acc, canon_mul(s_get(sb + S_ATTN, n * DIM + j), weights[op_base + j * DIM + d]));
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
            let inv_std = fp_inv_sqrt(variance);
            for (var d = 0u; d < DIM; d++) {
                let normed = fp_mul(gl_sub(s_get(sb + S_EMB, start + d), mean), inv_std);
                s_set(sb + S_EMB, start + d,
                    gl_add(fp_mul(normed, weights[ln_s_base + d]), weights[ln_b_base + d]));
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
                    acc = gl_add(acc, canon_mul(s_get(sb + S_EMB, n * DIM + d), weights[ffn1_base + d * FFN_HIDDEN + fh]));
                }
                ffn_h[fh] = fp_relu(canon_mul(acc, inv_scale()));
            }
            for (var d = 0u; d < DIM; d++) {
                var acc = FP_ZERO;
                for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                    acc = gl_add(acc, canon_mul(ffn_h[fh], weights[ffn2_base + fh * DIM + d]));
                }
                s_set(sb + S_FFN_OUT, n * DIM + d, canon_mul(acc, inv_scale()));
            }
        }
        for (var i = 0u; i < seq_len * DIM; i++) {
            s_set(sb + S_EMB, i, gl_add(s_get(sb + S_EMB, i), s_get(sb + S_FFN_OUT, i)));
        }

        layer_w += LAYER_SIZE;
    }

    // 3. Mean pooling -> latent (on stack, only 64 elements)
    var latent: array<vec2<u32>, 64>;
    for (var d = 0u; d < DIM; d++) {
        var sum = FP_ZERO;
        for (var n = 0u; n < seq_len; n++) { sum = gl_add(sum, s_get(sb + S_EMB, n * DIM + d)); }
        let inv_n = fp_inv_u32(seq_len);
        latent[d] = fp_mul(sum, inv_n);
    }

    // 4. Autoregressive decoder (small arrays on stack)
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
            let bias = weights[dhb_base + fh];
            var acc = canon_mul(bias, vec2<u32>(FP_ONE_LO, FP_ONE_HI));
            for (var d = 0u; d < DIM; d++) {
                acc = gl_add(acc, canon_mul(latent[d], weights[dh_base + d * FFN_HIDDEN + fh]));
            }
            for (var d = 0u; d < DIM; d++) {
                acc = gl_add(acc, canon_mul(prev[d], weights[dh_base + (DIM + d) * FFN_HIDDEN + fh]));
            }
            dec_h[fh] = fp_relu(canon_mul(acc, inv_scale()));
        }

        for (var d = 0u; d < DIM; d++) {
            let bias = weights[dob_base + d];
            var acc = canon_mul(bias, vec2<u32>(FP_ONE_LO, FP_ONE_HI));
            for (var fh = 0u; fh < FFN_HIDDEN; fh++) {
                acc = gl_add(acc, canon_mul(dec_h[fh], weights[do_base + fh * DIM + d]));
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
