pub mod triton;

use crate::ast::BinOp;

pub(crate) use triton::TritonCostModel;

// ---------------------------------------------------------------------------
// CostModel trait — target-agnostic cost interface
// ---------------------------------------------------------------------------

/// Maximum number of cost tables any target can have.
pub const MAX_TABLES: usize = 8;

/// Trait for target-specific cost models.
///
/// Each target VM implements this to provide table names, per-instruction
/// costs, and formatting for cost reports. The cost analyzer delegates all
/// target-specific knowledge through this trait.
#[allow(dead_code)]
pub(crate) trait CostModel {
    /// Names of the execution tables (e.g. ["processor", "hash", "u32", ...]).
    fn table_names(&self) -> &[&str];

    /// Short display names for compact annotations (e.g. ["cc", "hash", "u32", ...]).
    fn table_short_names(&self) -> &[&str];

    /// Number of active tables for this target.
    fn table_count(&self) -> u8 {
        self.table_names().len() as u8
    }

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
// TableCost — target-generic cost vector
// ---------------------------------------------------------------------------

/// Cost across execution tables. Fixed-size array indexed by table position
/// as defined by the target's CostModel. Table names are external metadata,
/// not baked into this struct.
#[derive(Clone, Debug)]
pub struct TableCost {
    /// Cost values indexed by table position.
    pub values: [u64; MAX_TABLES],
    /// Number of active tables (from CostModel::table_count()).
    pub count: u8,
}

impl Default for TableCost {
    fn default() -> Self {
        Self::ZERO
    }
}

impl TableCost {
    pub const ZERO: TableCost = TableCost {
        values: [0; MAX_TABLES],
        count: 0,
    };

    /// Build from a slice of values (used by CostModel implementations).
    pub fn from_slice(vals: &[u64]) -> TableCost {
        let mut values = [0u64; MAX_TABLES];
        let n = vals.len().min(MAX_TABLES);
        values[..n].copy_from_slice(&vals[..n]);
        TableCost {
            values,
            count: n as u8,
        }
    }

    /// Get value at table index.
    pub fn get(&self, i: usize) -> u64 {
        self.values[i]
    }

    /// Check if any table has non-zero cost.
    pub fn is_nonzero(&self) -> bool {
        let n = self.count as usize;
        self.values[..n].iter().any(|&v| v > 0)
    }

    pub fn add(&self, other: &TableCost) -> TableCost {
        let n = self.count.max(other.count) as usize;
        let mut values = [0u64; MAX_TABLES];
        for i in 0..n {
            values[i] = self.values[i] + other.values[i];
        }
        TableCost {
            values,
            count: n as u8,
        }
    }

    pub fn scale(&self, factor: u64) -> TableCost {
        let n = self.count as usize;
        let mut values = [0u64; MAX_TABLES];
        for i in 0..n {
            values[i] = self.values[i].saturating_mul(factor);
        }
        TableCost {
            values,
            count: self.count,
        }
    }

    pub fn max(&self, other: &TableCost) -> TableCost {
        let n = self.count.max(other.count) as usize;
        let mut values = [0u64; MAX_TABLES];
        for i in 0..n {
            values[i] = self.values[i].max(other.values[i]);
        }
        TableCost {
            values,
            count: n as u8,
        }
    }

    /// The maximum height across all active tables.
    pub fn max_height(&self) -> u64 {
        let n = self.count as usize;
        self.values[..n].iter().copied().max().unwrap_or(0)
    }

    /// Which table is the tallest, by short name.
    pub fn dominant_table<'a>(&self, short_names: &[&'a str]) -> &'a str {
        let n = self.count as usize;
        if n == 0 || short_names.is_empty() {
            return "?";
        }
        let max = self.max_height();
        if max == 0 {
            return short_names[0];
        }
        for i in 0..n.min(short_names.len()) {
            if self.values[i] == max {
                return short_names[i];
            }
        }
        short_names[0]
    }

    /// Serialize to a JSON object string using the given table names as keys.
    pub fn to_json_value(&self, names: &[&str]) -> String {
        let n = self.count as usize;
        let mut parts = Vec::new();
        for i in 0..n.min(names.len()) {
            parts.push(format!("\"{}\": {}", names[i], self.values[i]));
        }
        format!("{{{}}}", parts.join(", "))
    }

    /// Deserialize from a JSON object string using the given table names as keys.
    pub fn from_json_value(s: &str, names: &[&str]) -> Option<TableCost> {
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

        let mut values = [0u64; MAX_TABLES];
        for (i, name) in names.iter().enumerate() {
            values[i] = extract_u64(s, name)?;
        }
        Some(TableCost {
            values,
            count: names.len() as u8,
        })
    }

    /// Format a compact annotation string showing non-zero cost fields.
    pub fn format_annotation(&self, short_names: &[&str]) -> String {
        let n = self.count as usize;
        let mut parts = Vec::new();
        for i in 0..n.min(short_names.len()) {
            if self.values[i] > 0 {
                parts.push(format!("{}={}", short_names[i], self.values[i]));
            }
        }
        parts.join(" ")
    }
}

/// Look up builtin cost using a named target's cost model.
pub(crate) fn cost_builtin(target: &str, name: &str) -> TableCost {
    create_cost_model(target).builtin_cost(name)
}

/// Select the cost model for a given target name.
pub(crate) fn create_cost_model(target_name: &str) -> &'static dyn CostModel {
    match target_name {
        "triton" => &TritonCostModel,
        _ => &TritonCostModel, // fallback until other models are implemented
    }
}
