//! RISC-V (RV64) lowering — produces RISC-V machine code from LIR.
//!
//! Stub. The actual implementation will perform:
//! - Register allocation (virtual → physical: x0–x31)
//! - Instruction selection (LIROp → RISC-V encoding)
//! - Binary encoding (32-bit base instruction words)
//!
//! This backend also serves SP1/OpenVM zkVMs (RISC-V based),
//! giving both conventional and provable execution from one lowering.

use super::RegisterLowering;
use crate::lir::LIROp;

pub struct RiscVLowering;

impl RiscVLowering {
    pub fn new() -> Self {
        Self
    }
}

impl RegisterLowering for RiscVLowering {
    fn target_name(&self) -> &str {
        "riscv64"
    }

    fn lower(&self, _ops: &[LIROp]) -> Vec<u8> {
        todo!("RISC-V machine code emission")
    }
}
