use std::path::Path;

use super::analyzer::{find_matching_brace, next_power_of_two, FunctionCost, ProgramCost};
use super::model::TableCost;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

// --- Report formatting ---

impl ProgramCost {
    /// Format a table-style cost report.
    pub fn format_report(&self) -> String {
        let short = self.short_names();
        let n = short.len();
        let mut out = String::new();
        out.push_str(&format!("Cost report: {}\n", self.program_name));

        // Header
        out.push_str(&format!("{:<24}", "Function"));
        for name in &short {
            out.push_str(&format!(" {:>6}", name));
        }
        out.push_str("  dominant\n");
        let line_width = 24 + n * 7 + 10;
        out.push_str(&"-".repeat(line_width));
        out.push('\n');

        for func in &self.functions {
            out.push_str(&format!("{:<24}", func.name));
            for i in 0..n {
                out.push_str(&format!(" {:>6}", func.cost.get(i)));
            }
            out.push_str(&format!("  {}\n", func.cost.dominant_table(&short)));
            if let Some((per_iter, bound)) = &func.per_iteration {
                out.push_str(&format!("  per iteration (x{})", bound));
                let label_len = format!("  per iteration (x{})", bound).len();
                // Pad to align with columns
                for _ in label_len..24 {
                    out.push(' ');
                }
                for i in 0..n {
                    out.push_str(&format!(" {:>6}", per_iter.get(i)));
                }
                out.push('\n');
            }
        }

        out.push_str(&"-".repeat(line_width));
        out.push('\n');
        out.push_str(&format!("{:<24}", "TOTAL"));
        for i in 0..n {
            out.push_str(&format!(" {:>6}", self.total.get(i)));
        }
        out.push_str(&format!("  {}\n", self.total.dominant_table(&short)));
        out.push('\n');
        out.push_str(&format!(
            "Padded height:           {}\n",
            self.padded_height
        ));
        out.push_str(&format!(
            "Program attestation:     {} hash rows\n",
            self.attestation_hash_rows
        ));
        out.push_str(&format!(
            "Estimated proving time:  ~{:.1}s\n",
            self.estimated_proving_secs
        ));

        // Power-of-2 boundary warning.
        let headroom = self.padded_height - self.total.max_height();
        if headroom < self.padded_height / 8 {
            out.push_str(&format!(
                "\nwarning: {} rows below padded height boundary ({})\n",
                headroom, self.padded_height
            ));
            out.push_str(&format!(
                "  adding {}+ rows to any table will double proving cost to {}\n",
                headroom + 1,
                self.padded_height * 2
            ));
        }

        out
    }

    /// Format a hotspots report (top N cost contributors).
    pub fn format_hotspots(&self, top_n: usize) -> String {
        let short = self.short_names();
        let mut out = String::new();
        out.push_str(&format!("Top {} cost contributors:\n", top_n));

        let dominant = self.total.dominant_table(&short);
        let dominant_idx = self.dominant_index();
        let dominant_total = self.total.get(dominant_idx);

        let mut ranked: Vec<&FunctionCost> = self.functions.iter().collect();
        ranked.sort_by(|a, b| {
            let av = a.cost.get(dominant_idx);
            let bv = b.cost.get(dominant_idx);
            bv.cmp(&av)
        });

        for (i, func) in ranked.iter().take(top_n).enumerate() {
            let val = func.cost.get(dominant_idx);
            let pct = if dominant_total > 0 {
                (val as f64 / dominant_total as f64) * 100.0
            } else {
                0.0
            };
            out.push_str(&format!(
                "  {}. {:<24} {:>6} {} rows ({:.0}% of {} table)\n",
                i + 1,
                func.name,
                val,
                dominant,
                pct,
                dominant
            ));
        }

        out.push_str(&format!(
            "\nDominant table: {} ({} rows). Reduce {} operations to lower padded height.\n",
            dominant, dominant_total, dominant
        ));

        out
    }

    /// Generate optimization hints (H0001, H0002, H0004).
    pub fn optimization_hints(&self) -> Vec<Diagnostic> {
        let short = self.short_names();
        let mut hints = Vec::new();

        // H0001: Secondary table dominance — a non-primary table is much taller than primary.
        // (For Triton: hash[1] vs processor[0]; generalized to dominant vs first.)
        if self.total.count >= 2 && self.total.get(0) > 0 {
            let dominant_idx = self.dominant_index();
            if dominant_idx > 0 {
                let ratio = self.total.get(dominant_idx) as f64 / self.total.get(0) as f64;
                if ratio > 2.0 {
                    let dominant_name = short.get(dominant_idx).unwrap_or(&"?");
                    let primary_name = short.first().unwrap_or(&"?");
                    let mut diag = Diagnostic::warning(
                        format!(
                            "hint[H0001]: {} table is {:.1}x taller than {} table",
                            dominant_name, ratio, primary_name
                        ),
                        Span::dummy(),
                    );
                    diag.notes.push(format!(
                        "{} optimizations will not reduce proving cost",
                        primary_name
                    ));
                    diag.help = Some(format!(
                        "focus on reducing {} table usage to lower padded height",
                        dominant_name
                    ));
                    hints.push(diag);
                }
            }
        }

        // H0002: Headroom hint (far below boundary = room to grow)
        let max_height = self.total.max_height().max(self.attestation_hash_rows);
        let headroom = self.padded_height - max_height;
        if headroom > self.padded_height / 4 && self.padded_height >= 16 {
            let headroom_pct = (headroom as f64 / self.padded_height as f64) * 100.0;
            let mut diag = Diagnostic::warning(
                format!(
                    "hint[H0002]: padded height is {}, but max table height is only {}",
                    self.padded_height, max_height
                ),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "you have {} rows of headroom ({:.0}%) before the next doubling",
                headroom, headroom_pct
            ));
            diag.help = Some(format!(
                "this program could be {:.0}% more complex at zero additional proving cost",
                headroom_pct
            ));
            hints.push(diag);
        }

        // H0004: Loop bound waste (entries already filtered at 4x+ in analyzer)
        for (fn_name, end_val, bound) in &self.loop_bound_waste {
            let ratio = *bound as f64 / *end_val.max(&1) as f64;
            let mut diag = Diagnostic::warning(
                format!(
                    "hint[H0004]: loop in '{}' bounded {} but iterates only {} times",
                    fn_name, bound, end_val
                ),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "declared bound is {:.0}x the actual iteration count",
                ratio
            ));
            diag.help = Some(format!(
                "tightening the bound to {} would reduce worst-case cost",
                next_power_of_two(*end_val)
            ));
            hints.push(diag);
        }

        hints
    }

    /// Serialize ProgramCost to a JSON string.
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
            estimated_proving_secs: 0.0,
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
    fn dominant_index(&self) -> usize {
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
