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

/// Wrap a hand-written baseline .tasm (function library) into a
/// standalone program that Triton VM can execute.
///
/// The baseline contains function definitions (`__funcname:` ... `return`)
/// but no entry point. We prepend `halt` so the program terminates
/// immediately — the function bodies are still parsed and validated
/// by the VM. This proves syntactic validity and enables prove/verify.
pub fn wrap_baseline_tasm(baseline: &str) -> String {
    let mut out = String::with_capacity(baseline.len() + 20);
    out.push_str("    halt\n");
    for line in baseline.lines() {
        let trimmed = line.trim();
        // Strip comment-only lines (triton-vm rejects bare "//" and "// ...")
        if trimmed.starts_with("//") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
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
        return Err(stderr.trim().to_string());
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
