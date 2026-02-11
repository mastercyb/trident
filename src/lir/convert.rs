//! TIR → LIR conversion pass.
//!
//! Transforms stack-based TIR into register-based LIR by:
//! 1. Simulating the TIR operand stack
//! 2. Assigning virtual registers for each stack position
//! 3. Flattening nested control flow (IfElse/IfOnly/Loop) into
//!    labels + branches + jumps
//!
//! The algorithm is not yet implemented — only the types and helpers.

use super::{LIROp, Label, Reg};
use crate::tir::TIROp;

/// Convert a sequence of TIR operations into LIR operations.
///
/// The conversion simulates the TIR operand stack, assigning a fresh
/// virtual register for each value produced. Nested structural control
/// flow (`IfElse`, `IfOnly`, `Loop`) is flattened into `Branch`, `Jump`,
/// and `LabelDef` operations.
///
/// # Example (conceptual)
///
/// ```text
/// TIR:                    LIR:
///   Push(10)        →      LoadImm(v0, 10)
///   Push(20)        →      LoadImm(v1, 20)
///   Add             →      Add(v2, v0, v1)
///   WriteIo(1)      →      WriteIo { src: v2, count: 1 }
/// ```
pub fn tir_to_lir(_ops: &[TIROp]) -> Vec<LIROp> {
    todo!("TIR→LIR conversion: simulate stack, assign virtual registers, flatten control flow")
}

/// State for the TIR→LIR conversion pass.
///
/// Tracks the virtual register stack (simulating TIR's operand stack)
/// and generates fresh labels for flattened control flow.
#[allow(dead_code)]
pub(crate) struct ConvertCtx {
    /// Next virtual register number.
    next_reg: u32,
    /// Simulated stack: each entry is a virtual register holding the value
    /// at that stack position.
    stack: Vec<Reg>,
    /// Next label counter for flattened control flow.
    next_label: u32,
    /// Accumulated LIR output.
    out: Vec<LIROp>,
}

#[allow(dead_code)]
impl ConvertCtx {
    pub fn new() -> Self {
        Self {
            next_reg: 0,
            stack: Vec::new(),
            next_label: 0,
            out: Vec::new(),
        }
    }

    /// Allocate a fresh virtual register.
    pub fn fresh_reg(&mut self) -> Reg {
        let r = Reg(self.next_reg);
        self.next_reg += 1;
        r
    }

    /// Generate a fresh label with the given prefix.
    pub fn fresh_label(&mut self, prefix: &str) -> Label {
        self.next_label += 1;
        Label::new(format!("{}{}", prefix, self.next_label))
    }

    /// Push a virtual register onto the simulated stack.
    pub fn push(&mut self, reg: Reg) {
        self.stack.push(reg);
    }

    /// Pop a virtual register from the simulated stack.
    pub fn pop(&mut self) -> Reg {
        self.stack
            .pop()
            .expect("ConvertCtx::pop called on empty stack")
    }

    /// Peek at the register at the given depth (0 = top of stack).
    pub fn peek(&self, depth: u32) -> Reg {
        let idx = self.stack.len() - 1 - depth as usize;
        self.stack[idx]
    }

    /// Emit an LIR operation.
    pub fn emit(&mut self, op: LIROp) {
        self.out.push(op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_ctx_fresh_reg() {
        let mut ctx = ConvertCtx::new();
        assert_eq!(ctx.fresh_reg(), Reg(0));
        assert_eq!(ctx.fresh_reg(), Reg(1));
        assert_eq!(ctx.fresh_reg(), Reg(2));
    }

    #[test]
    fn test_convert_ctx_fresh_label() {
        let mut ctx = ConvertCtx::new();
        assert_eq!(ctx.fresh_label("then_"), Label::new("then_1"));
        assert_eq!(ctx.fresh_label("else_"), Label::new("else_2"));
    }

    #[test]
    fn test_convert_ctx_stack() {
        let mut ctx = ConvertCtx::new();
        let r0 = ctx.fresh_reg();
        let r1 = ctx.fresh_reg();
        ctx.push(r0);
        ctx.push(r1);
        assert_eq!(ctx.peek(0), r1);
        assert_eq!(ctx.peek(1), r0);
        assert_eq!(ctx.pop(), r1);
        assert_eq!(ctx.pop(), r0);
    }
}
