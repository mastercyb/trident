use super::*;
use std::path::PathBuf;

#[test]
fn legend_has_all_types() {
    let legend = token_legend();
    assert_eq!(legend.token_types.len(), 13);
    assert_eq!(legend.token_modifiers.len(), 4);
}

#[test]
fn simple_program_tokens() {
    let source = "program test\nfn main() {\n  let x: Field = 42\n}\n";
    let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
    assert!(!tokens.is_empty());
    assert_eq!(tokens[0].token_type, TT_KEYWORD);
    assert_eq!(tokens[0].delta_line, 0);
    assert_eq!(tokens[0].delta_start, 0);
    assert_eq!(tokens[0].length, 7);
}

#[test]
fn builtin_classified_as_function() {
    let source = "program test\nfn main() {\n  let x: Field = pub_read()\n}\n";
    let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
    let pub_read = tokens.iter().find(|t| t.length == 8);
    assert!(pub_read.is_some());
    let pr = pub_read.unwrap();
    assert_eq!(pr.token_type, TT_FUNCTION);
    assert_ne!(pr.token_modifiers_bitset & MOD_DEFAULT_LIBRARY, 0);
}

#[test]
fn struct_name_classified_as_type() {
    let source = "module std.test\npub struct Point {\n  x: Field,\n  y: Field,\n}\n";
    let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
    let point = tokens
        .iter()
        .find(|t| t.length == 5 && t.token_type == TT_TYPE);
    assert!(point.is_some());
}

#[test]
fn comments_included() {
    let source = "program test\n// this is a comment\nfn main() {}\n";
    let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
    let comment = tokens.iter().find(|t| t.token_type == TT_COMMENT);
    assert!(comment.is_some());
}

#[test]
fn delta_identical_is_empty() {
    let tokens = vec![SemanticToken {
        delta_line: 0,
        delta_start: 0,
        length: 7,
        token_type: TT_KEYWORD,
        token_modifiers_bitset: 0,
    }];
    let edits = compute_semantic_delta(&tokens, &tokens);
    assert!(edits.is_empty());
}

#[test]
fn delta_single_change() {
    let old = vec![
        SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 2,
            token_type: TT_KEYWORD,
            token_modifiers_bitset: 0,
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 3,
            length: 4,
            token_type: TT_FUNCTION,
            token_modifiers_bitset: 0,
        },
    ];
    let new = vec![
        SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 2,
            token_type: TT_KEYWORD,
            token_modifiers_bitset: 0,
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 3,
            length: 5,
            token_type: TT_FUNCTION,
            token_modifiers_bitset: 0,
        },
    ];
    let edits = compute_semantic_delta(&old, &new);
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].start, 1);
    assert_eq!(edits[0].delete_count, 1);
    assert_eq!(edits[0].data.as_ref().map(|d| d.len()), Some(1));
}

#[test]
fn delta_token_insertion() {
    let old = vec![SemanticToken {
        delta_line: 0,
        delta_start: 0,
        length: 2,
        token_type: TT_KEYWORD,
        token_modifiers_bitset: 0,
    }];
    let new = vec![
        SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 3,
            token_type: TT_KEYWORD,
            token_modifiers_bitset: 0,
        },
        SemanticToken {
            delta_line: 0,
            delta_start: 4,
            length: 2,
            token_type: TT_KEYWORD,
            token_modifiers_bitset: 0,
        },
    ];
    let edits = compute_semantic_delta(&old, &new);
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].delete_count, 1);
    assert_eq!(edits[0].data.as_ref().map(|d| d.len()), Some(2));
}

#[test]
fn asm_block_produces_multiple_tokens() {
    let source = "program test\nfn main() {\n    asm { push 1\nadd }\n}\n";
    let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
    // Should have more than one token for the asm region:
    // `asm` keyword + `push` instruction + `1` number + `add` instruction
    let asm_region_tokens: Vec<_> = tokens
        .iter()
        .filter(|t| t.token_type == TT_KEYWORD && t.token_modifiers_bitset & (1 << 3) != 0)
        .collect();
    assert!(
        asm_region_tokens.len() >= 2,
        "asm block should produce multiple instruction tokens, got {}",
        asm_region_tokens.len()
    );
}

#[test]
fn asm_target_highlighted_as_namespace() {
    let source = "program test\nfn main() {\n    asm(triton) { push 42 }\n}\n";
    let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
    let ns = tokens.iter().find(|t| t.token_type == 9); // TT_NAMESPACE
    assert!(
        ns.is_some(),
        "target tag should be highlighted as namespace"
    );
}

#[test]
fn asm_numbers_highlighted() {
    let source = "program test\nfn main() {\n    asm { push 42 }\n}\n";
    let tokens = semantic_tokens(source, &PathBuf::from("test.tri"));
    // Find a number token that's the `42` inside the asm block
    let nums: Vec<_> = tokens
        .iter()
        .filter(|t| t.token_type == TT_NUMBER)
        .collect();
    assert!(
        !nums.is_empty(),
        "asm block should contain number tokens for operands"
    );
}
