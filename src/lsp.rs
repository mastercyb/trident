use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::ast::{self, Item};
use crate::resolve::resolve_modules;
use crate::typeck::{ModuleExports, TypeChecker};
use crate::types::Ty;

pub struct TridentLsp {
    client: Client,
    documents: Mutex<HashMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for TridentLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "trident-lsp initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let source = params.text_document.text.clone();
        self.documents
            .lock()
            .unwrap()
            .insert(uri.clone(), source.clone());
        self.publish_diagnostics(uri, &source).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents
                .lock()
                .unwrap()
                .insert(uri.clone(), change.text.clone());
            self.publish_diagnostics(uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let filename = uri.path();
        match crate::format_source(&source, filename) {
            Ok(formatted) => {
                if formatted == source {
                    return Ok(None);
                }
                let line_count = source.lines().count() as u32;
                let last_line_len = source.lines().last().map_or(0, |l| l.len()) as u32;
                Ok(Some(vec![TextEdit {
                    range: Range::new(
                        Position::new(0, 0),
                        Position::new(line_count, last_line_len),
                    ),
                    new_text: formatted,
                }]))
            }
            Err(_) => Ok(None),
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let file = match crate::parse_source_silent(&source, uri.path()) {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let mut symbols = Vec::new();
        for item in &file.items {
            let (name, kind, detail) = match &item.node {
                Item::Fn(f) => {
                    let sig = format_fn_signature(f);
                    (f.name.node.clone(), SymbolKind::FUNCTION, Some(sig))
                }
                Item::Struct(s) => (s.name.node.clone(), SymbolKind::STRUCT, None),
                Item::Const(c) => (c.name.node.clone(), SymbolKind::CONSTANT, None),
                Item::Event(e) => (e.name.node.clone(), SymbolKind::EVENT, None),
            };

            let range = span_to_range(&source, item.span);
            let selection_range = match &item.node {
                Item::Fn(f) => span_to_range(&source, f.name.span),
                Item::Struct(s) => span_to_range(&source, s.name.span),
                Item::Const(c) => span_to_range(&source, c.name.span),
                Item::Event(e) => span_to_range(&source, e.name.span),
            };

            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name,
                detail,
                kind,
                tags: None,
                deprecated: None,
                range,
                selection_range,
                children: None,
            });
        }

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let source = match self.documents.lock().unwrap().get(uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let word = word_at_position(&source, pos);
        if word.is_empty() {
            return Ok(None);
        }

        // Build symbol index from project
        let file_path = PathBuf::from(uri.path());
        let index = self.build_symbol_index(&file_path);

        // Try exact name, then qualified forms
        if let Some((target_uri, range)) = index.get(&word) {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri.clone(),
                range: *range,
            })));
        }

        // Try with module prefix: if word is "foo", check "*.foo" patterns
        for (key, (target_uri, range)) in &index {
            if key.ends_with(&format!(".{}", word)) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri.clone(),
                    range: *range,
                })));
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let source = match self.documents.lock().unwrap().get(uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let word = word_at_position(&source, pos);
        if word.is_empty() {
            return Ok(None);
        }

        // Check builtins first
        if let Some(mut info) = builtin_hover(&word) {
            let cost = crate::cost::cost_builtin(&word);
            info = format!("{}\n\n**Cost:** {}", info, format_cost_inline(&cost));
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: None,
            }));
        }

        // Check project exports
        let file_path = PathBuf::from(uri.path());
        let exports = self.collect_project_exports(&file_path);
        for exp in &exports {
            // Functions
            for (fname, params, ret_ty) in &exp.functions {
                let bare = fname.rsplit('.').next().unwrap_or(fname);
                if bare == word || *fname == word {
                    let params_str: Vec<String> = params
                        .iter()
                        .map(|(n, t)| format!("{}: {}", n, t.display()))
                        .collect();
                    let ret = if *ret_ty == Ty::Unit {
                        String::new()
                    } else {
                        format!(" -> {}", ret_ty.display())
                    };
                    let mut info = format!(
                        "```trident\nfn {}({}){}\n```",
                        fname,
                        params_str.join(", "),
                        ret
                    );
                    // Compute cost for this user-defined function
                    if let Some(cost) = self.compute_function_cost(&file_path, bare) {
                        info = format!("{}\n\n**Cost:** {}", info, format_cost_inline(&cost));
                    }
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: info,
                        }),
                        range: None,
                    }));
                }
            }

            // Structs
            for st in &exp.structs {
                let bare = st.name.rsplit('.').next().unwrap_or(&st.name);
                if bare == word || st.name == word {
                    let fields: Vec<String> = st
                        .fields
                        .iter()
                        .map(|(n, t, _)| format!("    {}: {}", n, t.display()))
                        .collect();
                    let info = format!(
                        "```trident\nstruct {} {{\n{}\n}}\n```\nWidth: {} field elements",
                        st.name,
                        fields.join(",\n"),
                        st.width()
                    );
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: info,
                        }),
                        range: None,
                    }));
                }
            }

            // Constants
            for (cname, ty, value) in &exp.constants {
                let bare = cname.rsplit('.').next().unwrap_or(cname);
                if bare == word || *cname == word {
                    let info = format!(
                        "```trident\nconst {}: {} = {}\n```",
                        cname,
                        ty.display(),
                        value
                    );
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: info,
                        }),
                        range: None,
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let source = match self.documents.lock().unwrap().get(uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let mut items = Vec::new();

        // Check if we're after a dot (module member completion)
        let prefix = text_before_dot(&source, pos);
        if let Some(module_prefix) = prefix {
            let file_path = PathBuf::from(uri.path());
            let exports = self.collect_project_exports(&file_path);
            for exp in &exports {
                let mod_short = exp
                    .module_name
                    .rsplit('.')
                    .next()
                    .unwrap_or(&exp.module_name);
                if mod_short != module_prefix && exp.module_name != module_prefix {
                    continue;
                }

                // Offer functions
                for (fname, params, ret_ty) in &exp.functions {
                    let bare = fname.rsplit('.').next().unwrap_or(fname);
                    let params_str: Vec<String> = params
                        .iter()
                        .map(|(n, t)| format!("{}: {}", n, t.display()))
                        .collect();
                    let ret = if *ret_ty == Ty::Unit {
                        String::new()
                    } else {
                        format!(" -> {}", ret_ty.display())
                    };
                    items.push(CompletionItem {
                        label: bare.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(format!("fn({}){}", params_str.join(", "), ret)),
                        ..Default::default()
                    });
                }

                // Offer constants
                for (cname, ty, _val) in &exp.constants {
                    let bare = cname.rsplit('.').next().unwrap_or(cname);
                    items.push(CompletionItem {
                        label: bare.to_string(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(ty.display()),
                        ..Default::default()
                    });
                }

                // Offer structs
                for st in &exp.structs {
                    let bare = st.name.rsplit('.').next().unwrap_or(&st.name);
                    items.push(CompletionItem {
                        label: bare.to_string(),
                        kind: Some(CompletionItemKind::STRUCT),
                        detail: Some(format!("struct ({} fields)", st.fields.len())),
                        ..Default::default()
                    });
                }
            }

            return Ok(Some(CompletionResponse::Array(items)));
        }

        // General completions: keywords + builtins + imported module names
        let keywords = [
            "fn", "let", "mut", "const", "struct", "event", "if", "else", "for", "in", "bounded",
            "return", "use", "pub", "emit", "seal", "true", "false",
        ];
        for kw in &keywords {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }

        // Type keywords
        let type_kws = ["Field", "XField", "Bool", "U32", "Digest"];
        for ty in &type_kws {
            items.push(CompletionItem {
                label: ty.to_string(),
                kind: Some(CompletionItemKind::TYPE_PARAMETER),
                ..Default::default()
            });
        }

        // Builtin functions
        for (name, detail) in builtin_completions() {
            items.push(CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail),
                ..Default::default()
            });
        }

        // Imported module names from the current file
        if let Ok(file) = crate::parse_source_silent(&source, uri.path()) {
            for use_stmt in &file.uses {
                let short = use_stmt
                    .node
                    .0
                    .last()
                    .cloned()
                    .unwrap_or_else(|| use_stmt.node.as_dotted());
                items.push(CompletionItem {
                    label: short,
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(format!("module {}", use_stmt.node.as_dotted())),
                    ..Default::default()
                });
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let source = match self.documents.lock().unwrap().get(uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };

        let (fn_name, active_param) = match find_call_context(&source, pos) {
            Some(ctx) => ctx,
            None => return Ok(None),
        };

        // Strip module prefix for builtin lookup
        let bare_name = fn_name.rsplit('.').next().unwrap_or(&fn_name);

        // Try builtins first
        if let Some((params, ret_ty)) = builtin_signature(bare_name) {
            let params_str: Vec<String> = params
                .iter()
                .map(|(n, t)| format!("{}: {}", n, t))
                .collect();
            let ret = if ret_ty.is_empty() {
                String::new()
            } else {
                format!(" -> {}", ret_ty)
            };
            let label = format!("fn {}({}){}", bare_name, params_str.join(", "), ret);
            let parameters: Vec<ParameterInformation> = params
                .iter()
                .map(|(n, t)| ParameterInformation {
                    label: ParameterLabel::Simple(format!("{}: {}", n, t)),
                    documentation: None,
                })
                .collect();

            let sig_info = SignatureInformation {
                label,
                documentation: None,
                parameters: Some(parameters),
                active_parameter: Some(active_param),
            };

            return Ok(Some(SignatureHelp {
                signatures: vec![sig_info],
                active_signature: Some(0),
                active_parameter: Some(active_param),
            }));
        }

        // Try project exports
        let file_path = PathBuf::from(uri.path());
        let exports = self.collect_project_exports(&file_path);
        for exp in &exports {
            for (fname, fn_params, ret_ty) in &exp.functions {
                let exp_bare = fname.rsplit('.').next().unwrap_or(fname);
                if exp_bare == bare_name || *fname == fn_name {
                    let params_str: Vec<String> = fn_params
                        .iter()
                        .map(|(n, t)| format!("{}: {}", n, t.display()))
                        .collect();
                    let ret = if *ret_ty == Ty::Unit {
                        String::new()
                    } else {
                        format!(" -> {}", ret_ty.display())
                    };
                    let label = format!("fn {}({}){}", exp_bare, params_str.join(", "), ret);
                    let parameters: Vec<ParameterInformation> = fn_params
                        .iter()
                        .map(|(n, t)| ParameterInformation {
                            label: ParameterLabel::Simple(format!("{}: {}", n, t.display())),
                            documentation: None,
                        })
                        .collect();

                    let sig_info = SignatureInformation {
                        label,
                        documentation: None,
                        parameters: Some(parameters),
                        active_parameter: Some(active_param),
                    };

                    return Ok(Some(SignatureHelp {
                        signatures: vec![sig_info],
                        active_signature: Some(0),
                        active_parameter: Some(active_param),
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl TridentLsp {
    async fn publish_diagnostics(&self, uri: Url, source: &str) {
        let file_path = PathBuf::from(uri.path());
        let result = crate::check_file_in_project(source, &file_path);

        let diagnostics = match result {
            Ok(()) => Vec::new(),
            Err(errors) => errors
                .into_iter()
                .map(|d| to_lsp_diagnostic(&d, source))
                .collect(),
        };

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Build a symbol index mapping names to (uri, range) for go-to-definition.
    fn build_symbol_index(&self, file_path: &Path) -> HashMap<String, (Url, Range)> {
        let mut index = HashMap::new();

        let dir = file_path.parent().unwrap_or(std::path::Path::new("."));
        let entry = match crate::project::Project::find(dir) {
            Some(toml_path) => match crate::project::Project::load(&toml_path) {
                Ok(p) => p.entry,
                Err(_) => file_path.to_path_buf(),
            },
            None => file_path.to_path_buf(),
        };

        let modules = match resolve_modules(&entry) {
            Ok(m) => m,
            Err(_) => return index,
        };

        for module in &modules {
            let parsed = match crate::parse_source_silent(
                &module.source,
                &module.file_path.to_string_lossy(),
            ) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let mod_uri = Url::from_file_path(&module.file_path).unwrap_or_else(|_| {
                Url::parse(&format!("file://{}", module.file_path.display())).unwrap()
            });
            let mod_short = module.name.rsplit('.').next().unwrap_or(&module.name);

            for item in &parsed.items {
                let (name, name_span) = match &item.node {
                    Item::Fn(f) => (f.name.node.clone(), f.name.span),
                    Item::Struct(s) => (s.name.node.clone(), s.name.span),
                    Item::Const(c) => (c.name.node.clone(), c.name.span),
                    Item::Event(e) => (e.name.node.clone(), e.name.span),
                };

                let range = span_to_range(&module.source, name_span);
                let qualified = format!("{}.{}", mod_short, name);
                let full_qualified = format!("{}.{}", module.name, name);

                // Register under bare name (last definition wins — entry file takes precedence)
                index.insert(name.clone(), (mod_uri.clone(), range));
                // Register under short qualified name
                index.insert(qualified, (mod_uri.clone(), range));
                // Register under full qualified name
                if full_qualified != format!("{}.{}", mod_short, name) {
                    index.insert(full_qualified, (mod_uri.clone(), range));
                }
            }
        }

        index
    }

    /// Collect type-checked exports from all project modules.
    fn collect_project_exports(&self, file_path: &Path) -> Vec<ModuleExports> {
        let dir = file_path.parent().unwrap_or(Path::new("."));
        let entry = match crate::project::Project::find(dir) {
            Some(toml_path) => match crate::project::Project::load(&toml_path) {
                Ok(p) => p.entry,
                Err(_) => file_path.to_path_buf(),
            },
            None => file_path.to_path_buf(),
        };

        let modules = match resolve_modules(&entry) {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let mut all_exports = Vec::new();
        for module in &modules {
            let parsed = match crate::parse_source_silent(
                &module.source,
                &module.file_path.to_string_lossy(),
            ) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let mut tc = TypeChecker::new();
            for exports in &all_exports {
                tc.import_module(exports);
            }

            match tc.check_file(&parsed) {
                Ok(exports) => all_exports.push(exports),
                Err(_) => continue,
            }
        }

        all_exports
    }

    /// Compute cost for a specific user-defined function by name.
    ///
    /// Resolves project modules, parses sources, runs CostAnalyzer, and
    /// looks up the named function in the resulting ProgramCost.
    fn compute_function_cost(
        &self,
        file_path: &Path,
        fn_name: &str,
    ) -> Option<crate::cost::TableCost> {
        let dir = file_path.parent().unwrap_or(Path::new("."));
        let entry = match crate::project::Project::find(dir) {
            Some(toml_path) => match crate::project::Project::load(&toml_path) {
                Ok(p) => p.entry,
                Err(_) => file_path.to_path_buf(),
            },
            None => file_path.to_path_buf(),
        };

        let modules = resolve_modules(&entry).ok()?;

        // Find the module that contains this function and analyze it
        for module in &modules {
            let parsed =
                crate::parse_source_silent(&module.source, &module.file_path.to_string_lossy())
                    .ok()?;

            // Check if this module contains the function
            let has_fn = parsed.items.iter().any(|item| {
                if let Item::Fn(f) = &item.node {
                    f.name.node == fn_name
                } else {
                    false
                }
            });

            if has_fn {
                let mut analyzer = crate::cost::CostAnalyzer::new();
                let program_cost = analyzer.analyze_file(&parsed);
                for fc in &program_cost.functions {
                    if fc.name == fn_name {
                        return Some(fc.cost.clone());
                    }
                }
            }
        }

        None
    }
}

// --- Helpers ---

fn to_lsp_diagnostic(
    diag: &crate::diagnostic::Diagnostic,
    source: &str,
) -> tower_lsp::lsp_types::Diagnostic {
    let start = byte_offset_to_position(source, diag.span.start as usize);
    let end = byte_offset_to_position(source, diag.span.end as usize);

    let severity = match diag.severity {
        crate::diagnostic::Severity::Error => DiagnosticSeverity::ERROR,
        crate::diagnostic::Severity::Warning => DiagnosticSeverity::WARNING,
    };

    let mut message = diag.message.clone();
    for note in &diag.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    if let Some(help) = &diag.help {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }

    tower_lsp::lsp_types::Diagnostic {
        range: Range::new(start, end),
        severity: Some(severity),
        source: Some("trident".to_string()),
        message,
        ..Default::default()
    }
}

fn byte_offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position::new(line, col)
}

fn span_to_range(source: &str, span: crate::span::Span) -> Range {
    Range::new(
        byte_offset_to_position(source, span.start as usize),
        byte_offset_to_position(source, span.end as usize),
    )
}

/// Extract the word (identifier) at a given cursor position.
fn word_at_position(source: &str, pos: Position) -> String {
    let Some(offset) = position_to_byte_offset(source, pos) else {
        return String::new();
    };

    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    // Include dot for qualified names like "hash.tip5"
    if start > 0 && bytes[start - 1] == b'.' {
        let mut dot_start = start - 1;
        while dot_start > 0 && is_ident_char(bytes[dot_start - 1]) {
            dot_start -= 1;
        }
        source[dot_start..end].to_string()
    } else if end < bytes.len() && bytes[end] == b'.' {
        let mut dot_end = end + 1;
        while dot_end < bytes.len() && is_ident_char(bytes[dot_end]) {
            dot_end += 1;
        }
        source[start..dot_end].to_string()
    } else {
        source[start..end].to_string()
    }
}

/// Check if there's a dot before the cursor and return the module prefix.
fn text_before_dot(source: &str, pos: Position) -> Option<String> {
    let offset = position_to_byte_offset(source, pos)?;
    let bytes = source.as_bytes();

    // Walk back from cursor to find the dot
    let mut i = offset;
    while i > 0 && is_ident_char(bytes[i - 1]) {
        i -= 1;
    }
    // Check if there's a dot right before the identifier start
    if i > 0 && bytes[i - 1] == b'.' {
        let dot_pos = i - 1;
        let mut start = dot_pos;
        while start > 0 && is_ident_char(bytes[start - 1]) {
            start -= 1;
        }
        if start < dot_pos {
            return Some(source[start..dot_pos].to_string());
        }
    }
    None
}

fn position_to_byte_offset(source: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return Some(i);
        }
        if ch == '\n' {
            if line == pos.line {
                return Some(i);
            }
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    if line == pos.line {
        Some(source.len())
    } else {
        None
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn format_fn_signature(f: &ast::FnDef) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.node, format_ast_type(&p.ty.node)))
        .collect();
    let ret = match &f.return_ty {
        Some(ty) => format!(" -> {}", format_ast_type(&ty.node)),
        None => String::new(),
    };
    format!("fn {}({}){}", f.name.node, params.join(", "), ret)
}

fn format_ast_type(ty: &ast::Type) -> String {
    match ty {
        ast::Type::Field => "Field".to_string(),
        ast::Type::XField => "XField".to_string(),
        ast::Type::Bool => "Bool".to_string(),
        ast::Type::U32 => "U32".to_string(),
        ast::Type::Digest => "Digest".to_string(),
        ast::Type::Array(inner, n) => format!("[{}; {}]", format_ast_type(inner), n),
        ast::Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(format_ast_type).collect();
            format!("({})", parts.join(", "))
        }
        ast::Type::Named(path) => path.as_dotted(),
    }
}

/// Format a `TableCost` as a compact inline string for hover display.
///
/// Only shows non-zero tables (except `cc` which is always shown).
/// Example output: `cc=1, hash=6 | dominant: hash`
fn format_cost_inline(cost: &crate::cost::TableCost) -> String {
    let mut parts = vec![format!("cc={}", cost.processor)];
    if cost.hash > 0 {
        parts.push(format!("hash={}", cost.hash));
    }
    if cost.u32_table > 0 {
        parts.push(format!("u32={}", cost.u32_table));
    }
    if cost.op_stack > 0 {
        parts.push(format!("opstack={}", cost.op_stack));
    }
    if cost.ram > 0 {
        parts.push(format!("ram={}", cost.ram));
    }
    if cost.jump_stack > 0 {
        parts.push(format!("jump={}", cost.jump_stack));
    }
    format!("{} | dominant: {}", parts.join(", "), cost.dominant_table())
}

/// Hover info for builtin functions.
fn builtin_hover(name: &str) -> Option<String> {
    let info = match name {
        "pub_read" => "```trident\nfn pub_read() -> Field\n```\nRead one field element from public input.",
        "pub_read2" => "```trident\nfn pub_read2() -> (Field, Field)\n```\nRead 2 field elements from public input.",
        "pub_read3" => "```trident\nfn pub_read3() -> (Field, Field, Field)\n```\nRead 3 field elements from public input.",
        "pub_read4" => "```trident\nfn pub_read4() -> (Field, Field, Field, Field)\n```\nRead 4 field elements from public input.",
        "pub_read5" => "```trident\nfn pub_read5() -> Digest\n```\nRead 5 field elements (Digest) from public input.",
        "pub_write" => "```trident\nfn pub_write(v: Field)\n```\nWrite one field element to public output.",
        "pub_write2" => "```trident\nfn pub_write2(a: Field, b: Field)\n```\nWrite 2 field elements to public output.",
        "pub_write3" => "```trident\nfn pub_write3(a: Field, b: Field, c: Field)\n```\nWrite 3 field elements to public output.",
        "pub_write4" => "```trident\nfn pub_write4(a: Field, b: Field, c: Field, d: Field)\n```\nWrite 4 field elements to public output.",
        "pub_write5" => "```trident\nfn pub_write5(a: Field, b: Field, c: Field, d: Field, e: Field)\n```\nWrite 5 field elements to public output.",
        "divine" => "```trident\nfn divine() -> Field\n```\nRead one non-deterministic field element (secret witness).",
        "divine3" => "```trident\nfn divine3() -> (Field, Field, Field)\n```\nRead 3 non-deterministic field elements.",
        "divine5" => "```trident\nfn divine5() -> Digest\n```\nRead 5 non-deterministic field elements (Digest).",
        "assert" => "```trident\nfn assert(cond: Bool)\n```\nAbort execution if condition is false.",
        "assert_eq" => "```trident\nfn assert_eq(a: Field, b: Field)\n```\nAbort execution if a != b.",
        "assert_digest_eq" => "```trident\nfn assert_digest_eq(a: Digest, b: Digest)\n```\nAbort execution if digests are not equal.",
        "hash" => "```trident\nfn hash(x0..x9: Field) -> Digest\n```\nTip5 hash of 10 field elements.",
        "sponge_init" => "```trident\nfn sponge_init()\n```\nInitialize the Tip5 sponge state.",
        "sponge_absorb" => "```trident\nfn sponge_absorb(x0..x9: Field)\n```\nAbsorb 10 field elements into the sponge.",
        "sponge_squeeze" => "```trident\nfn sponge_squeeze() -> [Field; 10]\n```\nSqueeze 10 field elements from the sponge.",
        "split" => "```trident\nfn split(a: Field) -> (U32, U32)\n```\nSplit field element into (hi, lo) u32 limbs.",
        "log2" => "```trident\nfn log2(a: U32) -> U32\n```\nFloor of log base 2.",
        "pow" => "```trident\nfn pow(base: U32, exp: U32) -> U32\n```\nInteger exponentiation.",
        "popcount" => "```trident\nfn popcount(a: U32) -> U32\n```\nCount set bits.",
        "as_u32" => "```trident\nfn as_u32(a: Field) -> U32\n```\nRange-check and convert field to u32.",
        "as_field" => "```trident\nfn as_field(a: U32) -> Field\n```\nConvert u32 to field element.",
        "field_add" => "```trident\nfn field_add(a: Field, b: Field) -> Field\n```\nField addition.",
        "field_mul" => "```trident\nfn field_mul(a: Field, b: Field) -> Field\n```\nField multiplication.",
        "inv" => "```trident\nfn inv(a: Field) -> Field\n```\nField multiplicative inverse.",
        "neg" => "```trident\nfn neg(a: Field) -> Field\n```\nField negation.",
        "sub" => "```trident\nfn sub(a: Field, b: Field) -> Field\n```\nField subtraction.",
        "ram_read" => "```trident\nfn ram_read(addr: Field) -> Field\n```\nRead one field element from RAM.",
        "ram_write" => "```trident\nfn ram_write(addr: Field, val: Field)\n```\nWrite one field element to RAM.",
        "ram_read_block" => "```trident\nfn ram_read_block(addr: Field) -> Digest\n```\nRead 5 consecutive field elements from RAM.",
        "ram_write_block" => "```trident\nfn ram_write_block(addr: Field, d: Digest)\n```\nWrite 5 consecutive field elements to RAM.",
        "merkle_step" => "```trident\nfn merkle_step(idx: U32, d0..d4: Field) -> (U32, Digest)\n```\nOne step of Merkle tree authentication.",
        "xfield" => "```trident\nfn xfield(a: Field, b: Field, c: Field) -> XField\n```\nConstruct extension field element.",
        "xinvert" => "```trident\nfn xinvert(a: XField) -> XField\n```\nExtension field multiplicative inverse.",
        _ => return None,
    };
    Some(info.to_string())
}

/// Return the parameter list and return type for a builtin function.
/// Each parameter is `(name, type_name)`. The second element is the return type string.
fn builtin_signature(name: &str) -> Option<(Vec<(&'static str, &'static str)>, &'static str)> {
    let sig: (Vec<(&str, &str)>, &str) = match name {
        "pub_read" => (vec![], "Field"),
        "pub_read2" => (vec![], "(Field, Field)"),
        "pub_read3" => (vec![], "(Field, Field, Field)"),
        "pub_read4" => (vec![], "(Field, Field, Field, Field)"),
        "pub_read5" => (vec![], "Digest"),
        "pub_write" => (vec![("v", "Field")], ""),
        "pub_write2" => (vec![("a", "Field"), ("b", "Field")], ""),
        "pub_write3" => (vec![("a", "Field"), ("b", "Field"), ("c", "Field")], ""),
        "pub_write4" => (
            vec![
                ("a", "Field"),
                ("b", "Field"),
                ("c", "Field"),
                ("d", "Field"),
            ],
            "",
        ),
        "pub_write5" => (
            vec![
                ("a", "Field"),
                ("b", "Field"),
                ("c", "Field"),
                ("d", "Field"),
                ("e", "Field"),
            ],
            "",
        ),
        "divine" => (vec![], "Field"),
        "divine3" => (vec![], "(Field, Field, Field)"),
        "divine5" => (vec![], "Digest"),
        "assert" => (vec![("cond", "Bool")], ""),
        "assert_eq" => (vec![("a", "Field"), ("b", "Field")], ""),
        "assert_digest_eq" => (vec![("a", "Digest"), ("b", "Digest")], ""),
        "hash" => (
            vec![
                ("x0", "Field"),
                ("x1", "Field"),
                ("x2", "Field"),
                ("x3", "Field"),
                ("x4", "Field"),
                ("x5", "Field"),
                ("x6", "Field"),
                ("x7", "Field"),
                ("x8", "Field"),
                ("x9", "Field"),
            ],
            "Digest",
        ),
        "sponge_init" => (vec![], ""),
        "sponge_absorb" => (
            vec![
                ("x0", "Field"),
                ("x1", "Field"),
                ("x2", "Field"),
                ("x3", "Field"),
                ("x4", "Field"),
                ("x5", "Field"),
                ("x6", "Field"),
                ("x7", "Field"),
                ("x8", "Field"),
                ("x9", "Field"),
            ],
            "",
        ),
        "sponge_squeeze" => (vec![], "[Field; 10]"),
        "split" => (vec![("a", "Field")], "(U32, U32)"),
        "log2" => (vec![("a", "U32")], "U32"),
        "pow" => (vec![("base", "U32"), ("exp", "U32")], "U32"),
        "popcount" => (vec![("a", "U32")], "U32"),
        "as_u32" => (vec![("a", "Field")], "U32"),
        "as_field" => (vec![("a", "U32")], "Field"),
        "field_add" => (vec![("a", "Field"), ("b", "Field")], "Field"),
        "field_mul" => (vec![("a", "Field"), ("b", "Field")], "Field"),
        "inv" => (vec![("a", "Field")], "Field"),
        "neg" => (vec![("a", "Field")], "Field"),
        "sub" => (vec![("a", "Field"), ("b", "Field")], "Field"),
        "ram_read" => (vec![("addr", "Field")], "Field"),
        "ram_write" => (vec![("addr", "Field"), ("val", "Field")], ""),
        "ram_read_block" => (vec![("addr", "Field")], "Digest"),
        "ram_write_block" => (vec![("addr", "Field"), ("d", "Digest")], ""),
        "merkle_step" => (
            vec![
                ("idx", "U32"),
                ("d0", "Field"),
                ("d1", "Field"),
                ("d2", "Field"),
                ("d3", "Field"),
                ("d4", "Field"),
            ],
            "(U32, Digest)",
        ),
        "xfield" => (
            vec![("a", "Field"), ("b", "Field"), ("c", "Field")],
            "XField",
        ),
        "xinvert" => (vec![("a", "XField")], "XField"),
        _ => return None,
    };
    Some(sig)
}

/// Find the function name and active parameter index at a given position.
fn find_call_context(source: &str, pos: Position) -> Option<(String, u32)> {
    let offset = position_to_byte_offset(source, pos)?;
    let bytes = source.as_bytes();

    // Walk backward to find the matching '('
    let mut depth = 0i32;
    let mut comma_count = 0u32;
    let mut i = offset;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    // Found the opening paren - extract function name
                    let mut name_end = i;
                    while name_end > 0 && bytes[name_end - 1] == b' ' {
                        name_end -= 1;
                    }
                    let mut name_start = name_end;
                    while name_start > 0
                        && (is_ident_char(bytes[name_start - 1]) || bytes[name_start - 1] == b'.')
                    {
                        name_start -= 1;
                    }
                    if name_start < name_end {
                        let name = source[name_start..name_end].to_string();
                        return Some((name, comma_count));
                    }
                    return None;
                }
                depth -= 1;
            }
            b',' if depth == 0 => comma_count += 1,
            _ => {}
        }
    }
    None
}

/// Completion items for all builtin functions.
fn builtin_completions() -> Vec<(String, String)> {
    vec![
        ("pub_read".into(), "() -> Field".into()),
        ("pub_read2".into(), "() -> (Field, Field)".into()),
        ("pub_read3".into(), "() -> (Field, Field, Field)".into()),
        (
            "pub_read4".into(),
            "() -> (Field, Field, Field, Field)".into(),
        ),
        ("pub_read5".into(), "() -> Digest".into()),
        ("pub_write".into(), "(v: Field)".into()),
        ("pub_write2".into(), "(a: Field, b: Field)".into()),
        ("pub_write3".into(), "(a: Field, b: Field, c: Field)".into()),
        (
            "pub_write4".into(),
            "(a: Field, b: Field, c: Field, d: Field)".into(),
        ),
        ("pub_write5".into(), "(a..e: Field)".into()),
        ("divine".into(), "() -> Field".into()),
        ("divine3".into(), "() -> (Field, Field, Field)".into()),
        ("divine5".into(), "() -> Digest".into()),
        ("assert".into(), "(cond: Bool)".into()),
        ("assert_eq".into(), "(a: Field, b: Field)".into()),
        ("assert_digest_eq".into(), "(a: Digest, b: Digest)".into()),
        ("hash".into(), "(x0..x9: Field) -> Digest".into()),
        ("sponge_init".into(), "()".into()),
        ("sponge_absorb".into(), "(x0..x9: Field)".into()),
        ("sponge_squeeze".into(), "() -> [Field; 10]".into()),
        ("split".into(), "(a: Field) -> (U32, U32)".into()),
        ("log2".into(), "(a: U32) -> U32".into()),
        ("pow".into(), "(base: U32, exp: U32) -> U32".into()),
        ("popcount".into(), "(a: U32) -> U32".into()),
        ("as_u32".into(), "(a: Field) -> U32".into()),
        ("as_field".into(), "(a: U32) -> Field".into()),
        ("field_add".into(), "(a: Field, b: Field) -> Field".into()),
        ("field_mul".into(), "(a: Field, b: Field) -> Field".into()),
        ("inv".into(), "(a: Field) -> Field".into()),
        ("neg".into(), "(a: Field) -> Field".into()),
        ("sub".into(), "(a: Field, b: Field) -> Field".into()),
        ("ram_read".into(), "(addr: Field) -> Field".into()),
        ("ram_write".into(), "(addr: Field, val: Field)".into()),
        ("ram_read_block".into(), "(addr: Field) -> Digest".into()),
        ("ram_write_block".into(), "(addr: Field, d: Digest)".into()),
        (
            "merkle_step".into(),
            "(idx: U32, d0..d4: Field) -> (U32, Digest)".into(),
        ),
        (
            "xfield".into(),
            "(a: Field, b: Field, c: Field) -> XField".into(),
        ),
        ("xinvert".into(), "(a: XField) -> XField".into()),
    ]
}

/// Start the LSP server on stdin/stdout.
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| TridentLsp {
        client,
        documents: Mutex::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    // --- byte_offset_to_position ---

    #[test]
    fn test_byte_offset_first_line() {
        let src = "let x = 1\n";
        assert_eq!(byte_offset_to_position(src, 0), Position::new(0, 0));
        assert_eq!(byte_offset_to_position(src, 4), Position::new(0, 4));
    }

    #[test]
    fn test_byte_offset_second_line() {
        // "let x = 1\n" is 10 bytes (indices 0..9), so offset 10 is start of line 2
        let src = "let x = 1\nlet y = 2\n";
        assert_eq!(byte_offset_to_position(src, 10), Position::new(1, 0));
        assert_eq!(byte_offset_to_position(src, 14), Position::new(1, 4));
    }

    #[test]
    fn test_byte_offset_clamps() {
        let src = "abc";
        // offset beyond source length should clamp
        let pos = byte_offset_to_position(src, 999);
        assert_eq!(pos, Position::new(0, 3));
    }

    // --- position_to_byte_offset ---

    #[test]
    fn test_position_to_offset_start() {
        let src = "let x = 1\nlet y = 2\n";
        assert_eq!(position_to_byte_offset(src, Position::new(0, 0)), Some(0));
        assert_eq!(position_to_byte_offset(src, Position::new(0, 4)), Some(4));
        assert_eq!(position_to_byte_offset(src, Position::new(1, 0)), Some(10));
        assert_eq!(position_to_byte_offset(src, Position::new(1, 4)), Some(14));
    }

    #[test]
    fn test_position_to_offset_end_of_line() {
        let src = "abc\ndef\n";
        // Past end of line 0 should return the newline position
        assert_eq!(position_to_byte_offset(src, Position::new(0, 3)), Some(3));
    }

    #[test]
    fn test_position_to_offset_past_end() {
        let src = "abc";
        // Line 5 doesn't exist
        assert_eq!(position_to_byte_offset(src, Position::new(5, 0)), None);
    }

    // --- word_at_position ---

    #[test]
    fn test_word_simple() {
        let src = "let foo = bar\n";
        assert_eq!(word_at_position(src, Position::new(0, 4)), "foo");
        assert_eq!(word_at_position(src, Position::new(0, 10)), "bar");
    }

    #[test]
    fn test_word_at_start() {
        let src = "hello world\n";
        assert_eq!(word_at_position(src, Position::new(0, 0)), "hello");
    }

    #[test]
    fn test_word_qualified_after_dot() {
        let src = "let x = hash.tip5()\n";
        // Cursor on "tip5" — should pick up "hash.tip5"
        assert_eq!(word_at_position(src, Position::new(0, 14)), "hash.tip5");
    }

    #[test]
    fn test_word_qualified_before_dot() {
        let src = "let x = hash.tip5()\n";
        // Cursor on "hash" — should pick up "hash.tip5"
        assert_eq!(word_at_position(src, Position::new(0, 9)), "hash.tip5");
    }

    #[test]
    fn test_word_on_boundary_picks_left() {
        // Cursor right after "let" (on the space) — picks up preceding identifier
        let src = "let x = 1\n";
        assert_eq!(word_at_position(src, Position::new(0, 3)), "let");
    }

    #[test]
    fn test_word_between_symbols_empty() {
        // Cursor on "=" which is not an ident char and has no ident neighbor
        let src = "a = b\n";
        assert_eq!(word_at_position(src, Position::new(0, 2)), "");
    }

    // --- text_before_dot ---

    #[test]
    fn test_dot_completion_prefix() {
        let src = "hash.t";
        // Cursor at end, after dot + partial identifier
        assert_eq!(
            text_before_dot(src, Position::new(0, 6)),
            Some("hash".to_string())
        );
    }

    #[test]
    fn test_dot_completion_right_after_dot() {
        let src = "hash.";
        assert_eq!(
            text_before_dot(src, Position::new(0, 5)),
            Some("hash".to_string())
        );
    }

    #[test]
    fn test_no_dot_prefix() {
        let src = "let x = 1";
        assert_eq!(text_before_dot(src, Position::new(0, 5)), None);
    }

    // --- span_to_range ---

    #[test]
    fn test_span_to_range_single_line() {
        let src = "let foo = 1\n";
        let span = crate::span::Span::new(0, 4, 7);
        let range = span_to_range(src, span);
        assert_eq!(range.start, Position::new(0, 4));
        assert_eq!(range.end, Position::new(0, 7));
    }

    #[test]
    fn test_span_to_range_multi_line() {
        let src = "line1\nline2\nline3\n";
        let span = crate::span::Span::new(0, 6, 17);
        let range = span_to_range(src, span);
        assert_eq!(range.start, Position::new(1, 0));
        assert_eq!(range.end, Position::new(2, 5));
    }

    // --- to_lsp_diagnostic ---

    #[test]
    fn test_lsp_diagnostic_error() {
        let source = "let x: U32 = pub_read()\n";
        let diag = crate::diagnostic::Diagnostic::error(
            "type mismatch".to_string(),
            crate::span::Span::new(0, 13, 23),
        )
        .with_note("expected U32, found Field".to_string());

        let lsp_diag = to_lsp_diagnostic(&diag, source);
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::ERROR));
        assert!(lsp_diag.message.contains("type mismatch"));
        assert!(lsp_diag.message.contains("note: expected U32, found Field"));
        assert_eq!(lsp_diag.source, Some("trident".to_string()));
    }

    #[test]
    fn test_lsp_diagnostic_warning_with_help() {
        let source = "as_u32(x)\n";
        let diag = crate::diagnostic::Diagnostic::warning(
            "redundant".to_string(),
            crate::span::Span::new(0, 0, 9),
        )
        .with_help("already proven".to_string());

        let lsp_diag = to_lsp_diagnostic(&diag, source);
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::WARNING));
        assert!(lsp_diag.message.contains("help: already proven"));
    }

    // --- builtin_hover ---

    #[test]
    fn test_builtin_hover_known() {
        assert!(builtin_hover("pub_read").is_some());
        assert!(builtin_hover("hash").is_some());
        assert!(builtin_hover("split").is_some());
        assert!(builtin_hover("merkle_step").is_some());
    }

    #[test]
    fn test_builtin_hover_unknown() {
        assert!(builtin_hover("nonexistent").is_none());
        assert!(builtin_hover("my_function").is_none());
    }

    #[test]
    fn test_builtin_hover_contains_signature() {
        let info = builtin_hover("pub_read").unwrap();
        assert!(info.contains("fn pub_read()"));
        assert!(info.contains("-> Field"));
    }

    // --- builtin_completions ---

    #[test]
    fn test_builtin_completions_count() {
        let completions = builtin_completions();
        // Should have all builtins
        assert!(
            completions.len() >= 30,
            "expected many builtins, got {}",
            completions.len()
        );
        let names: Vec<&str> = completions.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"pub_read"));
        assert!(names.contains(&"hash"));
        assert!(names.contains(&"split"));
        assert!(names.contains(&"ram_read"));
        assert!(names.contains(&"xinvert"));
    }

    // --- format_fn_signature ---

    #[test]
    fn test_format_fn_signature_no_params() {
        let f = crate::ast::FnDef {
            is_pub: false,
            is_test: false,
            is_pure: false,
            cfg: None,
            intrinsic: None,
            requires: vec![],
            ensures: vec![],
            name: crate::span::Spanned::dummy("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: None,
        };
        assert_eq!(format_fn_signature(&f), "fn main()");
    }

    #[test]
    fn test_format_fn_signature_with_return() {
        let f = crate::ast::FnDef {
            is_pub: true,
            is_test: false,
            is_pure: false,
            cfg: None,
            intrinsic: None,
            requires: vec![],
            ensures: vec![],
            name: crate::span::Spanned::dummy("add".to_string()),
            type_params: vec![],
            params: vec![
                crate::ast::Param {
                    name: crate::span::Spanned::dummy("a".to_string()),
                    ty: crate::span::Spanned::dummy(crate::ast::Type::Field),
                },
                crate::ast::Param {
                    name: crate::span::Spanned::dummy("b".to_string()),
                    ty: crate::span::Spanned::dummy(crate::ast::Type::Field),
                },
            ],
            return_ty: Some(crate::span::Spanned::dummy(crate::ast::Type::Field)),
            body: None,
        };
        assert_eq!(
            format_fn_signature(&f),
            "fn add(a: Field, b: Field) -> Field"
        );
    }

    // --- format_ast_type ---

    #[test]
    fn test_format_ast_types() {
        assert_eq!(format_ast_type(&crate::ast::Type::Field), "Field");
        assert_eq!(format_ast_type(&crate::ast::Type::XField), "XField");
        assert_eq!(format_ast_type(&crate::ast::Type::Bool), "Bool");
        assert_eq!(format_ast_type(&crate::ast::Type::U32), "U32");
        assert_eq!(format_ast_type(&crate::ast::Type::Digest), "Digest");
        assert_eq!(
            format_ast_type(&crate::ast::Type::Array(
                Box::new(crate::ast::Type::Field),
                crate::ast::ArraySize::Literal(5)
            )),
            "[Field; 5]"
        );
        assert_eq!(
            format_ast_type(&crate::ast::Type::Tuple(vec![
                crate::ast::Type::Field,
                crate::ast::Type::U32
            ])),
            "(Field, U32)"
        );
    }

    // --- is_ident_char ---

    #[test]
    fn test_ident_chars() {
        assert!(is_ident_char(b'a'));
        assert!(is_ident_char(b'Z'));
        assert!(is_ident_char(b'0'));
        assert!(is_ident_char(b'_'));
        assert!(!is_ident_char(b'.'));
        assert!(!is_ident_char(b' '));
        assert!(!is_ident_char(b'('));
    }

    // --- find_call_context ---

    #[test]
    fn test_find_call_context_simple() {
        let src = "pub_write(x, y)";
        let ctx = find_call_context(src, Position::new(0, 12));
        assert_eq!(ctx, Some(("pub_write".to_string(), 1)));
    }

    #[test]
    fn test_find_call_context_first_param() {
        let src = "pub_write(x)";
        let ctx = find_call_context(src, Position::new(0, 10));
        assert_eq!(ctx, Some(("pub_write".to_string(), 0)));
    }

    #[test]
    fn test_find_call_context_no_paren() {
        let src = "let x = 1";
        let ctx = find_call_context(src, Position::new(0, 5));
        assert_eq!(ctx, None);
    }

    #[test]
    fn test_find_call_context_nested() {
        // Cursor inside inner call: split(field_add(a, b))
        // At position inside field_add(a, _b_)
        let src = "split(field_add(a, b))";
        let ctx = find_call_context(src, Position::new(0, 19));
        assert_eq!(ctx, Some(("field_add".to_string(), 1)));
    }

    #[test]
    fn test_find_call_context_qualified_name() {
        let src = "math.add(x, y, z)";
        let ctx = find_call_context(src, Position::new(0, 15));
        assert_eq!(ctx, Some(("math.add".to_string(), 2)));
    }

    #[test]
    fn test_find_call_context_right_after_open_paren() {
        let src = "assert(";
        let ctx = find_call_context(src, Position::new(0, 7));
        assert_eq!(ctx, Some(("assert".to_string(), 0)));
    }

    #[test]
    fn test_find_call_context_space_before_paren() {
        let src = "foo (a, b)";
        let ctx = find_call_context(src, Position::new(0, 8));
        assert_eq!(ctx, Some(("foo".to_string(), 1)));
    }

    // --- builtin_signature ---

    #[test]
    fn test_builtin_signature_known() {
        let (params, ret) = builtin_signature("pub_write").unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], ("v", "Field"));
        assert_eq!(ret, "");
    }

    #[test]
    fn test_builtin_signature_with_return() {
        let (params, ret) = builtin_signature("split").unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], ("a", "Field"));
        assert_eq!(ret, "(U32, U32)");
    }

    #[test]
    fn test_builtin_signature_no_params() {
        let (params, ret) = builtin_signature("pub_read").unwrap();
        assert_eq!(params.len(), 0);
        assert_eq!(ret, "Field");
    }

    #[test]
    fn test_builtin_signature_unknown() {
        assert!(builtin_signature("nonexistent").is_none());
    }

    #[test]
    fn test_builtin_signature_multi_params() {
        let (params, ret) = builtin_signature("pow").unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], ("base", "U32"));
        assert_eq!(params[1], ("exp", "U32"));
        assert_eq!(ret, "U32");
    }

    // --- format_cost_inline ---

    #[test]
    fn test_format_cost_inline_zero() {
        let cost = crate::cost::TableCost::ZERO;
        let s = format_cost_inline(&cost);
        assert!(s.contains("cc=0"), "should contain cc=0, got: {}", s);
        assert!(
            s.contains("dominant:"),
            "should contain dominant label, got: {}",
            s
        );
    }

    #[test]
    fn test_format_cost_inline_hash_dominant() {
        let cost = crate::cost::TableCost {
            processor: 1,
            hash: 6,
            u32_table: 0,
            op_stack: 1,
            ram: 0,
            jump_stack: 0,
        };
        let s = format_cost_inline(&cost);
        assert!(s.contains("cc=1"), "should contain cc=1, got: {}", s);
        assert!(s.contains("hash=6"), "should contain hash=6, got: {}", s);
        assert!(
            s.contains("dominant: hash"),
            "dominant should be hash, got: {}",
            s
        );
        // u32 is zero, so it should NOT appear
        assert!(
            !s.contains("u32="),
            "zero u32 should be omitted, got: {}",
            s
        );
    }

    // --- builtin hover cost ---

    #[test]
    fn test_builtin_hover_includes_cost() {
        let mut info = builtin_hover("hash").unwrap();
        let cost = crate::cost::cost_builtin("hash");
        info = format!("{}\n\n**Cost:** {}", info, format_cost_inline(&cost));
        assert!(
            info.contains("hash=6"),
            "hash hover should include hash=6 cost, got: {}",
            info
        );
        assert!(
            info.contains("**Cost:**"),
            "hover should include Cost header, got: {}",
            info
        );
    }

    #[test]
    fn test_builtin_hover_pub_read_cost() {
        let mut info = builtin_hover("pub_read").unwrap();
        let cost = crate::cost::cost_builtin("pub_read");
        info = format!("{}\n\n**Cost:** {}", info, format_cost_inline(&cost));
        assert!(
            info.contains("cc=1"),
            "pub_read hover should show cc=1, got: {}",
            info
        );
    }
}
