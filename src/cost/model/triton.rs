use super::{CostModel, TableCost};
use crate::ast::BinOp;

// ---------------------------------------------------------------------------
// TritonCostModel — Triton VM's 6-table cost model
// ---------------------------------------------------------------------------

/// Triton VM cost model with 6 Algebraic Execution Tables.
pub(crate) struct TritonCostModel;

/// Number of active tables for Triton VM.
const N: u8 = 6;

// Table indices for Triton VM.
// [0]=processor, [1]=hash, [2]=u32, [3]=op_stack, [4]=ram, [5]=jump_stack
const _PROC: usize = 0;
const _HASH: usize = 1;
const _U32: usize = 2;
const _OPST: usize = 3;
const _RAM: usize = 4;
const _JUMP: usize = 5;

/// Build a Triton TableCost from a 6-element array.
const fn tc(v: [u64; 6]) -> TableCost {
    TableCost {
        values: [v[0], v[1], v[2], v[3], v[4], v[5], 0, 0],
        count: N,
    }
}

impl TritonCostModel {
    /// Worst-case U32 table rows for 32-bit operations.
    const U32_WORST: u64 = 33;

    //                              proc  hash  u32   opst  ram   jump
    const SIMPLE_OP: TableCost = tc([1, 0, 0, 1, 0, 0]);
    const U32_OP: TableCost = tc([1, 0, 33, 1, 0, 0]);
    const U32_NOSTACK: TableCost = tc([1, 0, 33, 0, 0, 0]);
    const HASH_OP: TableCost = tc([1, 6, 0, 1, 0, 0]);
    const ASSERT2: TableCost = tc([2, 0, 0, 2, 0, 0]);
    const RAM_RW: TableCost = tc([2, 0, 0, 2, 1, 0]);
    const RAM_BLOCK_RW: TableCost = tc([2, 0, 0, 2, 5, 0]);
    const PURE_PROC: TableCost = tc([1, 0, 0, 0, 0, 0]);
}

impl CostModel for TritonCostModel {
    fn table_names(&self) -> &[&str] {
        &["processor", "hash", "u32", "op_stack", "ram", "jump_stack"]
    }

    fn table_short_names(&self) -> &[&str] {
        &["cc", "hash", "u32", "opst", "ram", "jump"]
    }

    fn builtin_cost(&self, name: &str) -> TableCost {
        match name {
            // I/O
            "pub_read" | "pub_read2" | "pub_read3" | "pub_read4" | "pub_read5" => Self::SIMPLE_OP,
            "pub_write" | "pub_write2" | "pub_write3" | "pub_write4" | "pub_write5" => {
                Self::SIMPLE_OP
            }

            // Non-deterministic input
            "divine" | "divine3" | "divine5" => Self::SIMPLE_OP,

            // Assertions
            "assert" => Self::SIMPLE_OP,
            "assert_eq" => Self::ASSERT2,
            "assert_digest" => Self::ASSERT2,

            // Field ops
            "field_add" => Self::SIMPLE_OP,
            "field_mul" => Self::SIMPLE_OP,
            "inv" => Self::PURE_PROC,
            "neg" => tc([2, 0, 0, 1, 0, 0]),
            "sub" => tc([3, 0, 0, 2, 0, 0]),

            // U32 ops
            "split" => Self::U32_OP,
            "log2" => Self::U32_NOSTACK,
            "pow" => Self::U32_OP,
            "popcount" => Self::U32_NOSTACK,

            // Hash ops (6 hash table rows each for Tip5 permutation)
            "hash" => Self::HASH_OP,
            "sponge_init" => tc([1, 6, 0, 0, 0, 0]),
            "sponge_absorb" => Self::HASH_OP,
            "sponge_squeeze" => Self::HASH_OP,
            "sponge_absorb_mem" => tc([1, 6, 0, 1, 10, 0]),

            // Merkle
            "merkle_step" => tc([1, 6, Self::U32_WORST, 0, 0, 0]),
            "merkle_step_mem" => tc([1, 6, Self::U32_WORST, 0, 5, 0]),

            // RAM
            "ram_read" => Self::RAM_RW,
            "ram_write" => Self::RAM_RW,
            "ram_read_block" => Self::RAM_BLOCK_RW,
            "ram_write_block" => Self::RAM_BLOCK_RW,

            // Dot steps
            "xx_dot_step" => tc([1, 0, 0, 0, 6, 0]),
            "xb_dot_step" => tc([1, 0, 0, 0, 4, 0]),

            // Conversions
            "as_u32" => tc([2, 0, Self::U32_WORST, 1, 0, 0]),
            "as_field" => TableCost::ZERO,

            // XField
            "xfield" => TableCost::ZERO,
            "xinvert" => Self::PURE_PROC,

            _ => TableCost::ZERO,
        }
    }

    fn binop_cost(&self, op: &BinOp) -> TableCost {
        match op {
            BinOp::Add => Self::SIMPLE_OP,
            BinOp::Mul => Self::SIMPLE_OP,
            BinOp::Eq => Self::SIMPLE_OP,
            BinOp::Lt => Self::U32_OP,
            BinOp::BitAnd => Self::U32_OP,
            BinOp::BitXor => Self::U32_OP,
            BinOp::DivMod => Self::U32_NOSTACK,
            BinOp::XFieldMul => Self::SIMPLE_OP,
        }
    }

    fn call_overhead(&self) -> TableCost {
        tc([2, 0, 0, 0, 0, 2])
    }

    fn stack_op(&self) -> TableCost {
        tc([1, 0, 0, 1, 0, 0])
    }

    fn if_overhead(&self) -> TableCost {
        tc([3, 0, 0, 2, 0, 1])
    }

    fn loop_overhead(&self) -> TableCost {
        tc([8, 0, 0, 4, 0, 1])
    }

    fn hash_rows_per_permutation(&self) -> u64 {
        6
    }

    fn target_name(&self) -> &str {
        "Triton VM"
    }
}
