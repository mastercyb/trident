//! KernelLowering: wraps scalar TIR programs into GPU compute kernels.
//!
//! Each GPU target implements `KernelLowering` to emit kernel source
//! that runs N instances of the same Trident program in parallel.
//! The program itself stays scalar — parallelism is across instances,
//! not within a single execution.
//!
//! This is the data-parallel counterpart of:
//! - `tir::lower::Lowering` — stack targets → assembly text
//! - `lir::lower::RegisterLowering` — register targets → machine code

mod cuda;
mod metal;
mod vulkan;

use crate::tir::TIROp;

pub use cuda::CudaLowering;
pub use metal::MetalLowering;
pub use vulkan::VulkanLowering;

/// Lowers TIR operations into a GPU compute kernel (source text).
///
/// The kernel wraps one Trident program for batch execution:
/// each GPU thread runs one instance with its own inputs/outputs.
pub trait KernelLowering {
    /// The target name (e.g. "cuda", "metal", "vulkan").
    fn target_name(&self) -> &str;

    /// Lower a scalar TIR program into GPU kernel source code.
    /// The returned string is a complete, compilable kernel.
    fn lower(&self, ops: &[TIROp]) -> String;
}

/// Create a kernel-lowering backend for the given target name.
pub fn create_kernel_lowering(target: &str) -> Option<Box<dyn KernelLowering>> {
    match target {
        "cuda" | "ptx" => Some(Box::new(CudaLowering::new())),
        "metal" | "msl" => Some(Box::new(MetalLowering::new())),
        "vulkan" | "spirv" | "spir-v" => Some(Box::new(VulkanLowering::new())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_kernel_lowering() {
        assert!(create_kernel_lowering("cuda").is_some());
        assert!(create_kernel_lowering("ptx").is_some());
        assert!(create_kernel_lowering("metal").is_some());
        assert!(create_kernel_lowering("msl").is_some());
        assert!(create_kernel_lowering("vulkan").is_some());
        assert!(create_kernel_lowering("spirv").is_some());
        assert!(create_kernel_lowering("spir-v").is_some());
        assert!(create_kernel_lowering("triton").is_none());
        assert!(create_kernel_lowering("x86_64").is_none());
    }

    #[test]
    fn test_target_names() {
        let cuda = create_kernel_lowering("cuda").unwrap();
        assert_eq!(cuda.target_name(), "cuda");

        let metal = create_kernel_lowering("metal").unwrap();
        assert_eq!(metal.target_name(), "metal");

        let vulkan = create_kernel_lowering("vulkan").unwrap();
        assert_eq!(vulkan.target_name(), "vulkan");
    }
}
