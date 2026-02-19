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

/// Generated test harness with its required external inputs.
pub struct Harness {
    /// The complete TASM program (harness preamble + function bodies).
    pub tasm: String,
    /// Number of functions in the harness.
    pub n_funcs: usize,
    /// Number of public input values needed (for `read_io`).
    pub read_io_count: usize,
    /// Number of secret/divine values needed (for `divine`).
    pub divine_count: usize,
    /// Number of merkle digests needed (for `merkle_step`).
    pub merkle_count: usize,
}

/// Generate a test harness for a TASM function library.
///
/// Parses the TASM for function labels, generates a harness that pushes
/// dummy values and calls every function. External inputs (`divine`,
/// `read_io`, `merkle_step`) are simulated by providing the required
/// counts in the returned `Harness`. Assertions are neutralized
/// (`assert` → `pop 1`) so they don't crash with dummy values.
///
/// Works for both compiler output (`__funcname:`) and hand baselines
/// (`module__funcname:`).
pub fn generate_test_harness(tasm: &str) -> Harness {
    // Strip comments and unresolved cross-module calls
    let clean_lines: Vec<&str> = tasm
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with("//") && !t.starts_with("call @")
        })
        .collect();
    let clean = clean_lines.join("\n");

    // Parse function labels
    let mut func_labels: Vec<&str> = Vec::new();
    let lines: Vec<&str> = clean.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.ends_with(':') && !t.is_empty() {
            func_labels.push(t.trim_end_matches(':'));
        }
        i += 1;
    }

    // Count external input instructions across entire TASM.
    // divine N → N values from secret input
    // read_io N → N values from public input
    // merkle_step → 1 digest (5 field elements) from nondeterminism
    let mut divine_count: usize = 0;
    let mut read_io_count: usize = 0;
    let mut merkle_count: usize = 0;

    for line in clean.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("divine ") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                divine_count += n;
            } else {
                divine_count += 1;
            }
        } else if t == "divine" {
            divine_count += 1;
        } else if let Some(rest) = t.strip_prefix("read_io ") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                read_io_count += n;
            } else {
                read_io_count += 1;
            }
        } else if t == "read_io" {
            read_io_count += 1;
        } else if t == "merkle_step" {
            merkle_count += 1;
        }
    }

    // Transform TASM body:
    // - assert → pop 1 (neutralize without crashing)
    // - recurse → return (terminate loops immediately)
    let mut body = String::with_capacity(clean.len());
    for line in clean.lines() {
        let t = line.trim();
        if t == "assert" {
            // assert pops 1 value and crashes if 0 — replace with pop 1
            body.push_str("    pop 1\n");
        } else if t == "assert_vector" {
            // assert_vector checks stack[0..5] == stack[5..10], pops 5
            body.push_str("    pop 5\n");
        } else if t == "merkle_step" || t == "merkle_step_mem" {
            // merkle_step reads from nondeterminism oracle — replace with nop
            // Stack effect is neutral (replaces top 5 + index in place)
            body.push_str("    nop\n");
        } else if t == "recurse" {
            // recurse re-enters current function — replace with return
            body.push_str("    return\n");
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }

    // Build harness preamble: push zeros, call each function
    let n_funcs = func_labels.len();
    let mut harness = String::with_capacity(body.len() + n_funcs * 200);

    for label in &func_labels {
        for _ in 0..16 {
            harness.push_str("    push 0\n");
        }
        harness.push_str(&format!("    call {}\n", label));
    }

    harness.push_str("    halt\n");
    harness.push_str(&body);

    Harness {
        tasm: harness,
        n_funcs,
        read_io_count,
        divine_count,
        merkle_count,
    }
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

/// Build trisha CLI args with external input flags.
///
/// Over-provisions inputs generously: each divine/read_io instruction may
/// be reached multiple times due to transitive calls from the harness,
/// so we multiply the static count by the number of functions as an upper bound.
pub fn trisha_args_with_inputs(base_args: &[&str], harness: &Harness) -> Vec<String> {
    let mut args: Vec<String> = base_args.iter().map(|s| s.to_string()).collect();

    // Over-provision generously: deep call chains can multiply consumption
    // well beyond n_funcs * static_count. Use n_funcs² as a safe upper bound.
    let n = harness.n_funcs.max(1);
    let multiplier = n * n;

    if harness.read_io_count > 0 {
        args.push("--input-values".into());
        let n = harness.read_io_count * multiplier;
        let vals: Vec<String> = vec!["0".into(); n];
        args.push(vals.join(","));
    }

    if harness.divine_count > 0 {
        args.push("--secret".into());
        let n = harness.divine_count * multiplier;
        let vals: Vec<String> = vec!["0".into(); n];
        args.push(vals.join(","));
    }

    if harness.merkle_count > 0 {
        args.push("--digests".into());
        let n = harness.merkle_count * multiplier * 5;
        let vals: Vec<String> = vec!["0".into(); n];
        args.push(vals.join(","));
    }

    args
}

/// Run trisha with harness-computed inputs.
pub fn run_trisha_with_inputs(
    base_args: &[&str],
    harness: &Harness,
) -> Result<TrishaResult, String> {
    let args = trisha_args_with_inputs(base_args, harness);
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_trisha(&str_args)
}
