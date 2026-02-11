//! Vulkan lowering — produces SPIR-V compute shaders from TIR.
//!
//! Stub. The actual implementation will:
//! - Wrap scalar TIR program in a SPIR-V compute shader
//! - Map ReadIo/WriteIo to storage buffer accesses via gl_GlobalInvocationID
//! - Emit field arithmetic as SPIR-V instructions
//! - Cross-platform GPU target (NVIDIA, AMD, Intel, mobile)

use super::KernelLowering;
use crate::tir::TIROp;

pub struct VulkanLowering;

impl VulkanLowering {
    pub fn new() -> Self {
        Self
    }
}

impl KernelLowering for VulkanLowering {
    fn target_name(&self) -> &str {
        "vulkan"
    }

    fn lower(&self, _ops: &[TIROp]) -> String {
        todo!("SPIR-V compute shader generation")
    }
}
