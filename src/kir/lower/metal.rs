//! Metal lowering — produces Metal Shading Language compute kernels from TIR.
//!
//! Stub. The actual implementation will:
//! - Wrap scalar TIR program in a kernel function
//! - Map ReadIo/WriteIo to buffer[thread_position_in_grid] accesses
//! - Emit field arithmetic as inline Metal functions
//! - Target Apple Silicon GPUs

use super::KernelLowering;
use crate::tir::TIROp;

pub struct MetalLowering;

impl MetalLowering {
    pub fn new() -> Self {
        Self
    }
}

impl KernelLowering for MetalLowering {
    fn target_name(&self) -> &str {
        "metal"
    }

    fn lower(&self, _ops: &[TIROp]) -> String {
        todo!("Metal compute kernel generation")
    }
}
