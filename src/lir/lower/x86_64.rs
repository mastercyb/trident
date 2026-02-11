//! x86-64 lowering — produces x86-64 machine code from LIR.
//!
//! Stub. The actual implementation will perform:
//! - Register allocation (virtual → physical: RAX, RBX, RCX, etc.)
//! - Instruction selection (LIROp → x86-64 encoding)
//! - Binary encoding (ModR/M, SIB, REX prefixes)

use super::RegisterLowering;
use crate::lir::LIROp;

pub struct X86_64Lowering;

impl X86_64Lowering {
    pub fn new() -> Self {
        Self
    }
}

impl RegisterLowering for X86_64Lowering {
    fn target_name(&self) -> &str {
        "x86_64"
    }

    fn lower(&self, _ops: &[LIROp]) -> Vec<u8> {
        todo!("x86-64 machine code emission")
    }
}
