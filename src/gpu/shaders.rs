/// Goldilocks field arithmetic (canonical form).
pub const GOLDILOCKS: &str = include_str!("shaders/goldilocks.wgsl");

/// Fixed-point arithmetic over Goldilocks (scale = 2^16).
pub const FIXED_POINT: &str = include_str!("shaders/fixed_point.wgsl");

/// Neural forward pass (block-parallel dispatch, Goldilocks field).
const NEURAL_FORWARD: &str = include_str!("shaders/neural_forward.wgsl");

/// Neural forward pass (float32, no field emulation).
pub const NEURAL_F32: &str = include_str!("shaders/neural_f32.wgsl");

/// Concatenated neural shader: goldilocks + fixed_point + forward pass.
pub fn neural_shader() -> String {
    format!("{}\n{}\n{}", GOLDILOCKS, FIXED_POINT, NEURAL_FORWARD)
}
