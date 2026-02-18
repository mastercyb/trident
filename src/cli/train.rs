use std::path::Path;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct TrainArgs {
    /// Epochs over the full corpus (default: 10)
    #[arg(short, long, default_value = "10")]
    pub epochs: u64,
    /// Generations per file per epoch (default: 10)
    #[arg(short, long, default_value = "10")]
    pub generations: u64,
    /// Use GPU acceleration (default: CPU parallel)
    #[arg(long)]
    pub gpu: bool,
}

struct CompiledFile {
    path: String,
    blocks: Vec<trident::ir::tir::encode::TIRBlock>,
    per_block_baselines: Vec<u64>,
    baseline_cost: u64,
}

// ANSI colors
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const BOLD: &str = "\x1b[1m";
const WHITE: &str = "\x1b[37m";

fn term_width() -> usize {
    if let Some(w) = std::env::var("COLUMNS").ok().and_then(|s| s.parse().ok()) {
        return w;
    }
    #[cfg(unix)]
    if let Ok(out) = std::process::Command::new("tput").arg("cols").output() {
        if let Ok(w) = String::from_utf8_lossy(&out.stdout).trim().parse::<usize>() {
            if w > 0 {
                return w;
            }
        }
    }
    80
}

/// Visible length of a string (strips ANSI escape sequences).
fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c.is_ascii_alphabetic() {
                in_esc = false;
            }
        } else if c == '\x1b' {
            in_esc = true;
        } else {
            len += 1;
        }
    }
    len
}

/// Print a box around lines. Each line is padded to fill the box width.
/// The box adapts to the widest content line, capped at terminal width.
fn print_box(lines: &[String]) {
    let tw = term_width().saturating_sub(4); // 2 indent + border chars
    let max_content = lines.iter().map(|l| visible_len(l)).max().unwrap_or(0);
    let inner = max_content.min(tw);

    eprintln!("  {DIM}┌{}┐{RESET}", "─".repeat(inner + 2));
    for line in lines {
        let vlen = visible_len(line);
        let pad = inner.saturating_sub(vlen);
        eprintln!("  {DIM}│{RESET} {}{} {DIM}│{RESET}", line, " ".repeat(pad));
    }
    eprintln!("  {DIM}└{}┘{RESET}", "─".repeat(inner + 2));
}

