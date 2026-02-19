/// Shared trisha subprocess helpers.
///
/// Used by both `trident bench` and `trident audit` to call trisha
/// for execution, proving, and verification.

/// Check if trisha binary is available on PATH.
pub fn trisha_available() -> bool {
    std::process::Command::new("trisha")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Result from a trisha subprocess call.
#[allow(dead_code)]
pub struct TrishaResult {
    pub output: Vec<u64>,
    pub cycle_count: u64,
    pub elapsed_ms: f64,
}

/// Generate a test harness for a TASM function library.
///
/// Parses the TASM for function labels, generates a harness that pushes
/// dummy values, calls each function, and drains the stack. Functions
/// requiring external input (divine, read_io, merkle_step) are skipped.
///
/// Works for both compiler output (`__funcname:`) and hand baselines
/// (`module__funcname:`).
pub fn generate_test_harness(tasm: &str) -> String {
    // Strip comments and unresolved cross-module calls
    let clean_lines: Vec<&str> = tasm
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with("//") && !t.starts_with("call @")
        })
        .collect();
    let clean = clean_lines.join("\n");

    // Parse function boundaries and build call graph
    let external_ops = ["divine", "read_io", "merkle_step", "sponge_absorb_mem"];
    let unsafe_ops = ["assert"]; // ops that fail with dummy zero inputs
    let mut func_labels: Vec<&str> = Vec::new();
    let mut func_has_ext: Vec<bool> = Vec::new();
    let mut func_has_unsafe: Vec<bool> = Vec::new();
    let mut func_calls: Vec<Vec<&str>> = Vec::new();

    let lines: Vec<&str> = clean.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.ends_with(':') && !t.is_empty() {
            let label = t.trim_end_matches(':');
            i += 1;
            let mut has_ext = false;
            let mut has_unsafe = false;
            let mut calls = Vec::new();
            while i < lines.len() {
                let t2 = lines[i].trim();
                if t2.ends_with(':') && !t2.is_empty() {
                    break;
                }
                for op in &external_ops {
                    if t2.starts_with(op) {
                        has_ext = true;
                    }
                }
                for op in &unsafe_ops {
                    if t2.starts_with(op) {
                        has_unsafe = true;
                    }
                }
                if let Some(target) = t2.strip_prefix("call ") {
                    if target.starts_with('@') {
                        // Cross-module call — unresolved linker symbol
                        has_ext = true;
                    } else {
                        calls.push(target);
                    }
                }
                if t2 == "recurse" {
                    // Recursive functions may loop forever with dummy inputs
                    has_unsafe = true;
                }
                i += 1;
            }
            func_labels.push(label);
            func_has_ext.push(has_ext);
            func_has_unsafe.push(has_unsafe);
            func_calls.push(calls);
        } else {
            i += 1;
        }
    }

    // Propagate: if function A calls function B which has external/unsafe ops,
    // then A is also external/unsafe. Fixed-point iteration.
    let n_funcs = func_labels.len();
    let label_to_idx: std::collections::HashMap<&str, usize> = func_labels
        .iter()
        .enumerate()
        .map(|(i, &l)| (l, i))
        .collect();

    let mut skip = vec![false; n_funcs];
    for i in 0..n_funcs {
        skip[i] = func_has_ext[i] || func_has_unsafe[i];
    }
    // Fixed-point propagation
    loop {
        let mut changed = false;
        for i in 0..n_funcs {
            if skip[i] {
                continue;
            }
            for &target in &func_calls[i] {
                if let Some(&j) = label_to_idx.get(target) {
                    if skip[j] && !skip[i] {
                        skip[i] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Build harness: for each callable function, push zeros and call
    let mut harness = String::with_capacity(clean.len() + n_funcs * 200);

    for i in 0..n_funcs {
        if skip[i] {
            continue;
        }
        // Push 16 zeros as dummy inputs (enough for any function).
        // Stack is unlimited (spills to RAM) so no drain needed between calls.
        for _ in 0..16 {
            harness.push_str("    push 0\n");
        }
        harness.push_str(&format!("    call {}\n", func_labels[i]));
    }

    harness.push_str("    halt\n");

    // Append all function bodies
    harness.push_str(&clean);
    harness.push('\n');

    harness
}

/// Run trisha as a subprocess, parse output.
pub fn run_trisha(args: &[&str]) -> Result<TrishaResult, String> {
    let start = std::time::Instant::now();
    let result = std::process::Command::new("trisha")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run trisha: {}", e))?;

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        // Filter out GPU init lines to surface the real error
        let err_msg: String = stderr
            .lines()
            .filter(|l| !l.starts_with("GPU:") && !l.starts_with("Backend:"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(err_msg.trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&result.stdout);
    let output: Vec<u64> = stdout
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();

    let stderr = String::from_utf8_lossy(&result.stderr);
    let cycle_count = stderr
        .lines()
        .find_map(|l| {
            if l.contains("cycles") {
                l.split_whitespace().find_map(|w| w.parse::<u64>().ok())
            } else {
                None
            }
        })
        .unwrap_or(0);

    Ok(TrishaResult {
        output,
        cycle_count,
        elapsed_ms,
    })
}
