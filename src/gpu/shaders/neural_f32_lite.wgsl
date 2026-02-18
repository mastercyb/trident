// Lite neural forward pass — float32, MLP-only, batched dispatch.
//
// No attention. Flat input -> hidden -> decoder. ~1KB private memory per thread.
// 2D dispatch: gid.x = block index, gid.y = individual index.
//
// Model: flatten(128) -> linear+ReLU(32) -> linear+ReLU(32) -> decoder(16 steps, vocab=64)
// Weight layout: 10,400 f32 values per individual.

const LITE_DIM: u32 = 32u;
const LITE_FFN: u32 = 32u;
const LITE_VOCAB: u32 = 64u;
const INPUT_FLAT: u32 = 128u;   // MAX_NODES * WORDS_PER_NODE
const MAX_OUTPUT: u32 = 16u;
const WORDS_PER_NODE: u32 = 4u;
const MAX_NODES: u32 = 32u;
const LITE_WEIGHT_COUNT: u32 = 10400u;

// Weight offsets
const IP_OFF: u32 = 0u;          // input_proj: 128*32 = 4096
const IB_OFF: u32 = 4096u;       // input_bias: 32
const HW_OFF: u32 = 4128u;       // hidden_w: 32*32 = 1024
const HB_OFF: u32 = 5152u;       // hidden_bias: 32
const DH_OFF: u32 = 5184u;       // dec_hidden: 96*32 = 3072
const DHB_OFF: u32 = 8256u;      // dec_hidden_bias: 32
const DO_OFF: u32 = 8288u;       // dec_output: 32*64 = 2048
const DOB_OFF: u32 = 10336u;     // dec_output_bias: 64

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

// Private memory: ~1KB total (vs 17KB for full model).
var<private> latent: array<f32, 32>;     // LITE_DIM
var<private> dec_h: array<f32, 32>;      // LITE_FFN
var<private> dec_out: array<f32, 64>;    // LITE_VOCAB
var<private> prev_out: array<f32, 64>;   // LITE_VOCAB

fn w(ind: u32, idx: u32) -> f32 {
    return weights[ind * LITE_WEIGHT_COUNT + idx];
}

@compute @workgroup_size(64)
fn neural_f32_lite(@builtin(global_invocation_id) gid: vec3<u32>) {
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

    // 1. Flatten + project: all nodes -> [LITE_DIM] + bias + ReLU
    for (var d = 0u; d < LITE_DIM; d++) {
        var acc = w(ind, IB_OFF + d);  // bias
        for (var n = 0u; n < seq_len; n++) {
            for (var ww = 0u; ww < WORDS_PER_NODE; ww++) {
                let flat_idx = n * WORDS_PER_NODE + ww;
                acc += block_nodes[b_base + flat_idx] * w(ind, IP_OFF + flat_idx * LITE_DIM + d);
            }
        }
        latent[d] = max(acc, 0.0);  // ReLU
    }

    // 2. Hidden layer: [LITE_DIM] -> [LITE_DIM] + bias + ReLU
    var hidden: array<f32, 32>;
    for (var d = 0u; d < LITE_DIM; d++) {
        var acc = w(ind, HB_OFF + d);
        for (var j = 0u; j < LITE_DIM; j++) {
            acc += latent[j] * w(ind, HW_OFF + j * LITE_DIM + d);
        }
        hidden[d] = max(acc, 0.0);
    }
    // Copy hidden to latent for decoder input
    for (var d = 0u; d < LITE_DIM; d++) { latent[d] = hidden[d]; }

    // 3. Autoregressive decoder
    for (var d = 0u; d < LITE_VOCAB; d++) { prev_out[d] = 0.0; }

    for (var step = 0u; step < MAX_OUTPUT; step++) {
        // Decoder hidden: [LITE_DIM + LITE_VOCAB] -> [LITE_FFN] + bias + ReLU
        for (var fh = 0u; fh < LITE_FFN; fh++) {
            var acc = w(ind, DHB_OFF + fh);
            for (var d = 0u; d < LITE_DIM; d++) {
                acc += latent[d] * w(ind, DH_OFF + d * LITE_FFN + fh);
            }
            for (var d = 0u; d < LITE_VOCAB; d++) {
                acc += prev_out[d] * w(ind, DH_OFF + (LITE_DIM + d) * LITE_FFN + fh);
            }
            dec_h[fh] = max(acc, 0.0);
        }

        // Decoder output: [LITE_FFN] -> [LITE_VOCAB] + bias
        for (var d = 0u; d < LITE_VOCAB; d++) {
            var acc = w(ind, DOB_OFF + d);
            for (var fh = 0u; fh < LITE_FFN; fh++) {
                acc += dec_h[fh] * w(ind, DO_OFF + fh * LITE_VOCAB + d);
            }
            dec_out[d] = acc;
        }

        // Argmax over LITE_VOCAB
        var best_idx = 0u;
        var best_val = dec_out[0];
        for (var di = 1u; di < LITE_VOCAB; di++) {
            if dec_out[di] > best_val { best_val = dec_out[di]; best_idx = di; }
        }
        outputs[out_base + step] = best_idx;
        if best_idx == 0u {
            for (var ss = step + 1u; ss < MAX_OUTPUT; ss++) { outputs[out_base + ss] = 0u; }
            return;
        }
        for (var d = 0u; d < LITE_VOCAB; d++) { prev_out[d] = dec_out[d]; }
    }
}
