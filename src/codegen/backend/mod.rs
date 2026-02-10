pub mod cairo;
pub mod miden;
pub mod openvm;
pub mod sp1;
pub mod triton;

/// Trait abstracting instruction emission for stack-machine backends.
///
/// Each method returns the target assembly string for a semantic operation.
/// The Emitter calls these to produce target-specific output while sharing
/// all AST-walking and stack-management logic.
#[allow(dead_code)] // Methods used by TritonBackend; other backends will use all methods.
pub(crate) trait StackBackend {
    /// Target name (e.g. "triton", "miden").
    fn target_name(&self) -> &str;
    /// File extension for output (e.g. ".tasm").
    fn output_extension(&self) -> &str;

    // --- Stack operations ---
    fn inst_push(&self, value: u64) -> String;
    fn inst_pop(&self, count: u32) -> String;
    fn inst_dup(&self, depth: u32) -> String;
    fn inst_swap(&self, depth: u32) -> String;

    // --- Arithmetic ---
    fn inst_add(&self) -> &'static str;
    fn inst_mul(&self) -> &'static str;
    fn inst_eq(&self) -> &'static str;
    fn inst_invert(&self) -> &'static str;
    fn inst_split(&self) -> &'static str;
    fn inst_lt(&self) -> &'static str;
    fn inst_and(&self) -> &'static str;
    fn inst_xor(&self) -> &'static str;
    fn inst_div_mod(&self) -> &'static str;
    fn inst_log2(&self) -> &'static str;
    fn inst_pow(&self) -> &'static str;
    fn inst_pop_count(&self) -> &'static str;
    fn inst_xb_mul(&self) -> &'static str;
    fn inst_x_invert(&self) -> &'static str;

    // --- I/O ---
    fn inst_read_io(&self, count: u32) -> String;
    fn inst_write_io(&self, count: u32) -> String;
    fn inst_divine(&self, count: u32) -> String;

    // --- Memory ---
    fn inst_read_mem(&self, count: u32) -> String;
    fn inst_write_mem(&self, count: u32) -> String;

    // --- Hash ---
    fn inst_hash(&self) -> &'static str;
    fn inst_sponge_init(&self) -> &'static str;
    fn inst_sponge_absorb(&self) -> &'static str;
    fn inst_sponge_squeeze(&self) -> &'static str;
    fn inst_sponge_absorb_mem(&self) -> &'static str;

    // --- Merkle ---
    fn inst_merkle_step(&self) -> &'static str;
    fn inst_merkle_step_mem(&self) -> &'static str;

    // --- Control flow ---
    fn inst_assert(&self) -> &'static str;
    fn inst_assert_vector(&self) -> &'static str;
    fn inst_skiz(&self) -> &'static str;
    fn inst_call(&self, label: &str) -> String;
    fn inst_return(&self) -> &'static str;
    fn inst_recurse(&self) -> &'static str;
    fn inst_halt(&self) -> &'static str;

    /// Push the additive inverse of 1 (i.e., p − 1 in the field).
    /// Default: "push -1" (Triton assembler syntax).
    fn inst_push_neg_one(&self) -> &'static str {
        "push -1"
    }

    // --- STARK-specific (optional, default to hash) ---
    fn inst_xx_dot_step(&self) -> &'static str {
        "xx_dot_step"
    }
    fn inst_xb_dot_step(&self) -> &'static str {
        "xb_dot_step"
    }
}

pub(crate) use cairo::CairoBackend;
pub(crate) use miden::MidenBackend;
pub(crate) use openvm::OpenVMBackend;
pub(crate) use sp1::SP1Backend;
pub(crate) use triton::TritonBackend;

// ─── Backend Factory ──────────────────────────────────────────────

/// Create the appropriate backend for a target name.
pub(crate) fn create_backend(target_name: &str) -> Box<dyn StackBackend> {
    match target_name {
        "triton" => Box::new(TritonBackend),
        "miden" => Box::new(MidenBackend),
        "openvm" => Box::new(OpenVMBackend),
        "sp1" => Box::new(SP1Backend),
        "cairo" => Box::new(CairoBackend),
        _ => Box::new(TritonBackend), // fallback
    }
}
