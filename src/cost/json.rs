use std::path::Path;

use super::analyzer::{FunctionCost, ProgramCost};
use super::model::TableCost;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

// --- Helpers ---

/// Find the index of the matching closing brace for a `{` at position `start`.
fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes[start..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

// --- Report formatting ---

impl ProgramCost {
    pub fn to_json(&self) -> String {
        let names = self.long_names();
        let mut out = String::new();
        out.push_str("{\n  \"functions\": {\n");
        for (i, func) in self.functions.iter().enumerate() {
            out.push_str(&format!(
                "    \"{}\": {}",
                func.name,
                func.cost.to_json_value(&names)
            ));
            if i + 1 < self.functions.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  },\n");
        out.push_str(&format!(
            "  \"total\": {},\n",
            self.total.to_json_value(&names)
        ));
        out.push_str(&format!("  \"padded_height\": {}\n", self.padded_height));
        out.push_str("}\n");
        out
    }

    /// Save cost analysis to a JSON file.
    pub fn save_json(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json())
            .map_err(|e| format!("cannot write '{}': {}", path.display(), e))
    }

    /// Load cost analysis from a JSON file.
    pub fn load_json(path: &Path) -> Result<ProgramCost, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
        Self::from_json(&content)
    }

    /// Parse a ProgramCost from a JSON string.
    /// Defaults to Triton table names for backward compatibility.
    pub fn from_json(s: &str) -> Result<ProgramCost, String> {
        Self::from_json_with_names(
            s,
            &["processor", "hash", "u32", "op_stack", "ram", "jump_stack"],
        )
    }

    /// Parse a ProgramCost from a JSON string with specific table names.
    pub fn from_json_with_names(s: &str, names: &[&str]) -> Result<ProgramCost, String> {
        // Extract "functions" block
        let fns_start = s
            .find("\"functions\"")
            .ok_or_else(|| "missing 'functions' key".to_string())?;
        let fns_obj_start = s[fns_start..]
            .find('{')
            .map(|i| fns_start + i)
            .ok_or_else(|| "missing functions object".to_string())?;

        // Find matching closing brace for functions object
        let fns_obj_end = find_matching_brace(s, fns_obj_start)
            .ok_or_else(|| "unmatched brace in functions".to_string())?;
        let fns_content = &s[fns_obj_start + 1..fns_obj_end];

        // Parse individual function entries
        let mut functions = Vec::new();
        let mut pos = 0;
        while pos < fns_content.len() {
            // Find next function name
            if let Some(quote_start) = fns_content[pos..].find('"') {
                let name_start = pos + quote_start + 1;
                if let Some(quote_end) = fns_content[name_start..].find('"') {
                    let name = fns_content[name_start..name_start + quote_end].to_string();
                    // Find the cost object for this function
                    let after_name = name_start + quote_end + 1;
                    if let Some(obj_start) = fns_content[after_name..].find('{') {
                        let abs_obj_start = after_name + obj_start;
                        if let Some(obj_end) = find_matching_brace(fns_content, abs_obj_start) {
                            let cost_str = &fns_content[abs_obj_start..=obj_end];
                            if let Some(cost) = TableCost::from_json_value(cost_str, names) {
                                functions.push(FunctionCost {
                                    name,
                                    cost,
                                    per_iteration: None,
                                });
                            }
                            pos = obj_end + 1;
                            continue;
                        }
                    }
                }
            }
            break;
        }

        // Extract "total"
        let total = {
            let total_start = s
                .find("\"total\"")
                .ok_or_else(|| "missing 'total' key".to_string())?;
            let obj_start = s[total_start..]
                .find('{')
                .map(|i| total_start + i)
                .ok_or_else(|| "missing total object".to_string())?;
            let obj_end = find_matching_brace(s, obj_start)
                .ok_or_else(|| "unmatched brace in total".to_string())?;
            TableCost::from_json_value(&s[obj_start..=obj_end], names)
                .ok_or_else(|| "invalid total cost".to_string())?
        };

        // Extract "padded_height"
        let padded_height = {
            let ph_start = s
                .find("\"padded_height\"")
                .ok_or_else(|| "missing 'padded_height' key".to_string())?;
            let rest = &s[ph_start + "\"padded_height\"".len()..];
            let colon = rest
                .find(':')
                .ok_or_else(|| "missing colon after padded_height".to_string())?;
            let after_colon = rest[colon + 1..].trim_start();
            let end = after_colon
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_colon.len());
            after_colon[..end]
                .parse::<u64>()
                .map_err(|e| format!("invalid padded_height: {}", e))?
        };

        Ok(ProgramCost {
            program_name: String::new(),
            functions,
            total,
            table_names: names.iter().map(|s| s.to_string()).collect(),
            table_short_names: names.iter().map(|s| s.to_string()).collect(),
            attestation_hash_rows: 0,
            padded_height,
            estimated_proving_ns: 0,
            loop_bound_waste: Vec::new(),
        })
    }

    /// Format a comparison between this cost and another (old vs new).
    pub fn format_comparison(&self, other: &ProgramCost) -> String {
        let short = self.short_names();
        let primary = short.first().unwrap_or(&"?");
        let mut out = String::new();
        out.push_str("Cost comparison:\n");
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "Function",
            format!("{} (old)", primary),
            format!("{} (new)", primary),
            "delta"
        ));
        out.push_str(&"-".repeat(48));
        out.push('\n');

        // Collect all function names from both
        let mut all_names: Vec<String> = Vec::new();
        for f in &self.functions {
            if !all_names.contains(&f.name) {
                all_names.push(f.name.clone());
            }
        }
        for f in &other.functions {
            if !all_names.contains(&f.name) {
                all_names.push(f.name.clone());
            }
        }

        for name in &all_names {
            let old_val = self
                .functions
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.cost.get(0))
                .unwrap_or(0);
            let new_val = other
                .functions
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.cost.get(0))
                .unwrap_or(0);
            let delta = new_val as i64 - old_val as i64;
            let delta_str = if delta > 0 {
                format!("+{}", delta)
            } else if delta == 0 {
                "0".to_string()
            } else {
                format!("{}", delta)
            };
            out.push_str(&format!(
                "{:<20} {:>9} {:>9}  {:>6}\n",
                name, old_val, new_val, delta_str
            ));
        }

        out.push_str(&"-".repeat(48));
        out.push('\n');

        let old_total = self.total.get(0);
        let new_total = other.total.get(0);
        let total_delta = new_total as i64 - old_total as i64;
        let total_delta_str = if total_delta > 0 {
            format!("+{}", total_delta)
        } else if total_delta == 0 {
            "0".to_string()
        } else {
            format!("{}", total_delta)
        };
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "TOTAL", old_total, new_total, total_delta_str
        ));

        let old_ph = self.padded_height;
        let new_ph = other.padded_height;
        let ph_delta = new_ph as i64 - old_ph as i64;
        let ph_delta_str = if ph_delta > 0 {
            format!("+{}", ph_delta)
        } else if ph_delta == 0 {
            "0".to_string()
        } else {
            format!("{}", ph_delta)
        };
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "Padded height:", old_ph, new_ph, ph_delta_str
        ));

        out
    }

    /// Generate diagnostics for power-of-2 boundary proximity.
    pub fn boundary_warnings(&self) -> Vec<Diagnostic> {
        let mut warnings = Vec::new();
        let max_height = self.total.max_height().max(self.attestation_hash_rows);
        let headroom = self.padded_height - max_height;

        if headroom < self.padded_height / 8 {
            let mut diag = Diagnostic::warning(
                format!("program is {} rows below padded height boundary", headroom),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "padded_height = {} (max table height = {})",
                self.padded_height, max_height
            ));
            diag.notes.push(format!(
                "adding {}+ rows to any table will double proving cost to {}",
                headroom + 1,
                self.padded_height * 2
            ));
            diag.help = Some(format!(
                "consider optimizing to stay well below {}",
                self.padded_height
            ));
            warnings.push(diag);
        }

        warnings
    }

    /// Index of the dominant (tallest) table.
    pub(crate) fn dominant_index(&self) -> usize {
        let n = self.total.count as usize;
        let max = self.total.max_height();
        if max == 0 {
            return 0;
        }
        for i in 0..n {
            if self.total.get(i) == max {
                return i;
            }
        }
        0
    }
}