pub fn cmd_train(args: TrainArgs) {
    use trident::ir::tir::neural::weights;

    let corpus = discover_corpus();
    if corpus.is_empty() {
        eprintln!("error: no .tri files found in vm/, std/, os/");
        process::exit(1);
    }

    let meta = weights::load_best_meta().ok();
    let gen_start = meta.as_ref().map_or(0, |m| m.generation);

    eprintln!("{BOLD}trident train{RESET}");
    eprintln!("  compiling corpus...");

    let _guard = trident::diagnostic::suppress_warnings();
    let compiled = compile_corpus(&corpus);
    drop(_guard);

    let total_blocks: usize = compiled.iter().map(|c| c.blocks.len()).sum();
    let total_baseline: u64 = compiled.iter().map(|c| c.baseline_cost).sum();
    let total_gens = args.epochs * compiled.len() as u64 * args.generations;

    // Header
    eprintln!();
    let header = vec![
        format!(
            "corpus    {WHITE}{}{RESET} files ({CYAN}{}{RESET} trainable, {CYAN}{}{RESET} blocks)",
            corpus.len(),
            compiled.len(),
            total_blocks
        ),
        format!("baseline  {WHITE}{}{RESET} total cost", total_baseline),
        format!(
            "schedule  {WHITE}{}{RESET} epochs x {WHITE}{}{RESET} gens = {CYAN}{}{RESET} total",
            args.epochs, args.generations, total_gens
        ),
        format!(
            "model     gen {WHITE}{}{RESET} | {}",
            gen_start,
            if args.gpu { "GPU" } else { "CPU" }
        ),
    ];
    print_box(&header);
    eprintln!();

    let start = std::time::Instant::now();
    let mut total_trained = 0u64;
    let mut prev_epoch_avg = 0u64;

    for epoch in 0..args.epochs {
        let mut indices: Vec<usize> = (0..compiled.len()).collect();
        shuffle(&mut indices, gen_start + epoch);

        let epoch_start = std::time::Instant::now();
        let mut epoch_costs: Vec<(usize, u64)> = Vec::new();

        for (i, &file_idx) in indices.iter().enumerate() {
            let cf = &compiled[file_idx];
            eprint!(
                "\r  {DIM}epoch {}/{}{RESET} {DIM}│{RESET} {}/{} {DIM}│{RESET} {}",
                epoch + 1,
                args.epochs,
                i + 1,
                compiled.len(),
                cf.path,
            );
            let pad = 50usize.saturating_sub(cf.path.len());
            eprint!("{}", " ".repeat(pad));
            use std::io::Write;
            let _ = std::io::stderr().flush();

            let cost = train_one_compiled(cf, args.generations, args.gpu);
            epoch_costs.push((file_idx, cost));
            total_trained += 1;
        }

        let epoch_elapsed = epoch_start.elapsed();
        let epoch_cost: u64 = epoch_costs.iter().map(|(_, c)| c).sum();
        let avg_cost = epoch_cost / compiled.len().max(1) as u64;
        let ratio = epoch_cost as f64 / total_baseline.max(1) as f64;

        let trend = if epoch == 0 {
            String::new()
        } else if avg_cost < prev_epoch_avg {
            format!(" {GREEN}-{}{RESET}", prev_epoch_avg - avg_cost)
        } else if avg_cost > prev_epoch_avg {
            format!(" {RED}+{}{RESET}", avg_cost - prev_epoch_avg)
        } else {
            format!(" {DIM}={RESET}")
        };
        prev_epoch_avg = avg_cost;

        let ratio_color = ratio_to_color(ratio);
        eprintln!(
            "\r  {BOLD}epoch {}/{}{RESET} {DIM}│{RESET} {ratio_color}{:.2}x{RESET} {DIM}│{RESET} cost {WHITE}{}{RESET}/{} {DIM}│{RESET} avg {WHITE}{}{RESET} {DIM}│{RESET} {DIM}{:.1}s{RESET}{}",
            epoch + 1, args.epochs, ratio, epoch_cost, total_baseline,
            avg_cost, epoch_elapsed.as_secs_f64(), trend,
        );

        // Per-file table on first and last epoch
        if epoch == 0 || epoch + 1 == args.epochs {
            let mut sorted: Vec<_> = epoch_costs
                .iter()
                .map(|&(idx, cost)| {
                    let cf = &compiled[idx];
                    (cf.path.as_str(), cf.blocks.len(), cost, cf.baseline_cost)
                })
                .collect();
            sorted.sort_by(|a, b| {
                let ra = a.2 as f64 / a.3.max(1) as f64;
                let rb = b.2 as f64 / b.3.max(1) as f64;
                ra.partial_cmp(&rb).unwrap()
            });

            let label = if epoch == 0 { "initial" } else { "final" };
            eprintln!();
            print_file_table(&sorted, label);
            eprintln!();
        }
    }

    let elapsed = start.elapsed();
    let meta = weights::load_best_meta().ok();
    let gen_end = meta.as_ref().map_or(0, |m| m.generation);

    // Summary
    let mut summary = vec![
        format!("{BOLD}done{RESET}"),
        format!(
            "generations  {WHITE}{}{RESET} -> {WHITE}{}{RESET} ({GREEN}+{}{RESET})",
            gen_start,
            gen_end,
            gen_end - gen_start
        ),
        format!(
            "trained      {WHITE}{}{RESET} file-passes in {WHITE}{:.1}s{RESET}",
            total_trained,
            elapsed.as_secs_f64()
        ),
    ];
    if let Some(meta) = meta {
        summary.push(format!(
            "model        score {BOLD}{}{RESET} | {}",
            meta.best_score, meta.status
        ));
        summary.push(format!(
            "weights      {DIM}{}{RESET}",
            &meta.weight_hash[..16.min(meta.weight_hash.len())]
        ));
    }
    print_box(&summary);
}

