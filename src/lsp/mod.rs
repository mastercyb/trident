//! Trident Language Server Protocol implementation.
//!
//! Provides IDE features: diagnostics, formatting, document symbols,
//! go-to-definition, hover, completion, signature help, semantic tokens
//! (with incremental deltas), folding ranges, selection ranges,
//! find references, rename, document highlight, workspace symbol,
//! inlay hints, and code actions.

mod actions;
mod builtins;
mod document;
mod folding;
mod hints;
mod incremental;
mod indent;
mod intelligence;
mod project;
mod references;
mod selection;
mod semantic;
mod textobjects;
pub mod util;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use document::{compute_line_starts, DocumentState};
use util::{position_to_byte_offset, to_lsp_diagnostic, word_at_position};

pub struct TridentLsp {
    client: Client,
    documents: Mutex<BTreeMap<Url, DocumentState>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for TridentLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
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
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: semantic::token_legend(),
                            full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                            range: None,
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\n".to_string(),
                    more_trigger_character: Some(vec!["}".to_string()]),
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
        let source = params.text_document.text;
        let mut doc = DocumentState::new(source);

        // Initial name_kinds from parse
        if let Ok(file) = crate::parse_source_silent(&doc.source, "") {
            doc.name_kinds = semantic::build_name_kinds(&file);
        }

        let diag_source = doc.source.clone();
        self.documents.lock().unwrap().insert(uri.clone(), doc);
        self.publish_diagnostics(uri, &diag_source).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        let diag_source = {
            let mut docs = self.documents.lock().unwrap();
            let doc = match docs.get_mut(&uri) {
                Some(d) => d,
                None => return,
            };

            for change in params.content_changes {
                if let Some(range) = change.range {
                    let edit_start = position_to_byte_offset(&doc.source, range.start).unwrap_or(0);
                    let edit_old_end =
                        position_to_byte_offset(&doc.source, range.end).unwrap_or(doc.source.len());

                    let mut new_source = String::with_capacity(
                        doc.source.len() - (edit_old_end - edit_start) + change.text.len(),
                    );
                    new_source.push_str(&doc.source[..edit_start]);
                    new_source.push_str(&change.text);
                    new_source.push_str(&doc.source[edit_old_end..]);

                    let edit_new_end = edit_start + change.text.len();

                    let result = incremental::incremental_lex(
                        &new_source,
                        &doc.tokens,
                        &doc.comments,
                        edit_start,
                        edit_old_end,
                        edit_new_end,
                    );

                    doc.source = new_source;
                    doc.tokens = result.tokens;
                    doc.comments = result.comments;
                    doc.line_starts = compute_line_starts(&doc.source);
                } else {
                    // Full replacement (fallback)
                    doc.source = change.text;
                    let (tokens, comments, _) =
                        crate::syntax::lexer::Lexer::new(&doc.source, 0).tokenize();
                    doc.tokens = tokens;
                    doc.comments = comments;
                    doc.line_starts = compute_line_starts(&doc.source);
                }
            }

            // Re-parse for name_kinds (cheap for contract-sized files)
            if let Ok(file) = crate::parse_source_silent(&doc.source, "") {
                doc.name_kinds = semantic::build_name_kinds(&file);
            }

            doc.source.clone()
        }; // lock dropped here

        self.publish_diagnostics(uri, &diag_source).await;
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
            Some(doc) => doc.source.clone(),
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

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document_position.text_document.uri;
        let docs = self.documents.lock().unwrap();
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        Ok(indent::on_type_formatting(
            &doc.source,
            &doc.tokens,
            params.text_document_position.position,
            &params.ch,
        ))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Ok(None),
        };
        let file = match crate::parse_source_silent(&source, uri.path()) {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };
        let symbols = self.document_symbols(&source, &file);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Ok(None),
        };

        let word = word_at_position(&source, pos);
        if word.is_empty() {
            return Ok(None);
        }

        let file_path = PathBuf::from(uri.path());
        let index = self.build_symbol_index(&file_path);

        if let Some((target_uri, range)) = index.get(&word) {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri.clone(),
                range: *range,
            })));
        }

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
        self.do_hover(uri, pos).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        self.do_completion(uri, pos).await
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.do_signature_help(uri, pos).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let mut docs = self.documents.lock().unwrap();
        let doc = match docs.get_mut(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let tokens = semantic::semantic_tokens_from_cache(doc);
        doc.last_semantic_tokens = tokens.clone();
        doc.result_version += 1;

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some(doc.result_id()),
            data: tokens,
        })))
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = &params.text_document.uri;
        let mut docs = self.documents.lock().unwrap();
        let doc = match docs.get_mut(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        // If client's previous_result_id doesn't match, send full tokens
        if params.previous_result_id != doc.result_id() {
            let tokens = semantic::semantic_tokens_from_cache(doc);
            doc.last_semantic_tokens = tokens.clone();
            doc.result_version += 1;
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: Some(doc.result_id()),
                    data: tokens,
                },
            )));
        }

        let new_tokens = semantic::semantic_tokens_from_cache(doc);
        let edits = semantic::compute_semantic_delta(&doc.last_semantic_tokens, &new_tokens);
        doc.last_semantic_tokens = new_tokens;
        doc.result_version += 1;

        Ok(Some(SemanticTokensFullDeltaResult::TokensDelta(
            SemanticTokensDelta {
                result_id: Some(doc.result_id()),
                edits,
            },
        )))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        let (source, comments) = {
            let docs = self.documents.lock().unwrap();
            match docs.get(uri) {
                Some(d) => (d.source.clone(), d.comments.clone()),
                None => return Ok(None),
            }
        };

        let (tokens, _, _) = crate::syntax::lexer::Lexer::new(&source, 0).tokenize();
        let file = match crate::syntax::parser::Parser::new(tokens).parse_file() {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };
        Ok(Some(folding::folding_ranges(&source, &file, &comments)))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = &params.text_document.uri;
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Ok(None),
        };
        let file = match crate::parse_source_silent(&source, uri.path()) {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };
        Ok(Some(selection::selection_ranges(
            &source,
            &file,
            &params.positions,
        )))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Ok(None),
        };
        let diags: Vec<_> = params.context.diagnostics;
        let result = actions::code_actions(&source, &diags, uri);
        Ok(if result.is_empty() {
            None
        } else {
            Some(result)
        })
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let refs = self.do_references(uri, pos);
        Ok(if refs.is_empty() { None } else { Some(refs) })
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        Ok(self.do_prepare_rename(&params.text_document.uri, params.position))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        Ok(self.do_rename(uri, pos, &params.new_name))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let highlights = self.do_document_highlight(uri, pos);
        Ok(if highlights.is_empty() {
            None
        } else {
            Some(highlights)
        })
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let docs = self.documents.lock().unwrap();
        let symbols = self.workspace_symbols(&params.query, &docs);
        Ok(if symbols.is_empty() {
            None
        } else {
            Some(symbols)
        })
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Ok(None),
        };
        let result = hints::inlay_hints(&source, params.range);
        Ok(if result.is_empty() {
            None
        } else {
            Some(result)
        })
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
}

/// Start the LSP server on stdin/stdout.
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| TridentLsp {
        client,
        documents: Mutex::new(BTreeMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
