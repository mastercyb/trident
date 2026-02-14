pub mod triton;

use crate::ast::BinOp;

pub(crate) use triton::TritonCostModel;

// ---------------------------------------------------------------------------
// CostModel trait — target-agnostic cost interface
// ---------------------------------------------------------------------------

/// Trait for target-specific cost models.
///
/// Each target VM implements this to provide table names, per-instruction
/// costs, and formatting for cost reports. The cost analyzer delegates all
/// target-specific knowledge through this trait.
pub(crate) trait CostModel {
    /// Names of the execution tables (e.g. ["processor", "hash", "u32", ...]).
    fn table_names(&self) -> &[&str];

    /// Short display names for compact annotations (e.g. ["cc", "hash", "u32", ...]).
    fn table_short_names(&self) -> &[&str];

    /// Cost of a builtin function call by name.
    fn builtin_cost(&self, name: &str) -> TableCost;

    /// Cost of a binary operation.
    fn binop_cost(&self, op: &BinOp) -> TableCost;

    /// Overhead cost for a function call/return pair.
    fn call_overhead(&self) -> TableCost;

    /// Cost of a single stack manipulation (push/dup/swap).
    fn stack_op(&self) -> TableCost;

    /// Overhead cost for an if/else branch.
    fn if_overhead(&self) -> TableCost;

    /// Overhead cost per loop iteration.
    fn loop_overhead(&self) -> TableCost;

    /// Number of hash table rows per hash permutation.
    fn hash_rows_per_permutation(&self) -> u64;

    /// Target display name for reports.
    fn target_name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// TableCost — per-table cost vector
// ---------------------------------------------------------------------------

/// Cost across all 6 Triton VM tables.
#[derive(Clone, Debug, Default)]
pub struct TableCost {
    /// Processor Table rows (= clock cycles).
    pub processor: u64,
    /// Hash Table rows (6 per hash operation).
    pub hash: u64,
    /// U32 Table rows (variable, worst-case 32-bit estimates).
    pub u32_table: u64,
    /// Op Stack Table rows.
    pub op_stack: u64,
    /// RAM Table rows.
    pub ram: u64,
    /// Jump Stack Table rows.
    pub jump_stack: u64,
}

impl TableCost {
    pub const ZERO: TableCost = TableCost {
        processor: 0,
        hash: 0,
        u32_table: 0,
        op_stack: 0,
        ram: 0,
        jump_stack: 0,
    };

    pub fn add(&self, other: &TableCost) -> TableCost {
        TableCost {
            processor: self.processor + other.processor,
            hash: self.hash + other.hash,
            u32_table: self.u32_table + other.u32_table,
            op_stack: self.op_stack + other.op_stack,
            ram: self.ram + other.ram,
            jump_stack: self.jump_stack + other.jump_stack,
        }
    }

    pub fn scale(&self, factor: u64) -> TableCost {
        TableCost {
            processor: self.processor.saturating_mul(factor),
            hash: self.hash.saturating_mul(factor),
            u32_table: self.u32_table.saturating_mul(factor),
            op_stack: self.op_stack.saturating_mul(factor),
            ram: self.ram.saturating_mul(factor),
            jump_stack: self.jump_stack.saturating_mul(factor),
        }
    }

    pub fn max(&self, other: &TableCost) -> TableCost {
        TableCost {
            processor: self.processor.max(other.processor),
            hash: self.hash.max(other.hash),
            u32_table: self.u32_table.max(other.u32_table),
            op_stack: self.op_stack.max(other.op_stack),
            ram: self.ram.max(other.ram),
            jump_stack: self.jump_stack.max(other.jump_stack),
        }
    }

    /// The maximum height across all tables.
    pub fn max_height(&self) -> u64 {
        self.processor
            .max(self.hash)
            .max(self.u32_table)
            .max(self.op_stack)
            .max(self.ram)
            .max(self.jump_stack)
    }

    /// Which table is the tallest.
    pub fn dominant_table(&self) -> &'static str {
        let max = self.max_height();
        if max == 0 {
            return "proc";
        }
        if self.hash == max {
            "hash"
        } else if self.u32_table == max {
            "u32"
        } else if self.ram == max {
            "ram"
        } else if self.processor == max {
            "proc"
        } else if self.op_stack == max {
            "opstack"
        } else {
            "jump"
        }
    }

    /// Serialize to a JSON object string.
    pub fn to_json_value(&self) -> String {
        format!(
            "{{\"processor\": {}, \"hash\": {}, \"u32_table\": {}, \"op_stack\": {}, \"ram\": {}, \"jump_stack\": {}}}",
            self.processor, self.hash, self.u32_table, self.op_stack, self.ram, self.jump_stack
        )
    }

    /// Deserialize from a JSON object string.
    pub fn from_json_value(s: &str) -> Option<TableCost> {
        fn extract_u64(s: &str, key: &str) -> Option<u64> {
            let needle = format!("\"{}\"", key);
            let idx = s.find(&needle)?;
            let rest = &s[idx + needle.len()..];
            let colon = rest.find(':')?;
            let after_colon = rest[colon + 1..].trim_start();
            let end = after_colon
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_colon.len());
            after_colon[..end].parse().ok()
        }

        Some(TableCost {
            processor: extract_u64(s, "processor")?,
            hash: extract_u64(s, "hash")?,
            u32_table: extract_u64(s, "u32_table")?,
            op_stack: extract_u64(s, "op_stack")?,
            ram: extract_u64(s, "ram")?,
            jump_stack: extract_u64(s, "jump_stack")?,
        })
    }

    /// Format a compact annotation string showing non-zero cost fields.
    pub fn format_annotation(&self) -> String {
        let mut parts = Vec::new();
        if self.processor > 0 {
            parts.push(format!("cc={}", self.processor));
        }
        if self.hash > 0 {
            parts.push(format!("hash={}", self.hash));
        }
        if self.u32_table > 0 {
            parts.push(format!("u32={}", self.u32_table));
        }
        if self.op_stack > 0 {
            parts.push(format!("opst={}", self.op_stack));
        }
        if self.ram > 0 {
            parts.push(format!("ram={}", self.ram));
        }
        if self.jump_stack > 0 {
            parts.push(format!("jump={}", self.jump_stack));
        }
        parts.join(" ")
    }
}

/// Convenience function: look up builtin cost using the default Triton cost model.
/// Used by LSP and other callers that don't have a CostModel reference.
pub(crate) fn cost_builtin(name: &str) -> TableCost {
    TritonCostModel.builtin_cost(name)
}
