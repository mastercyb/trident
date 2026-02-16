use tower_lsp::lsp_types::*;

use crate::ast::{Block, File, Item, Stmt};
use crate::syntax::span::Span;

use super::util::{byte_offset_to_position, position_to_byte_offset, span_to_range};

pub fn selection_ranges(source: &str, file: &File, positions: &[Position]) -> Vec<SelectionRange> {
    positions
        .iter()
        .map(|pos| build_selection_at(source, file, *pos))
        .collect()
}

fn build_selection_at(source: &str, file: &File, pos: Position) -> SelectionRange {
    let offset = match position_to_byte_offset(source, pos) {
        Some(o) => o as u32,
        None => {
            return SelectionRange {
                range: Range::new(pos, pos),
                parent: None,
            };
        }
    };

    // Collect enclosing spans from outermost to innermost
    let mut scopes: Vec<Span> = Vec::new();

    // File-level scope
    if let Some(last_item) = file.items.last() {
        let file_span = Span::new(0, file.name.span.start, last_item.span.end);
        scopes.push(file_span);
    }

    // Find enclosing item
    for item in &file.items {
        if !contains(item.span, offset) {
            continue;
        }
        scopes.push(item.span);

        match &item.node {
            Item::Fn(f) => {
                // Function name
                if contains(f.name.span, offset) {
                    scopes.push(f.name.span);
                }
                // Parameters
                for p in &f.params {
                    if contains(p.name.span, offset) {
                        scopes.push(p.name.span);
                    }
                }
                // Body
                if let Some(body) = &f.body {
                    if contains(body.span, offset) {
                        scopes.push(body.span);
                        collect_block_scopes(&body.node, offset, &mut scopes);
                    }
                }
            }
            Item::Struct(s) => {
                if contains(s.name.span, offset) {
                    scopes.push(s.name.span);
                }
                for field in &s.fields {
                    if contains(field.name.span, offset) {
                        scopes.push(field.name.span);
                    }
                }
            }
            Item::Event(e) => {
                if contains(e.name.span, offset) {
                    scopes.push(e.name.span);
                }
            }
            Item::Const(c) => {
                if contains(c.name.span, offset) {
                    scopes.push(c.name.span);
                }
                if contains(c.value.span, offset) {
                    scopes.push(c.value.span);
                }
            }
        }
        break;
    }

    // Build linked list from outermost (parent) to innermost (leaf)
    build_chain(source, &scopes)
}

fn collect_block_scopes(block: &Block, offset: u32, scopes: &mut Vec<Span>) {
    for stmt in &block.stmts {
        if !contains(stmt.span, offset) {
            continue;
        }
        scopes.push(stmt.span);

        match &stmt.node {
            Stmt::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                if contains(cond.span, offset) {
                    scopes.push(cond.span);
                }
                if contains(then_block.span, offset) {
                    scopes.push(then_block.span);
                    collect_block_scopes(&then_block.node, offset, scopes);
                }
                if let Some(eb) = else_block {
                    if contains(eb.span, offset) {
                        scopes.push(eb.span);
                        collect_block_scopes(&eb.node, offset, scopes);
                    }
                }
            }
            Stmt::For { body, .. } => {
                if contains(body.span, offset) {
                    scopes.push(body.span);
                    collect_block_scopes(&body.node, offset, scopes);
                }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    if contains(arm.body.span, offset) {
                        scopes.push(arm.body.span);
                        collect_block_scopes(&arm.body.node, offset, scopes);
                    }
                }
            }
            _ => {}
        }
        break;
    }

    // Tail expression
    if let Some(tail) = &block.tail_expr {
        if contains(tail.span, offset) {
            scopes.push(tail.span);
        }
    }
}

fn contains(span: Span, offset: u32) -> bool {
    offset >= span.start && offset < span.end
}

fn build_chain(source: &str, scopes: &[Span]) -> SelectionRange {
    let mut result: Option<SelectionRange> = None;

    // Build from outermost to innermost
    for span in scopes {
        let range = span_to_range(source, *span);
        result = Some(SelectionRange {
            range,
            parent: result.map(Box::new),
        });
    }

    result.unwrap_or(SelectionRange {
        range: Range::new(
            byte_offset_to_position(source, 0),
            byte_offset_to_position(source, source.len()),
        ),
        parent: None,
    })
}
