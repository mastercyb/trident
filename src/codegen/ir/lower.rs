//! Lowering: consumes `Vec<IROp>` and produces target assembly text.
//!
//! Each target implements `Lowering` to control instruction selection
//! and control-flow structure. TritonLowering will be implemented in Phase 3.

#![allow(dead_code)]

use super::IROp;

/// Lowers IR operations into target assembly lines.
pub trait Lowering {
    /// Convert a sequence of IR operations into assembly text lines.
    fn lower(&self, ops: &[IROp]) -> Vec<String>;
}
