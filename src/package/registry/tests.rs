use super::client::*;
use super::json::*;
use super::types::*;
use crate::hash::ContentHash;

#[test]
fn test_json_escape() {
    assert_eq!(json_escape("hello"), "\"hello\"");
    assert_eq!(json_escape("a\"b"), "\"a\\\"b\"");
    assert_eq!(json_escape("a\\b"), "\"a\\\\b\"");
    assert_eq!(json_escape("line1\nline2"), "\"line1\\nline2\"");
    assert_eq!(json_escape("tab\there"), "\"tab\\there\"");
}

#[test]
fn test_extract_json_string() {
    let json = r#"{"hash":"abc123","name":"test"}"#;
    assert_eq!(extract_json_string(json, "hash"), "abc123");
    assert_eq!(extract_json_string(json, "name"), "test");
    assert_eq!(extract_json_string(json, "missing"), "");
}

#[test]
fn test_extract_json_bool() {
    let json = r#"{"verified":true,"created":false}"#;
    assert!(extract_json_bool(json, "verified"));
    assert!(!extract_json_bool(json, "created"));
    assert!(!extract_json_bool(json, "missing"));
}

#[test]
fn test_extract_json_array_strings() {
    let json = r#"{"tags":["crypto","hash","verified"]}"#;
    let tags = extract_json_array_strings(json, "tags");
    assert_eq!(tags, vec!["crypto", "hash", "verified"]);
}

#[test]
fn test_extract_json_array_strings_empty() {
    let json = r#"{"tags":[]}"#;
    let tags = extract_json_array_strings(json, "tags");
    assert!(tags.is_empty());
}

#[test]
fn test_url_encode() {
    assert_eq!(url_encode("hello world"), "hello%20world");
    assert_eq!(url_encode("a+b=c"), "a%2Bb%3Dc");
    assert_eq!(url_encode("Field"), "Field");
}

#[test]
fn test_parse_hex_hash_valid() {
    let hex = "a".repeat(64);
    assert!(ContentHash::from_hex(&hex).is_some());
}

#[test]
fn test_parse_hex_hash_invalid_length() {
    assert!(ContentHash::from_hex("abc").is_none());
    assert!(ContentHash::from_hex(&"a".repeat(63)).is_none());
    assert!(ContentHash::from_hex(&"a".repeat(65)).is_none());
}

#[test]
fn test_parse_hex_hash_invalid_chars() {
    let mut hex = "a".repeat(64);
    hex.replace_range(0..1, "g");
    assert!(ContentHash::from_hex(&hex).is_none());
}

#[test]
fn test_parse_url() {
    let (host, port, _) = parse_url("http://127.0.0.1:8090").unwrap();
    assert_eq!(host, "127.0.0.1");
    assert_eq!(port, 8090);

    let (host, port, _) = parse_url("http://localhost").unwrap();
    assert_eq!(host, "localhost");
    assert_eq!(port, 80);
}

#[test]
fn test_publish_json_roundtrip() {
    let pub_def = PublishedDefinition {
        hash: "c".repeat(64),
        source: "fn test() { }".to_string(),
        module: "test_mod".to_string(),
        is_pub: false,
        params: Vec::new(),
        return_ty: None,
        dependencies: Vec::new(),
        requires: Vec::new(),
        ensures: Vec::new(),
        name: Some("test_fn".to_string()),
        tags: vec!["testing".to_string()],
        verified: false,
        verification_cert: None,
    };

    let json = format_publish_json(&pub_def);
    let parsed = parse_publish_body(&json).unwrap();

    assert_eq!(parsed.hash, pub_def.hash);
    assert_eq!(parsed.source, pub_def.source);
    assert_eq!(parsed.module, pub_def.module);
    assert_eq!(parsed.is_pub, pub_def.is_pub);
    assert_eq!(parsed.name, pub_def.name);
    assert_eq!(parsed.tags, pub_def.tags);
}

#[test]
fn test_publish_json_roundtrip_complex() {
    let pub_def = PublishedDefinition {
        hash: "d".repeat(64),
        source: "fn add(a: Field, b: Field) -> Field {\n    a + b\n}".to_string(),
        module: "std.math".to_string(),
        is_pub: true,
        params: vec![
            ("a".to_string(), "Field".to_string()),
            ("b".to_string(), "Field".to_string()),
        ],
        return_ty: Some("Field".to_string()),
        dependencies: vec!["e".repeat(64)],
        requires: vec!["a > 0".to_string()],
        ensures: vec!["result == a + b".to_string()],
        name: Some("add".to_string()),
        tags: vec!["math".to_string(), "core".to_string()],
        verified: true,
        verification_cert: Some("cert123".to_string()),
    };

    let json = format_publish_json(&pub_def);
    let parsed = parse_publish_body(&json).unwrap();

    assert_eq!(parsed.hash, pub_def.hash);
    assert_eq!(parsed.source, pub_def.source);
    assert_eq!(parsed.module, pub_def.module);
    assert_eq!(parsed.is_pub, pub_def.is_pub);
    assert_eq!(parsed.params, pub_def.params);
    assert_eq!(parsed.return_ty, pub_def.return_ty);
    assert_eq!(parsed.name, pub_def.name);
    assert_eq!(parsed.verified, pub_def.verified);
}

#[test]
fn test_parse_publish_body_missing_hash() {
    let body = r#"{"source":"fn test() { }"}"#;
    assert!(parse_publish_body(body).is_err());
}

#[test]
fn test_parse_publish_body_missing_source() {
    let hash = "a".repeat(64);
    let body = format!("{{\"hash\":\"{}\"}}", hash);
    assert!(parse_publish_body(&body).is_err());
}

#[test]
fn test_parse_publish_body_invalid_hash() {
    let body = r#"{"hash":"tooshort","source":"fn test() { }"}"#;
    assert!(parse_publish_body(body).is_err());
}
