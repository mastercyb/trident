use super::*;

pub fn analyze_costs(source: &str, filename: &str) -> Result<cost::ProgramCost, Vec<Diagnostic>> {
    let file = crate::parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    let cost = cost::CostAnalyzer::default().analyze_file(&file);
    Ok(cost)
}

/// Parse, type-check, and compute cost analysis for a multi-module project.
/// Falls back to single-file analysis if module resolution fails.
pub fn analyze_costs_project(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<cost::ProgramCost, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(entry_path, options)?;

    // Analyze costs for the program file (last in topological order)
    if let Some(file) = project.last_file() {
        let cost = cost::CostAnalyzer::for_target(&options.target_config.name).analyze_file(file);
        Ok(cost)
    } else {
        Err(vec![Diagnostic::error(
            "no program file found".to_string(),
            span::Span::dummy(),
        )])
    }
}

/// Parse, type-check, and verify a project using symbolic execution + solver.
///
/// Returns a `VerificationReport` with static analysis, random testing (Schwartz-Zippel),
/// and bounded model checking results.
pub fn verify_project(entry_path: &Path) -> Result<solve::VerificationReport, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build_default(entry_path)?;

    if let Some(file) = project.last_file() {
        let system = sym::analyze(file);
        Ok(solve::verify(&system))
    } else {
        Err(vec![Diagnostic::error(
            "no program file found".to_string(),
            span::Span::dummy(),
        )])
    }
}

/// Count the number of TASM instructions in a compiled output string.
/// Skips comments, labels, blank lines, and the halt instruction.
pub fn count_tasm_instructions(tasm: &str) -> usize {
    tasm.lines()
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty() && !line.starts_with("//") && !line.ends_with(':') && *line != "halt"
        })
        .count()
}

/// Benchmark result for a single program.
#[derive(Clone, Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub trident_instructions: usize,
    pub baseline_instructions: usize,
    pub overhead_ratio: f64,
    pub trident_padded_height: u64,
    pub baseline_padded_height: u64,
}

impl BenchmarkResult {
    pub fn format(&self) -> String {
        format!(
            "{:<24} {:>6} {:>6}  {:>5.2}x  {:>6} {:>6}",
            self.name,
            self.trident_instructions,
            self.baseline_instructions,
            self.overhead_ratio,
            self.trident_padded_height,
            self.baseline_padded_height,
        )
    }

    pub fn format_header() -> String {
        format!(
            "{:<24} {:>6} {:>6}  {:>6}  {:>6} {:>6}",
            "Benchmark", "Tri", "Hand", "Ratio", "TriPad", "HandPad"
        )
    }

    pub fn format_separator() -> String {
        "-".repeat(72)
    }
}

/// Generate markdown documentation for a Trident project.
pub fn generate_docs(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    doc::generate_docs(entry_path, options)
}

/// Parse, type-check, and produce per-line cost-annotated source output.
pub fn annotate_source(source: &str, filename: &str) -> Result<String, Vec<Diagnostic>> {
    annotate_source_with_target(source, filename, "triton")
}

/// Like `annotate_source`, but uses the specified target's cost model.
pub fn annotate_source_with_target(
    source: &str,
    filename: &str,
    target: &str,
) -> Result<String, Vec<Diagnostic>> {
    let file = crate::parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    let mut analyzer = cost::CostAnalyzer::for_target(target);
    let pc = analyzer.analyze_file(&file);
    let short_names = pc.short_names();
    let stmt_costs = analyzer.stmt_costs(&file, source);

    // Build a map from line number to aggregated cost
    let mut line_costs: BTreeMap<u32, cost::TableCost> = BTreeMap::new();
    for (line, cost) in &stmt_costs {
        line_costs
            .entry(*line)
            .and_modify(|existing| *existing = existing.add(cost))
            .or_insert_with(|| cost.clone());
    }

    let lines: Vec<&str> = source.lines().collect();
    let line_count = lines.len();
    let line_num_width = format!("{}", line_count).len().max(2);

    // Find max line length for alignment
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0).min(60);

    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let line_num = (i + 1) as u32;
        let padded_line = format!("{:<width$}", line, width = max_line_len);
        if let Some(cost) = line_costs.get(&line_num) {
            let annotation = cost.format_annotation(&short_names);
            if !annotation.is_empty() {
                out.push_str(&format!(
                    "{:>width$} | {}  [{}]\n",
                    line_num,
                    padded_line,
                    annotation,
                    width = line_num_width,
                ));
                continue;
            }
        }
        out.push_str(&format!(
            "{:>width$} | {}\n",
            line_num,
            line,
            width = line_num_width,
        ));
    }

    Ok(out)
}

/// Format Trident source code, preserving comments.
pub fn format_source(source: &str, _filename: &str) -> Result<String, Vec<Diagnostic>> {
    let (tokens, comments, lex_errors) = lexer::Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    let file = parser::Parser::new(tokens).parse_file()?;
    Ok(format::format_file(&file, &comments))
}

/// Type-check only, without rendering diagnostics to stderr.
/// Used by the LSP server to get structured errors.
pub fn check_silent(source: &str, filename: &str) -> Result<(), Vec<Diagnostic>> {
    let file = crate::parse_source_silent(source, filename)?;
    TypeChecker::new().check_file(&file)?;
    Ok(())
}

/// Project-aware type-check for the LSP.
/// Finds trident.toml, resolves dependencies, and type-checks
/// the given file with full module context.
/// Falls back to single-file check if no project is found.
pub fn check_file_in_project(source: &str, file_path: &Path) -> Result<(), Vec<Diagnostic>> {
    let dir = file_path.parent().unwrap_or(Path::new("."));
    let entry = match project::Project::find(dir) {
        Some(toml_path) => match project::Project::load(&toml_path) {
            Ok(p) => p.entry,
            Err(_) => file_path.to_path_buf(),
        },
        None => file_path.to_path_buf(),
    };

    // Resolve all modules from the entry point (handles std.* even without project)
    let modules = match resolve_modules(&entry) {
        Ok(m) => m,
        Err(_) => return check_silent(source, &file_path.to_string_lossy()),
    };

    // Parse and type-check all modules in dependency order
    let mut all_exports: Vec<ModuleExports> = Vec::new();
    let file_path_canon = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());

    for module in &modules {
        let mod_path_canon = module
            .file_path
            .canonicalize()
            .unwrap_or_else(|_| module.file_path.clone());
        let is_target = mod_path_canon == file_path_canon;

        // Use live buffer for the file being edited
        let src = if is_target { source } else { &module.source };
        let parsed = crate::parse_source_silent(src, &module.file_path.to_string_lossy())?;

        let mut tc = TypeChecker::new();
        for exports in &all_exports {
            tc.import_module(exports);
        }

        match tc.check_file(&parsed) {
            Ok(exports) => {
                all_exports.push(exports);
            }
            Err(errors) => {
                if is_target {
                    return Err(errors);
                }
                // Dep has errors — stop, but don't report
                // dep errors as if they're in this file
                return Ok(());
            }
        }
    }

    Ok(())
}
