use crate::ast::{Block, File, Item, Stmt};
use crate::syntax::lexer::Comment;
use crate::syntax::span::Span;

/// Kind of structural text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjectKind {
    Function,
    Struct,
    Event,
    Loop,
    Conditional,
    MatchArm,
    Parameter,
    Block,
    Comment,
}

/// A structural text object with around (full construct) and inside
/// (body contents) spans.
#[derive(Debug, Clone)]
pub struct TextObject {
    pub kind: TextObjectKind,
    pub around: Span,
    pub inside: Span,
}

/// Find all text objects enclosing the given byte offset.
/// Returns them from outermost to innermost.
pub fn text_objects_at(file: &File, comments: &[Comment], offset: u32) -> Vec<TextObject> {
    let mut objects = Vec::new();

    // Comment text objects
    for c in comments {
        if offset >= c.span.start && offset < c.span.end {
            objects.push(TextObject {
                kind: TextObjectKind::Comment,
                around: c.span,
                inside: c.span,
            });
        }
    }

    // Item-level text objects
    for item in &file.items {
        if !contains(item.span, offset) {
            continue;
        }

        match &item.node {
            Item::Fn(f) => {
                let inside = f
                    .body
                    .as_ref()
                    .map(|b| shrink_braces(b.span))
                    .unwrap_or(item.span);
                objects.push(TextObject {
                    kind: TextObjectKind::Function,
                    around: item.span,
                    inside,
                });

                // Parameter text objects
                for p in &f.params {
                    let param_span = Span::new(0, p.name.span.start, p.ty.span.end);
                    if contains(param_span, offset) {
                        objects.push(TextObject {
                            kind: TextObjectKind::Parameter,
                            around: param_span,
                            inside: param_span,
                        });
                    }
                }

                // Recurse into body
                if let Some(body) = &f.body {
                    if contains(body.span, offset) {
                        objects.push(TextObject {
                            kind: TextObjectKind::Block,
                            around: body.span,
                            inside: shrink_braces(body.span),
                        });
                        collect_block_objects(&body.node, offset, &mut objects);
                    }
                }
            }
            Item::Struct(_) => {
                objects.push(TextObject {
                    kind: TextObjectKind::Struct,
                    around: item.span,
                    inside: item.span,
                });
            }
            Item::Event(_) => {
                objects.push(TextObject {
                    kind: TextObjectKind::Event,
                    around: item.span,
                    inside: item.span,
                });
            }
            Item::Const(_) => {}
        }
        break;
    }

    objects
}

fn collect_block_objects(block: &Block, offset: u32, objects: &mut Vec<TextObject>) {
    for stmt in &block.stmts {
        if !contains(stmt.span, offset) {
            continue;
        }

        match &stmt.node {
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                objects.push(TextObject {
                    kind: TextObjectKind::Conditional,
                    around: stmt.span,
                    inside: shrink_braces(then_block.span),
                });
                if contains(then_block.span, offset) {
                    objects.push(TextObject {
                        kind: TextObjectKind::Block,
                        around: then_block.span,
                        inside: shrink_braces(then_block.span),
                    });
                    collect_block_objects(&then_block.node, offset, objects);
                }
                if let Some(eb) = else_block {
                    if contains(eb.span, offset) {
                        objects.push(TextObject {
                            kind: TextObjectKind::Block,
                            around: eb.span,
                            inside: shrink_braces(eb.span),
                        });
                        collect_block_objects(&eb.node, offset, objects);
                    }
                }
            }
            Stmt::For { body, .. } => {
                objects.push(TextObject {
                    kind: TextObjectKind::Loop,
                    around: stmt.span,
                    inside: shrink_braces(body.span),
                });
                if contains(body.span, offset) {
                    objects.push(TextObject {
                        kind: TextObjectKind::Block,
                        around: body.span,
                        inside: shrink_braces(body.span),
                    });
                    collect_block_objects(&body.node, offset, objects);
                }
            }
            Stmt::Match { arms, .. } => {
                objects.push(TextObject {
                    kind: TextObjectKind::Conditional,
                    around: stmt.span,
                    inside: stmt.span,
                });
                for arm in arms {
                    let arm_span = Span::new(0, arm.pattern.span.start, arm.body.span.end);
                    if contains(arm_span, offset) {
                        objects.push(TextObject {
                            kind: TextObjectKind::MatchArm,
                            around: arm_span,
                            inside: shrink_braces(arm.body.span),
                        });
                        if contains(arm.body.span, offset) {
                            collect_block_objects(&arm.body.node, offset, objects);
                        }
                    }
                }
            }
            _ => {}
        }
        break;
    }
}

/// Shrink a brace-delimited span by 1 byte on each side to get the
/// inner contents (excluding `{` and `}`).
fn shrink_braces(span: Span) -> Span {
    if span.end - span.start >= 2 {
        Span::new(0, span.start + 1, span.end - 1)
    } else {
        span
    }
}

fn contains(span: Span, offset: u32) -> bool {
    offset >= span.start && offset < span.end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> File {
        crate::parse_source_silent(source, "").expect("parse failed")
    }

    #[test]
    fn function_text_objects() {
        let src = "program test\nfn main() {\n    let x: Field = 42\n}\n";
        let file = parse(src);
        // Offset inside the function body
        let objects = text_objects_at(&file, &[], 30);
        let fn_obj = objects.iter().find(|o| o.kind == TextObjectKind::Function);
        assert!(fn_obj.is_some(), "should find function text object");
        let blk_obj = objects.iter().find(|o| o.kind == TextObjectKind::Block);
        assert!(blk_obj.is_some(), "should find block text object");
    }

    #[test]
    fn struct_text_object() {
        let src = "module test\npub struct Point {\n    x: Field,\n    y: Field,\n}\n";
        let file = parse(src);
        let objects = text_objects_at(&file, &[], 20);
        let s = objects.iter().find(|o| o.kind == TextObjectKind::Struct);
        assert!(s.is_some(), "should find struct text object");
    }

    #[test]
    fn loop_text_object() {
        let src = "program test\nfn main() {\n    for i in 0..10 bounded 10 {\n        let x: Field = 1\n    }\n}\n";
        let file = parse(src);
        // Offset inside the for loop body
        let objects = text_objects_at(&file, &[], 65);
        let loop_obj = objects.iter().find(|o| o.kind == TextObjectKind::Loop);
        assert!(loop_obj.is_some(), "should find loop text object");
    }

    #[test]
    fn conditional_text_object() {
        let src = "program test\nfn main() {\n    if true {\n        let x: Field = 1\n    }\n}\n";
        let file = parse(src);
        let objects = text_objects_at(&file, &[], 50);
        let cond = objects
            .iter()
            .find(|o| o.kind == TextObjectKind::Conditional);
        assert!(cond.is_some(), "should find conditional text object");
    }

    #[test]
    fn inside_span_excludes_braces() {
        let src = "program test\nfn main() {\n    let x: Field = 42\n}\n";
        let file = parse(src);
        let objects = text_objects_at(&file, &[], 30);
        let fn_obj = objects
            .iter()
            .find(|o| o.kind == TextObjectKind::Function)
            .unwrap();
        // inside span should be strictly smaller than around span
        assert!(fn_obj.inside.start >= fn_obj.around.start);
        assert!(fn_obj.inside.end <= fn_obj.around.end);
        assert!(fn_obj.inside.end - fn_obj.inside.start < fn_obj.around.end - fn_obj.around.start);
    }
}
