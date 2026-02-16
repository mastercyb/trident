use super::analyzer::{FunctionCost, ProgramCost};
use super::visit::next_power_of_two;
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
        let secs = self.estimated_proving_ns / 1_000_000_000;
        let tenths = (self.estimated_proving_ns / 100_000_000) % 10;
        out.push_str(&format!("Estimated proving time:  ~{}.{}s\n", secs, tenths));

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
                val * 100 / dominant_total
            } else {
                0
            };
            out.push_str(&format!(
                "  {}. {:<24} {:>6} {} rows ({}% of {} table)\n",
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
                let dominant_val = self.total.get(dominant_idx);
                let primary_val = self.total.get(0);
                // ratio > 2.0 equivalent: dominant_val > 2 * primary_val
                if dominant_val > 2 * primary_val {
                    let dominant_name = short.get(dominant_idx).unwrap_or(&"?");
                    let primary_name = short.first().unwrap_or(&"?");
                    // Integer ratio with one decimal: ratio_10 = dominant * 10 / primary
                    let ratio_10 = if primary_val > 0 {
                        dominant_val * 10 / primary_val
                    } else {
                        0
                    };
                    let mut diag = Diagnostic::warning(
                        format!(
                            "hint[H0001]: {} table is {}.{}x taller than {} table",
                            dominant_name,
                            ratio_10 / 10,
                            ratio_10 % 10,
                            primary_name
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
            let headroom_pct = if self.padded_height > 0 {
                headroom * 100 / self.padded_height
            } else {
                0
            };
            let mut diag = Diagnostic::warning(
                format!(
                    "hint[H0002]: padded height is {}, but max table height is only {}",
                    self.padded_height, max_height
                ),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "you have {} rows of headroom ({}%) before the next doubling",
                headroom, headroom_pct
            ));
            diag.help = Some(format!(
                "this program could be {}% more complex at zero additional proving cost",
                headroom_pct
            ));
            hints.push(diag);
        }

        // H0004: Loop bound waste (entries already filtered at 4x+ in analyzer)
        // Also handles unknown-bound entries (bound == 0) from non-constant loops.
        for (fn_name, end_val, bound) in &self.loop_bound_waste {
            if *bound == 0 {
                // Non-constant loop end with no `bounded` annotation
                let mut diag = Diagnostic::warning(
                    format!(
                        "hint[H0004]: loop in '{}' has non-constant bound, cost assumes {} iteration(s)",
                        fn_name, end_val
                    ),
                    Span::dummy(),
                );
                diag.help = Some(
                    "add a `bounded N` annotation to set a realistic worst-case iteration count"
                        .to_string(),
                );
                hints.push(diag);
            } else {
                let actual = *end_val.max(&1);
                let ratio = *bound / actual;
                let mut diag = Diagnostic::warning(
                    format!(
                        "hint[H0004]: loop in '{}' bounded {} but iterates only {} times",
                        fn_name, bound, end_val
                    ),
                    Span::dummy(),
                );
                diag.notes.push(format!(
                    "declared bound is {}x the actual iteration count",
                    ratio
                ));
                diag.help = Some(format!(
                    "tightening the bound to {} would reduce worst-case cost",
                    next_power_of_two(*end_val)
                ));
                hints.push(diag);
            }
        }

        hints
    }
}