fn print_file_table(rows: &[(&str, usize, u64, u64)], label: &str) {
    let tw = term_width();

    // Fixed columns: blk(5) + cost(6) + base(6) + ratio(7) = 24
    // Separators and padding: 7 columns × 3 chars (│ + 2 spaces) + outer = ~28
    // Minimum file = 12, minimum bar = 4
    let w_blk = 5;
    let w_ratio = 7;

    // Measure actual data widths for cost/base columns
    let total_cost: u64 = rows.iter().map(|(_, _, c, _)| c).sum();
    let total_base: u64 = rows.iter().map(|(_, _, _, b)| b).sum();
    let w_cost = format!(
        "{}",
        rows.iter()
            .map(|(_, _, c, _)| c)
            .max()
            .copied()
            .unwrap_or(0)
            .max(total_cost)
    )
    .len()
    .max(4);
    let w_base = format!(
        "{}",
        rows.iter()
            .map(|(_, _, _, b)| b)
            .max()
            .copied()
            .unwrap_or(0)
            .max(total_base)
    )
    .len()
    .max(4);

    // Fixed overhead: 2 indent + 7 borders + 12 padding (2 per col)
    let fixed = 2 + 7 + 12 + w_blk + w_cost + w_base + w_ratio;
    let flexible = tw.saturating_sub(fixed);

    // Split flexible space: 70% file, 30% bar (min 4)
    let w_bar = (flexible * 3 / 10).max(4);
    let w_file = flexible.saturating_sub(w_bar).max(12);

    let sep_top = format!(
        "  {DIM}┌{}┬{}┬{}┬{}┬{}┬{}┐{RESET}",
        "─".repeat(w_file + 2),
        "─".repeat(w_blk + 2),
        "─".repeat(w_cost + 2),
        "─".repeat(w_base + 2),
        "─".repeat(w_ratio + 2),
        "─".repeat(w_bar + 2),
    );
    let sep_mid = format!(
        "  {DIM}├{}┼{}┼{}┼{}┼{}┼{}┤{RESET}",
        "─".repeat(w_file + 2),
        "─".repeat(w_blk + 2),
        "─".repeat(w_cost + 2),
        "─".repeat(w_base + 2),
        "─".repeat(w_ratio + 2),
        "─".repeat(w_bar + 2),
    );
    let sep_bot = format!(
        "  {DIM}└{}┴{}┴{}┴{}┴{}┴{}┘{RESET}",
        "─".repeat(w_file + 2),
        "─".repeat(w_blk + 2),
        "─".repeat(w_cost + 2),
        "─".repeat(w_base + 2),
        "─".repeat(w_ratio + 2),
        "─".repeat(w_bar + 2),
    );

    eprintln!("  {DIM}{}{RESET}", label);
    eprintln!("{}", sep_top);
    eprintln!(
        "  {DIM}│{RESET} {BOLD}{:<w_file$}{RESET} {DIM}│{RESET} {BOLD}{:>w_blk$}{RESET} {DIM}│{RESET} {BOLD}{:>w_cost$}{RESET} {DIM}│{RESET} {BOLD}{:>w_base$}{RESET} {DIM}│{RESET} {BOLD}{:>w_ratio$}{RESET} {DIM}│{RESET} {BOLD}{:<w_bar$}{RESET} {DIM}│{RESET}",
        "file", "blk", "cost", "base", "ratio", "",
    );
    eprintln!("{}", sep_mid);

    for (path, blocks, cost, baseline) in rows {
        let r = *cost as f64 / (*baseline).max(1) as f64;
        let color = ratio_to_color(r);
        let bar = ratio_bar(r, w_bar);
        let display_path = truncate_path(path, w_file);
        eprintln!(
            "  {DIM}│{RESET} {:<w_file$} {DIM}│{RESET} {:>w_blk$} {DIM}│{RESET} {:>w_cost$} {DIM}│{RESET} {:>w_base$} {DIM}│{RESET} {color}{:>w_ratio$}{RESET} {DIM}│{RESET} {} {DIM}│{RESET}",
            display_path, blocks, cost, baseline, format!("{:.2}x", r), bar,
        );
    }

    // Totals
    let total_blocks: usize = rows.iter().map(|(_, b, _, _)| b).sum();
    let total_ratio = total_cost as f64 / total_base.max(1) as f64;
    let total_color = ratio_to_color(total_ratio);

    eprintln!("{}", sep_mid);
    eprintln!(
        "  {DIM}│{RESET} {BOLD}{:<w_file$}{RESET} {DIM}│{RESET} {BOLD}{:>w_blk$}{RESET} {DIM}│{RESET} {BOLD}{:>w_cost$}{RESET} {DIM}│{RESET} {BOLD}{:>w_base$}{RESET} {DIM}│{RESET} {total_color}{BOLD}{:>w_ratio$}{RESET} {DIM}│{RESET} {} {DIM}│{RESET}",
        "total", total_blocks, total_cost, total_base,
        format!("{:.2}x", total_ratio), ratio_bar(total_ratio, w_bar),
    );
    eprintln!("{}", sep_bot);
}

