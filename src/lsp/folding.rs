use tower_lsp::lsp_types::*;

use crate::ast::{Block, File, Item, Stmt};
use crate::syntax::lexer::Comment;

use super::util::byte_offset_to_position;

pub fn folding_ranges(source: &str, file: &File, comments: &[Comment]) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();

    // Use declarations block
    if file.uses.len() > 1 {
        let first = &file.uses[0];
        let last = &file.uses[file.uses.len() - 1];
        let start = byte_offset_to_position(source, first.span.start as usize);
        let end = byte_offset_to_position(source, last.span.end as usize);
        if start.line < end.line {
            ranges.push(FoldingRange {
                start_line: start.line,
                start_character: None,
                end_line: end.line,
                end_character: None,
                kind: Some(FoldingRangeKind::Imports),
                collapsed_text: None,
            });
        }
    }

    // Items
    for item in &file.items {
        let start = byte_offset_to_position(source, item.span.start as usize);
        let end = byte_offset_to_position(source, item.span.end as usize);
        if start.line < end.line {
            ranges.push(FoldingRange {
                start_line: start.line,
                start_character: None,
                end_line: end.line,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: None,
            });
        }

        // Nested blocks inside function bodies
        if let Item::Fn(f) = &item.node {
            if let Some(body) = &f.body {
                collect_block_folds(source, &body.node, &mut ranges);
            }
        }
    }

    // Consecutive comment blocks
    collect_comment_folds(source, comments, &mut ranges);

    ranges
}

fn collect_block_folds(source: &str, block: &Block, ranges: &mut Vec<FoldingRange>) {
    for stmt in &block.stmts {
        match &stmt.node {
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                fold_block(source, then_block.span, ranges);
                collect_block_folds(source, &then_block.node, ranges);
                if let Some(eb) = else_block {
                    fold_block(source, eb.span, ranges);
                    collect_block_folds(source, &eb.node, ranges);
                }
            }
            Stmt::For { body, .. } => {
                fold_block(source, body.span, ranges);
                collect_block_folds(source, &body.node, ranges);
            }
            Stmt::Match { arms, .. } => {
                let stmt_start = byte_offset_to_position(source, stmt.span.start as usize);
                let stmt_end = byte_offset_to_position(source, stmt.span.end as usize);
                if stmt_start.line < stmt_end.line {
                    ranges.push(FoldingRange {
                        start_line: stmt_start.line,
                        start_character: None,
                        end_line: stmt_end.line,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: None,
                    });
                }
                for arm in arms {
                    fold_block(source, arm.body.span, ranges);
                    collect_block_folds(source, &arm.body.node, ranges);
                }
            }
            _ => {}
        }
    }
}

fn fold_block(source: &str, span: crate::syntax::span::Span, ranges: &mut Vec<FoldingRange>) {
    let start = byte_offset_to_position(source, span.start as usize);
    let end = byte_offset_to_position(source, span.end as usize);
    if start.line < end.line {
        ranges.push(FoldingRange {
            start_line: start.line,
            start_character: None,
            end_line: end.line,
            end_character: None,
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: None,
        });
    }
}

fn collect_comment_folds(source: &str, comments: &[Comment], ranges: &mut Vec<FoldingRange>) {
    if comments.is_empty() {
        return;
    }

    let mut group_start: Option<u32> = None;
    let mut prev_line: Option<u32> = None;

    for comment in comments {
        let pos = byte_offset_to_position(source, comment.span.start as usize);
        let line = pos.line;

        match prev_line {
            Some(pl) if line == pl + 1 => {
                // Continue group
            }
            _ => {
                // Flush previous group
                if let (Some(gs), Some(pl)) = (group_start, prev_line) {
                    if gs < pl {
                        ranges.push(FoldingRange {
                            start_line: gs,
                            start_character: None,
                            end_line: pl,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Comment),
                            collapsed_text: None,
                        });
                    }
                }
                group_start = Some(line);
            }
        }
        prev_line = Some(line);
    }

    // Flush last group
    if let (Some(gs), Some(pl)) = (group_start, prev_line) {
        if gs < pl {
            ranges.push(FoldingRange {
                start_line: gs,
                start_character: None,
                end_line: pl,
                end_character: None,
                kind: Some(FoldingRangeKind::Comment),
                collapsed_text: None,
            });
        }
    }
}
