//! Multi-element return cleanup for the TIR builder.

use crate::tir::TIROp;

use super::TIRBuilder;

impl TIRBuilder {
    /// Emit cleanup for multi-element returns: remove `dead` elements below
    /// the `ret_width`-wide return value at the top of the stack.
    ///
    /// When `dead` is a multiple of `ret_width` and `ret_width <= 15`, uses
    /// `swap K; pop 1` x M which rotates the return block by M positions --
    /// a multiple of K means the rotation cancels and the original order is
    /// preserved.
    ///
    /// When `ret_width > 15`, or when `ret_width > 5` and `dead` is not a
    /// multiple of `ret_width`, saves the return value to scratch RAM via
    /// element-by-element write_mem/read_mem, pops dead elements, and
    /// restores. This avoids emitting Swap(k) with k > 15, which exceeds
    /// Triton VM's maximum swap depth.
    pub(crate) fn emit_multi_ret_cleanup(&mut self, ret_width: u32, dead: u32) {
        let k = ret_width;
        if k <= 15 && dead % k == 0 {
            // Rotation-free: M removals = M/K full rotations.
            for _ in 0..dead {
                self.ops.push(TIROp::Swap(k));
                self.ops.push(TIROp::Pop(1));
            }
        } else if k <= 5 {
            // Bulk save to RAM, pop dead, bulk restore.
            // Uses write_mem K / read_mem K to avoid triggering spill elimination.
            let scratch = self.stack.alloc_scratch(k);
            // Move address below the K return elements.
            self.ops.push(TIROp::Push(scratch));
            for d in 1..=k {
                self.ops.push(TIROp::Swap(d));
            }
            // Write K elements: [val_K, ..., val_1, addr] → [addr+K]
            self.ops.push(TIROp::WriteMem(k));
            self.ops.push(TIROp::Pop(1));
            // Pop dead elements.
            self.emit_pop(dead);
            // Restore: read_mem K reads from [addr] → [val_1, ..., val_K, addr-K]
            self.ops.push(TIROp::Push(scratch + k as u64 - 1));
            self.ops.push(TIROp::ReadMem(k));
            self.ops.push(TIROp::Pop(1));
        } else if k <= 15 {
            // 6 <= K <= 15: element-by-element swap is safe since K <= 15.
            for _ in 0..dead {
                self.ops.push(TIROp::Swap(k));
                self.ops.push(TIROp::Pop(1));
            }
            let rotation = (k - (dead % k)) % k;
            for _ in 0..rotation {
                for d in 1..k {
                    self.ops.push(TIROp::Swap(d));
                }
            }
        } else {
            // K > 15: Swap(k) would exceed Triton VM's max swap depth of 15.
            // Save return values to scratch RAM element-by-element (using
            // only Swap(1)), pop dead elements, then restore from RAM.
            let scratch = self.stack.alloc_scratch(k);
            // Save: push starting address, then repeatedly swap and write.
            // Stack: [ret_0, ret_1, ..., ret_{k-1}, dead...]
            self.ops.push(TIROp::Push(scratch));
            // [addr, ret_0, ret_1, ..., ret_{k-1}, dead...]
            for _ in 0..k {
                self.ops.push(TIROp::Swap(1));
                self.ops.push(TIROp::WriteMem(1));
                // write_mem 1: [val, addr] -> [addr+1]
            }
            // [addr+k, dead...]
            self.ops.push(TIROp::Pop(1));
            // Pop dead elements.
            self.emit_pop(dead);
            // Restore in reverse order so ret_0 ends up on top.
            // Read from scratch+k-1 down to scratch.
            self.ops.push(TIROp::Push(scratch + k as u64 - 1));
            // [addr, ...]
            for _ in 0..k {
                self.ops.push(TIROp::ReadMem(1));
                // read_mem 1: [addr] -> [val, addr-1]
                self.ops.push(TIROp::Swap(1));
            }
            // [addr-1, ret_0, ret_1, ..., ret_{k-1}]
            self.ops.push(TIROp::Pop(1));
            // [ret_0, ret_1, ..., ret_{k-1}]
        }
    }
}