/// Truncate a path to fit in `max` chars, preserving the tail.
fn truncate_path(path: &str, max: usize) -> String {
    if path.len() <= max {
        return path.to_string();
    }
    if max <= 3 {
        return path[path.len().saturating_sub(max)..].to_string();
    }
    format!("..{}", &path[path.len() - (max - 2)..])
}

fn ratio_to_color(r: f64) -> &'static str {
    if r <= 0.3 {
        GREEN
    } else if r <= 0.6 {
        CYAN
    } else if r <= 0.9 {
        YELLOW
    } else {
        RED
    }
}

fn ratio_bar(ratio: f64, width: usize) -> String {
    let filled = ((1.0 - ratio.min(1.0)) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    let color = ratio_to_color(ratio);
    format!(
        "{color}{}{RESET}{DIM}{}{RESET}",
        "█".repeat(filled),
        "░".repeat(empty)
    )
}

fn compile_corpus(files: &[std::path::PathBuf]) -> Vec<CompiledFile> {
    let options = super::resolve_options("triton", "debug", None);
    let mut compiled = Vec::new();

    for file in files {
        let ir = match trident::build_tir_project(file, &options) {
            Ok(ir) => ir,
            Err(_) => continue,
        };
        let blocks = trident::ir::tir::encode::encode_blocks(&ir);
        if blocks.is_empty() {
            continue;
        }

        let lowering = trident::ir::tir::lower::create_stack_lowering(&options.target_config.name);
        let baseline_tasm = lowering.lower(&ir);
        let baseline_profile = trident::cost::scorer::profile_tasm_str(&baseline_tasm.join("\n"));
        let baseline_cost = baseline_profile.cost();

        let per_block_baselines: Vec<u64> = blocks
            .iter()
            .map(|block| {
                let block_ops = &ir[block.start_idx..block.end_idx];
                if block_ops.is_empty() {
                    return 1;
                }
                let block_tasm = lowering.lower(block_ops);
                if block_tasm.is_empty() {
                    return 1;
                }
                let profile = trident::cost::scorer::profile_tasm(
                    &block_tasm.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                );
                profile.cost().max(1)
            })
            .collect();

        compiled.push(CompiledFile {
            path: short_path(file),
            blocks,
            per_block_baselines,
            baseline_cost,
        });
    }

    compiled
}

fn train_one_compiled(cf: &CompiledFile, generations: u64, gpu: bool) -> u64 {
    use trident::field::PrimeField;
    use trident::ir::tir::lower::decode_output;
    use trident::ir::tir::neural::evolve::Population;
    use trident::ir::tir::neural::model::NeuralModel;
    use trident::ir::tir::neural::weights::{self, OptimizerMeta, OptimizerStatus};

    let (model, meta) = match weights::load_best_weights() {
        Ok(w) => {
            let meta = weights::load_best_meta().unwrap_or(OptimizerMeta {
                generation: 0,
                weight_hash: weights::hash_weights(&w),
                best_score: 0,
                prev_score: 0,
                baseline_score: 0,
                status: OptimizerStatus::Improving,
            });
            (NeuralModel::from_weight_vec(&w), meta)
        }
        Err(_) => {
            let meta = OptimizerMeta {
                generation: 0,
                weight_hash: String::new(),
                best_score: 0,
                prev_score: 0,
                baseline_score: 0,
                status: OptimizerStatus::Improving,
            };
            (NeuralModel::zeros(), meta)
        }
    };

    let gen_start = meta.generation;
    let current_weights = model.to_weight_vec();
    let mut pop = if current_weights.iter().all(|w| w.to_f64() == 0.0) {
        Population::new_random(gen_start.wrapping_add(42))
    } else {
        Population::from_weights(&current_weights, gen_start.wrapping_add(42))
    };

    let score_before = if meta.best_score > 0 {
        meta.best_score
    } else {
        cf.baseline_cost
    };

    let gpu_accel = if gpu {
        trident::gpu::neural_accel::NeuralAccelerator::try_new(
            &cf.blocks,
            trident::ir::tir::neural::evolve::POP_SIZE as u32,
        )
    } else {
        None
    };

    let mut best_seen = i64::MIN;
    for gen in 0..generations {
        if let Some(ref accel) = gpu_accel {
            let weight_vecs: Vec<Vec<u64>> = pop
                .individuals
                .iter()
                .map(|ind| ind.weights.iter().map(|w| w.raw().to_u64()).collect())
                .collect();
            let gpu_outputs = accel.batch_forward(&weight_vecs);
            for (i, ind) in pop.individuals.iter_mut().enumerate() {
                let mut total = 0i64;
                for (b, _) in cf.blocks.iter().enumerate() {
                    total -=
                        score_neural_output(&gpu_outputs[i][b], cf.per_block_baselines[b]) as i64;
                }
                ind.fitness = total;
            }
            pop.update_best();
        } else {
            pop.evaluate_with_baselines(
                &cf.blocks,
                &cf.per_block_baselines,
                |m: &mut NeuralModel,
                 block: &trident::ir::tir::encode::TIRBlock,
                 block_baseline: u64| {
                    let output = m.forward(block);
                    if output.is_empty() {
                        return -(block_baseline as i64);
                    }
                    let candidate_lines = decode_output(&output);
                    if candidate_lines.is_empty() {
                        return -(block_baseline as i64);
                    }
                    let profile = trident::cost::scorer::profile_tasm(
                        &candidate_lines
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>(),
                    );
                    -(profile.cost().min(block_baseline) as i64)
                },
            );
        }

        let gen_best = pop
            .individuals
            .iter()
            .map(|i| i.fitness)
            .max()
            .unwrap_or(i64::MIN);
        if gen_best > best_seen {
            best_seen = gen_best;
        }
        pop.evolve(gen_start.wrapping_add(gen));
    }

    let best = pop.best_weights();
    let score_after = if best_seen > i64::MIN {
        (-best_seen) as u64
    } else {
        cf.baseline_cost
    };

    let weight_hash = weights::hash_weights(best);
    let dummy_root = Path::new(".");
    let _ = weights::save_weights(best, &weights::weights_path(dummy_root));

    let mut tracker = weights::ConvergenceTracker::new();
    let status = tracker.record(score_after);
    let new_meta = OptimizerMeta {
        generation: gen_start + generations,
        weight_hash,
        best_score: score_after,
        prev_score: score_before,
        baseline_score: cf.baseline_cost,
        status,
    };
    let _ = weights::save_meta(&new_meta, &weights::meta_path(dummy_root));

    score_after
}

fn discover_corpus() -> Vec<std::path::PathBuf> {
    let root = find_repo_root();
    let mut files = Vec::new();
    for dir in &["vm", "std", "os"] {
        let dir_path = root.join(dir);
        if dir_path.is_dir() {
            files.extend(super::resolve_tri_files(&dir_path));
        }
    }
    files.sort();
    files
}

fn find_repo_root() -> std::path::PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("vm").is_dir() {
            return dir;
        }
        if !dir.pop() {
            return std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        }
    }
}

fn shuffle(indices: &mut Vec<usize>, seed: u64) {
    let n = indices.len();
    if n <= 1 {
        return;
    }
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    for i in (1..n).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state >> 33) as usize % (i + 1);
        indices.swap(i, j);
    }
}

fn short_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    for prefix in &["vm/", "std/", "os/"] {
        if let Some(pos) = s.find(prefix) {
            return s[pos..].to_string();
        }
    }
    s.to_string()
}

fn score_neural_output(raw_codes: &[u32], block_baseline: u64) -> u64 {
    use trident::ir::tir::lower::decode_output;
    let codes: Vec<u64> = raw_codes
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u64)
        .collect();
    if codes.is_empty() {
        return block_baseline;
    }
    let candidate_lines = decode_output(&codes);
    if candidate_lines.is_empty() {
        return block_baseline;
    }
    let profile = trident::cost::scorer::profile_tasm(
        &candidate_lines
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    );
    profile.cost().min(block_baseline)
}
