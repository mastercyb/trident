//! ARM64 (AArch64) lowering — produces ARM64 machine code from LIR.
//!
//! Stub. The actual implementation will perform:
//! - Register allocation (virtual → physical: X0–X30, SP)
//! - Instruction selection (LIROp → ARM64 encoding)
//! - Binary encoding (fixed 32-bit instruction words)

use super::RegisterLowering;
use crate::lir::LIROp;

pub struct Arm64Lowering;

impl Arm64Lowering {
    pub fn new() -> Self {
        Self
    }
}

impl RegisterLowering for Arm64Lowering {
    fn target_name(&self) -> &str {
        "arm64"
    }

    fn lower(&self, _ops: &[LIROp]) -> Vec<u8> {
        todo!("ARM64 machine code emission")
    }
}
