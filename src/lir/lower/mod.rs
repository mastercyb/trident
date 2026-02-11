//! RegisterLowering: consumes `&[LIROp]` and produces target machine code.
//!
//! Each register-machine target implements `RegisterLowering` to perform
//! instruction selection, register allocation, and binary encoding.
//!
//! This is the register-machine counterpart of `tir::lower::Lowering`,
//! which produces assembly text for stack machines.

mod arm64;
mod riscv;
mod x86_64;

use super::LIROp;

pub use arm64::Arm64Lowering;
pub use riscv::RiscVLowering;
pub use x86_64::X86_64Lowering;

/// Lowers LIR operations into target machine code (binary).
pub trait RegisterLowering {
    /// The target name (e.g. "x86_64", "arm64", "riscv64").
    fn target_name(&self) -> &str;

    /// Lower a sequence of LIR operations into machine code bytes.
    fn lower(&self, ops: &[LIROp]) -> Vec<u8>;

    /// Lower to assembly text for debugging. Default uses Display.
    fn lower_text(&self, ops: &[LIROp]) -> Vec<String> {
        ops.iter().map(|op| format!("{}", op)).collect()
    }
}

/// Create a register-lowering backend for the given target name.
pub fn create_register_lowering(target: &str) -> Option<Box<dyn RegisterLowering>> {
    match target {
        "x86_64" | "x86-64" => Some(Box::new(X86_64Lowering::new())),
        "arm64" | "aarch64" => Some(Box::new(Arm64Lowering::new())),
        "riscv" | "riscv64" => Some(Box::new(RiscVLowering::new())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_register_lowering() {
        assert!(create_register_lowering("x86_64").is_some());
        assert!(create_register_lowering("x86-64").is_some());
        assert!(create_register_lowering("arm64").is_some());
        assert!(create_register_lowering("aarch64").is_some());
        assert!(create_register_lowering("riscv").is_some());
        assert!(create_register_lowering("riscv64").is_some());
        assert!(create_register_lowering("triton").is_none());
        assert!(create_register_lowering("miden").is_none());
    }

    #[test]
    fn test_target_names() {
        let x86 = create_register_lowering("x86_64").unwrap();
        assert_eq!(x86.target_name(), "x86_64");

        let arm = create_register_lowering("arm64").unwrap();
        assert_eq!(arm.target_name(), "arm64");

        let rv = create_register_lowering("riscv").unwrap();
        assert_eq!(rv.target_name(), "riscv64");
    }

    #[test]
    fn test_lower_text_default() {
        use crate::lir::Reg;

        let lowering = X86_64Lowering::new();
        let ops = vec![
            LIROp::LoadImm(Reg(0), 42),
            LIROp::Add(Reg(2), Reg(0), Reg(1)),
        ];
        let text = lowering.lower_text(&ops);
        assert_eq!(text[0], "li v0, 42");
        assert_eq!(text[1], "add v2, v0, v1");
    }
}
