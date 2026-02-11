//! Lowering: consumes `Vec<IROp>` and produces target assembly text.
//!
//! Each target implements `Lowering` to control instruction selection
//! and control-flow structure.

mod miden;
#[cfg(test)]
mod tests;
mod triton;

use super::IROp;

pub use miden::MidenLowering;
pub use triton::TritonLowering;

/// Lowers IR operations into target assembly lines.
pub trait Lowering {
    /// Convert a sequence of IR operations into assembly text lines.
    fn lower(&self, ops: &[IROp]) -> Vec<String>;
}

/// Create a lowering backend for the given target name.
pub fn create_lowering(target: &str) -> Box<dyn Lowering> {
    match target {
        "miden" => Box::new(MidenLowering::new()),
        _ => Box::new(TritonLowering::new()),
    }
}
