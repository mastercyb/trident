//! Inlay hints: inline cost estimates for functions and loops.

use tower_lsp::lsp_types::*;

use crate::ast::{Item, Stmt};
use crate::cost::CostAnalyzer;

use super::util::{byte_offset_to_position, format_cost_inline};

/// Compute inlay hints for a source file within the given range.
pub(super) fn inlay_hints(source: &str, range: Range) -> Vec<InlayHint> {
    let file = match crate::parse_source_silent(source, "") {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut analyzer = CostAnalyzer::default();
    let program_cost = analyzer.analyze_file(&file);

    let mut hints = Vec::new();

    for item in &file.items {
        if let Item::Fn(f) = &item.node {
            // Hint at end of function signature line (before opening brace)
            if let Some(body) = &f.body {
                let sig_end = body.span.start as usize;
                let hint_pos = byte_offset_to_position(source, sig_end);

                if !in_range(hint_pos, range) {
                    continue;
                }

                // Look up cost from program_cost
                if let Some(fc) = program_cost
                    .functions
                    .iter()
                    .find(|c| c.name == f.name.node)
                {
                    let cost_text = format_cost_inline(&fc.cost);
                    hints.push(InlayHint {
                        position: hint_pos,
                        label: InlayHintLabel::String(format!(" {} ", cost_text)),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: Some(InlayHintTooltip::String(format!(
                            "Estimated cost for fn {}",
                            f.name.node,
                        ))),
                        padding_left: Some(true),
                        padding_right: None,
                        data: None,
                    });

                    // Loop hints within this function body
                    if let Some(ref per_iter) = fc.per_iteration {
                        collect_loop_hints(
                            source,
                            &body.node.stmts,
                            &per_iter.0,
                            per_iter.1,
                            range,
                            &mut hints,
                        );
                    }
                }
            }
        }
    }

    hints
}

fn collect_loop_hints(
    source: &str,
    stmts: &[crate::syntax::span::Spanned<Stmt>],
    per_iter_cost: &crate::cost::TableCost,
    bound: u64,
    range: Range,
    hints: &mut Vec<InlayHint>,
) {
    for stmt in stmts {
        match &stmt.node {
            Stmt::For { body, .. } => {
                // Hint at the for statement's opening position
                let hint_pos = byte_offset_to_position(source, body.span.start as usize);
                if in_range(hint_pos, range) {
                    let cost_text = format_cost_inline(per_iter_cost);
                    hints.push(InlayHint {
                        position: hint_pos,
                        label: InlayHintLabel::String(format!(
                            " {} x {} iterations ",
                            cost_text, bound
                        )),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: Some(InlayHintTooltip::String(
                            "Per-iteration cost x bound".to_string(),
                        )),
                        padding_left: Some(true),
                        padding_right: None,
                        data: None,
                    });
                }
                // Recurse into loop body
                collect_loop_hints(source, &body.node.stmts, per_iter_cost, bound, range, hints);
            }
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                collect_loop_hints(
                    source,
                    &then_block.node.stmts,
                    per_iter_cost,
                    bound,
                    range,
                    hints,
                );
                if let Some(eb) = else_block {
                    collect_loop_hints(source, &eb.node.stmts, per_iter_cost, bound, range, hints);
                }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    collect_loop_hints(
                        source,
                        &arm.body.node.stmts,
                        per_iter_cost,
                        bound,
                        range,
                        hints,
                    );
                }
            }
            _ => {}
        }
    }
}

fn in_range(pos: Position, range: Range) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character <= range.end.character))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_range() -> Range {
        Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX))
    }

    #[test]
    fn function_cost_hint_appears() {
        let source = "program test\nfn main() {\n  let x: Field = 1\n  let y: Field = x + x\n}\n";
        let hints = inlay_hints(source, full_range());
        assert!(!hints.is_empty());
        // Should have at least one hint for fn main
        let main_hint = hints.iter().find(|h| match &h.tooltip {
            Some(InlayHintTooltip::String(s)) => s.contains("main"),
            _ => false,
        });
        assert!(main_hint.is_some());
    }

    #[test]
    fn loop_cost_hint_appears() {
        let source = "program test\nfn run() {\n  for i in 0..10 bounded 10 {\n    let x: Field = 1\n  }\n}\n";
        let hints = inlay_hints(source, full_range());
        let loop_hint = hints.iter().find(|h| match &h.label {
            InlayHintLabel::String(s) => s.contains("iterations"),
            _ => false,
        });
        assert!(loop_hint.is_some());
    }
}
