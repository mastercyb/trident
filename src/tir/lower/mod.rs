//! StackLowering: consumes `Vec<TIROp>` and produces target assembly text.
//!
//! Each target implements `StackLowering` to control instruction selection
//! and control-flow structure.

#[cfg(test)]
mod tests;
mod triton;

use super::TIROp;

pub use triton::TritonLowering;

/// Lowers IR operations into target assembly lines.
pub trait StackLowering {
    /// Convert a sequence of IR operations into assembly text lines.
    fn lower(&self, ops: &[TIROp]) -> Vec<String>;
}

/// Create a stack lowering backend for the given target name.
pub fn create_stack_lowering(_target: &str) -> Box<dyn StackLowering> {
    Box::new(TritonLowering::new())
}
