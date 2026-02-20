// Grammar mask compute shader for beam search inference.
//
// Maintains K=32 independent stack state machines and produces
// validity masks over the VOCAB (140 tokens) at each decoding step.
// One workgroup of 32 threads — one thread per beam.
//
// No CPU↔GPU sync during decode: stack states are GPU-resident.

// ─── Constants ───────────────────────────────────────────────────

const VOCAB_SIZE: u32 = 140u;
const MAX_BEAMS: u32 = 32u;

// ─── Bindings ────────────────────────────────────────────────────

// Per-beam stack depth (read/write between steps)
@group(0) @binding(0) var<storage, read_write> stack_depths: array<i32, 32>;

// Token chosen at this step for each beam (set by decoder before dispatch)
@group(0) @binding(1) var<storage, read> chosen_tokens: array<u32, 32>;

// Output mask: [K * VOCAB_SIZE] — 0.0 = valid, -1e9 = masked
@group(0) @binding(2) var<storage, read_write> masks: array<f32>;

// Stack effects table: [VOCAB_SIZE * 2] — (pops, pushes) packed as i32 pairs
@group(0) @binding(3) var<storage, read> stack_effects: array<i32>;

// Minimum stack depth table: [VOCAB_SIZE]
@group(0) @binding(4) var<storage, read> min_depths: array<i32, 140>;

// ─── Workgroup ───────────────────────────────────────────────────

@compute @workgroup_size(32, 1, 1)
fn grammar_mask_step(@builtin(local_invocation_id) lid: vec3<u32>) {
    let beam = lid.x;
    if beam >= MAX_BEAMS {
        return;
    }

    // 1. Apply the chosen token to update stack depth
    let token = chosen_tokens[beam];
    if token > 0u && token < VOCAB_SIZE {
        let pops = stack_effects[token * 2u];
        let pushes = stack_effects[token * 2u + 1u];
        stack_depths[beam] = max(stack_depths[beam] + pushes - pops, 0);
    }

    // 2. Compute validity mask for this beam
    let depth = stack_depths[beam];
    let base = beam * VOCAB_SIZE;

    // EOS always valid
    masks[base] = 0.0;

    // Check each non-EOS token
    for (var t: u32 = 1u; t < VOCAB_SIZE; t = t + 1u) {
        if depth < min_depths[t] {
            masks[base + t] = -1000000000.0;
        } else {
            masks[base + t] = 0.0;
        }
    }
}
