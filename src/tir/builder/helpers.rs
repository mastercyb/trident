//! Stack wrappers, label generation, cfg helpers, and spill parser.

use crate::ast::*;
use crate::span::Spanned;
use crate::tir::TIROp;

use super::TIRBuilder;

// ─── Spill effect parser ──────────────────────────────────────────

/// Convert a SpillFormatter-produced instruction string into an TIROp.
///
/// The default SpillFormatter (Triton-style) emits lines like:
///   `"    push 42"`, `"    swap 5"`, `"    pop 1"`,
///   `"    write_mem 1"`, `"    read_mem 1"`.
pub(crate) fn parse_spill_effect(line: &str) -> TIROp {
    let trimmed = line.trim();

    if let Some(rest) = trimmed.strip_prefix("push ") {
        if let Ok(val) = rest.trim().parse::<u64>() {
            return TIROp::Push(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("swap ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return TIROp::Swap(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("pop ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return TIROp::Pop(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("write_mem ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return TIROp::WriteMem(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("read_mem ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return TIROp::ReadMem(val);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("dup ") {
        if let Ok(val) = rest.trim().parse::<u32>() {
            return TIROp::Dup(val);
        }
    }

    // Fallback: emit as inline ASM so nothing is silently lost.
    TIROp::Asm {
        lines: vec![trimmed.to_string()],
        effect: 0,
    }
}

// ─── TIRBuilder helpers ────────────────────────────────────────────

impl TIRBuilder {
    // ── Cfg helpers ───────────────────────────────────────────────

    pub(crate) fn is_cfg_active(&self, cfg: &Option<Spanned<String>>) -> bool {
        match cfg {
            None => true,
            Some(flag) => self.cfg_flags.contains(&flag.node),
        }
    }

    pub(crate) fn is_item_cfg_active(&self, item: &Item) -> bool {
        match item {
            Item::Fn(f) => self.is_cfg_active(&f.cfg),
            Item::Const(c) => self.is_cfg_active(&c.cfg),
            Item::Struct(s) => self.is_cfg_active(&s.cfg),
            Item::Event(e) => self.is_cfg_active(&e.cfg),
        }
    }

    // ── Label generation ──────────────────────────────────────────

    pub(crate) fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("{}__{}", prefix, self.label_counter)
    }

    // ── Stack effect flushing ─────────────────────────────────────

    pub(crate) fn flush_stack_effects(&mut self) {
        for inst in self.stack.drain_side_effects() {
            self.ops.push(parse_spill_effect(&inst));
        }
    }

    // ── Emit helpers ──────────────────────────────────────────────

    /// Ensure stack space, flush spill effects, push the TIROp, push temp to model.
    pub(crate) fn emit_and_push(&mut self, op: TIROp, result_width: u32) {
        if result_width > 0 {
            self.stack.ensure_space(result_width);
            self.flush_stack_effects();
        }
        self.ops.push(op);
        self.stack.push_temp(result_width);
    }

    /// Push an anonymous temporary onto the stack model (no TIROp emitted).
    pub(crate) fn push_temp(&mut self, width: u32) {
        self.stack.push_temp(width);
        self.flush_stack_effects();
    }

    /// Find depth of a named variable (may trigger reload if spilled).
    pub(crate) fn find_var_depth(&mut self, name: &str) -> u32 {
        let d = self.stack.find_var_depth(name);
        self.flush_stack_effects();
        d
    }

    /// Find depth and width of a named variable (may trigger reload if spilled).
    pub(crate) fn find_var_depth_and_width(&mut self, name: &str) -> Option<(u32, u32)> {
        let r = self.stack.find_var_depth_and_width(name);
        self.flush_stack_effects();
        r
    }

    /// Emit pop instructions in batches of up to 5.
    pub(crate) fn emit_pop(&mut self, n: u32) {
        let mut remaining = n;
        while remaining > 0 {
            let batch = remaining.min(5);
            self.ops.push(TIROp::Pop(batch));
            remaining -= batch;
        }
    }

    /// Build a block into a separate Vec<TIROp> by temporarily swapping out self.ops.
    pub(crate) fn build_block_as_ir(&mut self, block: &Block) -> Vec<TIROp> {
        let saved_ops = std::mem::take(&mut self.ops);
        self.build_block(block);
        let nested = std::mem::take(&mut self.ops);
        self.ops = saved_ops;
        nested
    }
}
