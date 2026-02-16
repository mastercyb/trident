//! LSP intelligence: hover, completion, and signature help.

use std::path::PathBuf;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::types::Ty;

use super::builtins::{builtin_completions, builtin_hover, builtin_signature};
use super::util::{find_call_context, format_cost_inline, text_before_dot, word_at_position};
use super::TridentLsp;

impl TridentLsp {
    pub(super) async fn do_hover(&self, uri: &Url, pos: Position) -> Result<Option<Hover>> {
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Ok(None),
        };

        let word = word_at_position(&source, pos);
        if word.is_empty() {
            return Ok(None);
        }

        // Check builtins first
        if let Some(mut info) = builtin_hover(&word) {
            let cost = crate::cost::cost_builtin("triton", &word);
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

    pub(super) async fn do_completion(
        &self,
        uri: &Url,
        pos: Position,
    ) -> Result<Option<CompletionResponse>> {
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
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

                for (cname, ty, _val) in &exp.constants {
                    let bare = cname.rsplit('.').next().unwrap_or(cname);
                    items.push(CompletionItem {
                        label: bare.to_string(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(ty.display()),
                        ..Default::default()
                    });
                }

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
            "return", "use", "pub", "reveal", "seal", "true", "false",
        ];
        for kw in &keywords {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }

        let type_kws = ["Field", "XField", "Bool", "U32", "Digest"];
        for ty in &type_kws {
            items.push(CompletionItem {
                label: ty.to_string(),
                kind: Some(CompletionItemKind::TYPE_PARAMETER),
                ..Default::default()
            });
        }

        for (name, detail) in builtin_completions() {
            items.push(CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail),
                ..Default::default()
            });
        }

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

    pub(super) async fn do_signature_help(
        &self,
        uri: &Url,
        pos: Position,
    ) -> Result<Option<SignatureHelp>> {
        let source = match self.documents.lock().unwrap().get(uri) {
            Some(doc) => doc.source.clone(),
            None => return Ok(None),
        };

        let (fn_name, active_param) = match find_call_context(&source, pos) {
            Some(ctx) => ctx,
            None => return Ok(None),
        };

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
}
