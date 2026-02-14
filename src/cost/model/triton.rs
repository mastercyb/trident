use super::{CostModel, TableCost};
use crate::ast::BinOp;

// ---------------------------------------------------------------------------
// TritonCostModel — Triton VM's 6-table cost model
// ---------------------------------------------------------------------------

/// Triton VM cost model with 6 Algebraic Execution Tables.
pub(crate) struct TritonCostModel;

impl TritonCostModel {
    /// Worst-case U32 table rows for 32-bit operations.
    const U32_WORST: u64 = 33;

    /// Simple arithmetic/logic op: 1 processor cycle, 1 op_stack row.
    const SIMPLE_OP: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: 0,
        op_stack: 1,
        ram: 0,
        jump_stack: 0,
    };

    /// U32-table op with stack effect.
    const U32_OP: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: Self::U32_WORST,
        op_stack: 1,
        ram: 0,
        jump_stack: 0,
    };

    /// U32-table op without stack growth.
    const U32_NOSTACK: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: Self::U32_WORST,
        op_stack: 0,
        ram: 0,
        jump_stack: 0,
    };

    /// Hash-table op with stack effect (6 hash rows for Tip5 permutation).
    const HASH_OP: TableCost = TableCost {
        processor: 1,
        hash: 6,
        u32_table: 0,
        op_stack: 1,
        ram: 0,
        jump_stack: 0,
    };

    /// Two-element assertion: 2 processor cycles, 2 op_stack rows.
    const ASSERT2: TableCost = TableCost {
        processor: 2,
        hash: 0,
        u32_table: 0,
        op_stack: 2,
        ram: 0,
        jump_stack: 0,
    };

    /// Single RAM read/write: 2 processor cycles, 2 op_stack, 1 ram.
    const RAM_RW: TableCost = TableCost {
        processor: 2,
        hash: 0,
        u32_table: 0,
        op_stack: 2,
        ram: 1,
        jump_stack: 0,
    };

    /// Block RAM read/write: 2 processor cycles, 2 op_stack, 5 ram.
    const RAM_BLOCK_RW: TableCost = TableCost {
        processor: 2,
        hash: 0,
        u32_table: 0,
        op_stack: 2,
        ram: 5,
        jump_stack: 0,
    };

    /// Pure processor op (no stack/ram/hash effect): 1 processor cycle only.
    const PURE_PROC: TableCost = TableCost {
        processor: 1,
        hash: 0,
        u32_table: 0,
        op_stack: 0,
        ram: 0,
        jump_stack: 0,
    };
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
            "neg" => TableCost {
                processor: 2,
                hash: 0,
                u32_table: 0,
                op_stack: 1,
                ram: 0,
                jump_stack: 0,
            },
            "sub" => TableCost {
                processor: 3,
                hash: 0,
                u32_table: 0,
                op_stack: 2,
                ram: 0,
                jump_stack: 0,
            },

            // U32 ops
            "split" => Self::U32_OP,
            "log2" => Self::U32_NOSTACK,
            "pow" => Self::U32_OP,
            "popcount" => Self::U32_NOSTACK,

            // Hash ops (6 hash table rows each for Tip5 permutation)
            "hash" => Self::HASH_OP,
            "sponge_init" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            "sponge_absorb" => Self::HASH_OP,
            "sponge_squeeze" => Self::HASH_OP,
            "sponge_absorb_mem" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: 0,
                op_stack: 1,
                ram: 10,
                jump_stack: 0,
            },

            // Merkle
            "merkle_step" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: Self::U32_WORST,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            "merkle_step_mem" => TableCost {
                processor: 1,
                hash: 6,
                u32_table: Self::U32_WORST,
                op_stack: 0,
                ram: 5,
                jump_stack: 0,
            },

            // RAM
            "ram_read" => Self::RAM_RW,
            "ram_write" => Self::RAM_RW,
            "ram_read_block" => Self::RAM_BLOCK_RW,
            "ram_write_block" => Self::RAM_BLOCK_RW,

            // Dot steps
            "xx_dot_step" => TableCost {
                processor: 1,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 6,
                jump_stack: 0,
            },
            "xb_dot_step" => TableCost {
                processor: 1,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 4,
                jump_stack: 0,
            },

            // Conversions
            "as_u32" => TableCost {
                processor: 2,
                hash: 0,
                u32_table: Self::U32_WORST,
                op_stack: 1,
                ram: 0,
                jump_stack: 0,
            },
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
        TableCost {
            processor: 2,
            hash: 0,
            u32_table: 0,
            op_stack: 0,
            ram: 0,
            jump_stack: 2,
        }
    }

    fn stack_op(&self) -> TableCost {
        TableCost {
            processor: 1,
            hash: 0,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        }
    }

    fn if_overhead(&self) -> TableCost {
        TableCost {
            processor: 3,
            hash: 0,
            u32_table: 0,
            op_stack: 2,
            ram: 0,
            jump_stack: 1,
        }
    }

    fn loop_overhead(&self) -> TableCost {
        TableCost {
            processor: 8,
            hash: 0,
            u32_table: 0,
            op_stack: 4,
            ram: 0,
            jump_stack: 1,
        }
    }

    fn hash_rows_per_permutation(&self) -> u64 {
        6
    }

    fn target_name(&self) -> &str {
        "Triton VM"
    }
}
