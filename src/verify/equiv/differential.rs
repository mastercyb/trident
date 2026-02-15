use super::*;

// ─── Step 3: Differential Testing ──────────────────────────────────

/// Check equivalence by building a differential test program and running
/// the verification pipeline on it.
pub(super) fn check_differential(file: &File, fn_a: &str, fn_b: &str) -> EquivalenceResult {
    let program_source = match build_differential_program(file, fn_a, fn_b) {
        Some(src) => src,
        None => {
            return EquivalenceResult {
                fn_a: fn_a.to_string(),
                fn_b: fn_b.to_string(),
                verdict: EquivalenceVerdict::Unknown,
                counterexample: None,
                method: "error: could not build differential program".to_string(),
                tests_passed: 0,
            };
        }
    };

    // Parse the synthetic program.
    let parsed = match crate::parse_source_silent(&program_source, "<equiv>") {
        Ok(f) => f,
        Err(_) => {
            return EquivalenceResult {
                fn_a: fn_a.to_string(),
                fn_b: fn_b.to_string(),
                verdict: EquivalenceVerdict::Unknown,
                counterexample: None,
                method: "error: differential program failed to parse".to_string(),
                tests_passed: 0,
            };
        }
    };

    // Type-check the synthetic program.
    if let Err(_) = crate::typecheck::TypeChecker::new().check_file(&parsed) {
        return EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::Unknown,
            counterexample: None,
            method: "error: differential program failed type-check".to_string(),
            tests_passed: 0,
        };
    }

    // Symbolically analyze and verify.
    let system = crate::sym::analyze(&parsed);
    let report = crate::solve::verify(&system);

    let total_tests = report.random_result.rounds + report.bmc_result.rounds;

    if report.is_safe() {
        EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::Equivalent,
            counterexample: None,
            method: "differential testing (random + BMC)".to_string(),
            tests_passed: total_tests,
        }
    } else {
        // Extract counterexample from the verification report.
        let counterexample = extract_counterexample(&report, fn_a, fn_b);

        EquivalenceResult {
            fn_a: fn_a.to_string(),
            fn_b: fn_b.to_string(),
            verdict: EquivalenceVerdict::NotEquivalent,
            counterexample,
            method: "differential testing (counterexample found)".to_string(),
            tests_passed: total_tests,
        }
    }
}

/// Build a synthetic differential test program.
///
/// The program:
/// 1. Includes both function definitions
/// 2. Generates a main() that reads shared inputs, calls both, asserts equality
fn build_differential_program(file: &File, fn_a: &str, fn_b: &str) -> Option<String> {
    let func_a = find_fn(file, fn_a)?;
    let func_b = find_fn(file, fn_b)?;

    // Get formatted source for each function.
    let src_a = display::format_function(func_a);
    let src_b = display::format_function(func_b);

    // Build input reads and argument lists based on func_a's parameters.
    let mut reads = String::new();
    let mut args = Vec::new();
    for (i, param) in func_a.params.iter().enumerate() {
        let var_name = format!("__input_{}", i);
        let ty_str = format_type(&param.ty.node);
        // For most types, use pub_read().
        // For Digest, use pub_read5(). For XField, three reads.
        let read_call = match &param.ty.node {
            Type::Digest => "pub_read5()",
            _ => "pub_read()",
        };
        reads.push_str(&format!(
            "    let {}: {} = {}\n",
            var_name, ty_str, read_call
        ));
        args.push(var_name);
    }

    let args_str = args.join(", ");

    // Build the main function.
    let has_return = func_a.return_ty.is_some();
    let main_body = if has_return {
        format!(
            "{}\
    let __result_a: Field = {}({})\n\
    let __result_b: Field = {}({})\n\
    assert_eq(__result_a, __result_b)\n",
            reads, fn_a, args_str, fn_b, args_str
        )
    } else {
        // Void functions: just call both (checks only side effects/assertions).
        format!(
            "{}\
    {}({})\n\
    {}({})\n",
            reads, fn_a, args_str, fn_b, args_str
        )
    };

    // Assemble the program.
    let mut program = String::new();
    program.push_str("program __equiv_test\n\n");
    program.push_str(&src_a);
    program.push('\n');
    program.push_str(&src_b);
    program.push('\n');
    program.push_str("fn main() {\n");
    program.push_str(&main_body);
    program.push_str("}\n");

    Some(program)
}

/// Extract a counterexample from the verification report.
fn extract_counterexample(
    report: &crate::solve::VerificationReport,
    _fn_a: &str,
    _fn_b: &str,
) -> Option<EquivalenceCounterexample> {
    // Look for a counterexample in random results first, then BMC.
    let ce = report
        .random_result
        .counterexamples
        .first()
        .or_else(|| report.bmc_result.counterexamples.first())?;

    let mut inputs = Vec::new();
    let mut sorted_assignments: Vec<_> = ce.assignments.iter().collect();
    sorted_assignments.sort_by_key(|(k, _)| (*k).clone());

    for (name, value) in &sorted_assignments {
        if name.starts_with("pub_in_") || name.starts_with("__input_") {
            inputs.push((name.to_string(), **value));
        }
    }

    // Try to extract the output values for each function.
    let output_a = sorted_assignments
        .iter()
        .find(|(k, _)| k.contains("result_a") || k.contains("__call_"))
        .map(|(_, v)| **v)
        .unwrap_or(0);
    let output_b = sorted_assignments
        .iter()
        .find(|(k, _)| k.contains("result_b"))
        .map(|(_, v)| **v)
        .unwrap_or(0);

    Some(EquivalenceCounterexample {
        inputs,
        output_a,
        output_b,
    })
}

