//! CUDA lowering — produces PTX/CUDA kernel source from TIR.
//!
//! Stub. The actual implementation will:
//! - Wrap scalar TIR program in a __global__ kernel function
//! - Map ReadIo/WriteIo to buffer[thread_id] accesses
//! - Map control flow to predicated execution (minimize divergence)
//! - Emit field arithmetic as inline device functions

use super::KernelLowering;
use crate::tir::TIROp;

pub struct CudaLowering;

impl CudaLowering {
    pub fn new() -> Self {
        Self
    }
}

impl KernelLowering for CudaLowering {
    fn target_name(&self) -> &str {
        "cuda"
    }

    fn lower(&self, _ops: &[TIROp]) -> String {
        todo!("CUDA kernel generation")
    }
}
