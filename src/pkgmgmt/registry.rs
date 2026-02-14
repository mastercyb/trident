//! Registry Client — HTTP client for interacting with a Trident registry.
//!
//! Provides a client for publishing and pulling content-addressed definitions
//! to/from a remote registry over HTTP. Wire format is JSON.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use crate::hash::ContentHash;
use crate::ucm::{Codebase, Definition};

// ─── Published Definition (wire format) ───────────────────────────

/// A definition as published to the registry (JSON wire format).
#[derive(Clone, Debug)]
pub struct PublishedDefinition {
    /// Content hash (hex).
    pub hash: String,
    /// Function source code.
    pub source: String,
    /// Module name.
    pub module: String,
    /// Is it public?
    pub is_pub: bool,
    /// Parameters: [(name, type)].
    pub params: Vec<(String, String)>,
    /// Return type (if any).
    pub return_ty: Option<String>,
    /// Dependencies: hex hashes of called functions.
    pub dependencies: Vec<String>,
    /// Preconditions.
    pub requires: Vec<String>,
    /// Postconditions.
    pub ensures: Vec<String>,
    /// Name binding (if any).
    pub name: Option<String>,
    /// Tags for search.
    pub tags: Vec<String>,
    /// Verification status.
    pub verified: bool,
    /// Verification certificate (opaque string, if available).
    pub verification_cert: Option<String>,
}

/// Search result entry.
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub name: String,
    pub hash: String,
    pub module: String,
    pub signature: String,
    pub verified: bool,
    pub tags: Vec<String>,
}

/// Result of a publish operation.
#[derive(Clone, Debug)]
pub struct PublishResult {
    pub hash: String,
    pub created: bool,
    pub name_bound: bool,
}

/// Result of a pull operation.
#[derive(Clone, Debug)]
pub struct PullResult {
    pub hash: String,
    pub source: String,
    pub module: String,
    pub params: Vec<(String, String)>,
    pub return_ty: Option<String>,
    pub dependencies: Vec<String>,
    pub requires: Vec<String>,
    pub ensures: Vec<String>,
}

// ─── Registry Client ──────────────────────────────────────────────

/// Client for interacting with a remote Trident registry.
pub struct RegistryClient {
    base_url: String,
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new(url: &str) -> Self {
        Self {
            base_url: url.trim_end_matches('/').to_string(),
        }
    }

    /// Get the default registry URL from environment or config.
    pub fn default_url() -> String {
        std::env::var("TRIDENT_REGISTRY_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string())
    }

    /// Publish a definition to the registry.
    pub fn publish(&self, def: &PublishedDefinition) -> Result<PublishResult, String> {
        let body = format_publish_json(def);
        let response = self.http_post("/api/v1/definitions", &body)?;

        if response.status >= 400 {
            return Err(format!(
                "publish failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(PublishResult {
            hash: extract_json_string(&response.body, "hash"),
            created: extract_json_bool(&response.body, "created"),
            name_bound: extract_json_bool(&response.body, "name_bound"),
        })
    }

    /// Pull a definition from the registry by hash.
    pub fn pull(&self, hash: &str) -> Result<PullResult, String> {
        let path = format!("/api/v1/definitions/{}", hash);
        let response = self.http_get(&path)?;

        if response.status == 404 {
            return Err(format!("definition {} not found in registry", hash));
        }
        if response.status >= 400 {
            return Err(format!(
                "pull failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_pull_response(&response.body))
    }

    /// Pull a definition by name.
    pub fn pull_by_name(&self, name: &str) -> Result<PullResult, String> {
        let path = format!("/api/v1/names/{}", name);
        let response = self.http_get(&path)?;

        if response.status == 404 {
            return Err(format!("name '{}' not found in registry", name));
        }
        if response.status >= 400 {
            return Err(format!(
                "pull failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_pull_response(&response.body))
    }

    /// Search the registry.
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, String> {
        let path = format!("/api/v1/search?q={}", url_encode(query));
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "search failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_search_response(&response.body))
    }

    /// Search by type signature.
    pub fn search_by_type(&self, type_sig: &str) -> Result<Vec<SearchResult>, String> {
        let path = format!("/api/v1/search?type={}", url_encode(type_sig));
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "search failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_search_response(&response.body))
    }

    /// Search by tag.
    pub fn search_by_tag(&self, tag: &str) -> Result<Vec<SearchResult>, String> {
        let path = format!("/api/v1/search?tag={}", url_encode(tag));
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "search failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_search_response(&response.body))
    }

    /// Check registry health.
    pub fn health(&self) -> Result<bool, String> {
        let response = self.http_get("/health")?;
        Ok(response.status == 200)
    }

    /// Get registry statistics.
    pub fn stats(&self) -> Result<String, String> {
        let response = self.http_get("/api/v1/stats")?;
        if response.status >= 400 {
            return Err(format!(
                "stats failed ({}): {}",
                response.status, response.body
            ));
        }
        Ok(response.body)
    }

    /// Get transitive dependencies.
    pub fn deps(&self, hash: &str) -> Result<Vec<(String, String)>, String> {
        let path = format!("/api/v1/deps/{}", hash);
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "deps failed ({}): {}",
                response.status, response.body
            ));
        }

        let mut result = Vec::new();
        let body = &response.body;
        let deps_start = body.find('[').unwrap_or(body.len());
        let deps_end = body.rfind(']').unwrap_or(body.len());
        if deps_start < deps_end {
            let deps_str = &body[deps_start + 1..deps_end];
            for obj in deps_str.split("},") {
                let name = extract_json_string(obj, "name");
                let hash = extract_json_string(obj, "hash");
                if !hash.is_empty() {
                    result.push((name, hash));
                }
            }
        }

        Ok(result)
    }

    // ─── HTTP Transport ───────────────────────────────────────

    fn http_get(&self, path: &str) -> Result<ClientResponse, String> {
        let (host, port, scheme_host) = parse_url(&self.base_url)?;
        let addr = format!("{}:{}", host, port);

        let stream =
            TcpStream::connect(&addr).map_err(|e| format!("cannot connect to {}: {}", addr, e))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| format!("set timeout: {}", e))?;

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: trident/0.1\r\n\r\n",
            path, scheme_host,
        );

        (&stream)
            .write_all(request.as_bytes())
            .map_err(|e| format!("write request: {}", e))?;

        read_response(&stream)
    }

    fn http_post(&self, path: &str, body: &str) -> Result<ClientResponse, String> {
        let (host, port, scheme_host) = parse_url(&self.base_url)?;
        let addr = format!("{}:{}", host, port);

        let stream =
            TcpStream::connect(&addr).map_err(|e| format!("cannot connect to {}: {}", addr, e))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| format!("set timeout: {}", e))?;

        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nUser-Agent: trident/0.1\r\n\r\n{}",
            path, scheme_host, body.len(), body,
        );

        (&stream)
            .write_all(request.as_bytes())
            .map_err(|e| format!("write request: {}", e))?;

        read_response(&stream)
    }
}

struct ClientResponse {
    status: u16,
    body: String,
}

fn read_response(stream: &TcpStream) -> Result<ClientResponse, String> {
    let mut reader = BufReader::new(stream);

    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| format!("read status: {}", e))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(500);

    let mut content_length: usize = 0;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("read header: {}", e))?;
        let line = line.trim().to_string();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim();
            if key == "content-length" {
                content_length = value.parse().unwrap_or(0);
            } else if key == "transfer-encoding" && value.to_lowercase().contains("chunked") {
                chunked = true;
            }
        }
    }

    let body = if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        std::io::Read::read_exact(&mut reader, &mut buf)
            .map_err(|e| format!("read body: {}", e))?;
        String::from_utf8(buf).unwrap_or_default()
    } else if chunked {
        let mut body = String::new();
        loop {
            let mut chunk_line = String::new();
            reader
                .read_line(&mut chunk_line)
                .map_err(|e| format!("read chunk size: {}", e))?;
            let chunk_size = usize::from_str_radix(chunk_line.trim(), 16).unwrap_or(0);
            if chunk_size == 0 {
                break;
            }
            let mut chunk = vec![0u8; chunk_size];
            std::io::Read::read_exact(&mut reader, &mut chunk)
                .map_err(|e| format!("read chunk: {}", e))?;
            body.push_str(&String::from_utf8(chunk).unwrap_or_default());
            let mut crlf = String::new();
            let _ = reader.read_line(&mut crlf);
        }
        body
    } else {
        let mut body = String::new();
        let _ = std::io::Read::read_to_string(&mut reader, &mut body);
        body
    };

    Ok(ClientResponse { status, body })
}

// ─── Publish from Local UCM ───────────────────────────────────────

/// Publish all definitions from the local UCM codebase to a registry.
pub fn publish_codebase(
    codebase: &Codebase,
    client: &RegistryClient,
    tags: &[String],
) -> Result<Vec<PublishResult>, String> {
    let names = codebase.list_names();
    let mut results = Vec::new();

    for (name, hash) in &names {
        let def = match codebase.lookup_hash(hash) {
            Some(d) => d,
            None => continue,
        };

        let pub_def = PublishedDefinition {
            hash: hash.to_hex(),
            source: def.source.clone(),
            module: def.module.clone(),
            is_pub: def.is_pub,
            params: def.params.clone(),
            return_ty: def.return_ty.clone(),
            dependencies: def.dependencies.iter().map(|h| h.to_hex()).collect(),
            requires: def.requires.clone(),
            ensures: def.ensures.clone(),
            name: Some(name.to_string()),
            tags: tags.to_vec(),
            verified: false,
            verification_cert: None,
        };

        match client.publish(&pub_def) {
            Ok(result) => results.push(result),
            Err(e) => {
                eprintln!("  warning: failed to publish '{}': {}", name, e);
            }
        }
    }

    Ok(results)
}

/// Pull a definition from a registry into the local UCM codebase.
pub fn pull_into_codebase(
    codebase: &mut Codebase,
    client: &RegistryClient,
    name_or_hash: &str,
) -> Result<PullResult, String> {
    let pull = if name_or_hash.len() == 64 && name_or_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        client.pull(name_or_hash)?
    } else {
        client.pull_by_name(name_or_hash)?
    };

    let hash =
        parse_hex_hash(&pull.hash).ok_or_else(|| "invalid hash in pull response".to_string())?;

    if codebase.lookup_hash(&hash).is_some() {
        return Ok(pull);
    }

    let deps: Vec<ContentHash> = pull
        .dependencies
        .iter()
        .filter_map(|h| parse_hex_hash(h))
        .collect();

    let def = Definition {
        source: pull.source.clone(),
        module: pull.module.clone(),
        is_pub: true,
        params: pull.params.clone(),
        return_ty: pull.return_ty.clone(),
        dependencies: deps,
        requires: pull.requires.clone(),
        ensures: pull.ensures.clone(),
        first_seen: unix_timestamp(),
    };

    codebase.store_definition(hash, def);

    if name_or_hash.len() != 64 || !name_or_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        codebase.bind_name(name_or_hash, hash);
    }

    codebase.save().map_err(|e| e.to_string())?;

    Ok(pull)
}

// ─── JSON Helpers ─────────────────────────────────────────────────

fn json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Find a top-level JSON key (depth 1) and return its byte offset.
/// Skips keys nested inside arrays or sub-objects by tracking
/// brace/bracket nesting while being aware of JSON strings.
fn find_toplevel_key(json: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{}\":", key);
    let bytes = json.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            // Skip over the entire JSON string (key or value).
            // Record the start position — we may need to match here.
            let start = i;
            i += 1; // skip opening quote
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2; // skip escaped char
                } else if bytes[i] == b'"' {
                    i += 1; // skip closing quote
                    break;
                } else {
                    i += 1;
                }
            }
            // At depth 1, check if this position is our needle.
            if depth == 1 && json[start..].starts_with(&needle) {
                return Some(start);
            }
            continue;
        }
        match b {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    None
}

fn extract_json_string(json: &str, key: &str) -> String {
    let needle = format!("\"{}\":", key);
    if let Some(pos) = find_toplevel_key(json, key) {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        if after.starts_with('"') {
            let inner = &after[1..];
            let mut result = String::new();
            let mut chars = inner.chars();
            while let Some(ch) = chars.next() {
                if ch == '"' {
                    break;
                }
                if ch == '\\' {
                    match chars.next() {
                        Some('n') => result.push('\n'),
                        Some('r') => result.push('\r'),
                        Some('t') => result.push('\t'),
                        Some('"') => result.push('"'),
                        Some('\\') => result.push('\\'),
                        Some(c) => {
                            result.push('\\');
                            result.push(c);
                        }
                        None => break,
                    }
                } else {
                    result.push(ch);
                }
            }
            return result;
        }
    }
    String::new()
}

fn extract_json_bool(json: &str, key: &str) -> bool {
    let needle = format!("\"{}\":", key);
    if let Some(pos) = find_toplevel_key(json, key) {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        return after.starts_with("true");
    }
    false
}

fn extract_json_array_strings(json: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{}\":", key);
    let mut results = Vec::new();
    if let Some(pos) = find_toplevel_key(json, key) {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        if after.starts_with('[') {
            let bracket_end = find_matching_bracket(after);
            let inner = &after[1..bracket_end];
            for item in inner.split(',') {
                let item = item.trim();
                if item.starts_with('"') && item.ends_with('"') {
                    results.push(item[1..item.len() - 1].to_string());
                }
            }
        }
    }
    results
}

fn find_matching_bracket(s: &str) -> usize {
    let mut depth = 0;
    for (i, ch) in s.chars().enumerate() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

fn format_publish_json(def: &PublishedDefinition) -> String {
    let deps: Vec<String> = def
        .dependencies
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect();
    let params: Vec<String> = def
        .params
        .iter()
        .map(|(n, t)| {
            format!(
                "{{\"name\":{},\"type\":{}}}",
                json_escape(n),
                json_escape(t)
            )
        })
        .collect();
    let requires: Vec<String> = def.requires.iter().map(|r| json_escape(r)).collect();
    let ensures: Vec<String> = def.ensures.iter().map(|e| json_escape(e)).collect();
    let tags: Vec<String> = def.tags.iter().map(|t| json_escape(t)).collect();

    format!(
        "{{\"hash\":\"{}\",\"source\":{},\"module\":{},\"is_pub\":{},\"params\":[{}],\"return_ty\":{},\"dependencies\":[{}],\"requires\":[{}],\"ensures\":[{}],\"name\":{},\"tags\":[{}],\"verified\":{},\"verification_cert\":{}}}",
        def.hash,
        json_escape(&def.source),
        json_escape(&def.module),
        def.is_pub,
        params.join(","),
        def.return_ty.as_ref().map(|t| json_escape(t)).unwrap_or_else(|| "null".to_string()),
        deps.join(","),
        requires.join(","),
        ensures.join(","),
        def.name.as_ref().map(|n| json_escape(n)).unwrap_or_else(|| "null".to_string()),
        tags.join(","),
        def.verified,
        def.verification_cert.as_ref().map(|c| json_escape(c)).unwrap_or_else(|| "null".to_string()),
    )
}

#[cfg(test)]
fn parse_publish_body(body: &str) -> Result<PublishedDefinition, String> {
    let hash = extract_json_string(body, "hash");
    if hash.is_empty() {
        return Err("missing 'hash' field".to_string());
    }
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid hash format (expected 64 hex chars)".to_string());
    }

    let source = extract_json_string(body, "source");
    if source.is_empty() {
        return Err("missing 'source' field".to_string());
    }

    let module = extract_json_string(body, "module");
    let is_pub = extract_json_bool(body, "is_pub");
    let return_ty = {
        let rt = extract_json_string(body, "return_ty");
        if rt.is_empty() {
            None
        } else {
            Some(rt)
        }
    };

    let params = extract_params_array(body);
    let dependencies = extract_json_array_strings(body, "dependencies");
    let requires = extract_json_array_strings(body, "requires");
    let ensures = extract_json_array_strings(body, "ensures");
    let tags = extract_json_array_strings(body, "tags");
    let name = {
        let n = extract_json_string(body, "name");
        if n.is_empty() {
            None
        } else {
            Some(n)
        }
    };
    let verified = extract_json_bool(body, "verified");
    let verification_cert = {
        let vc = extract_json_string(body, "verification_cert");
        if vc.is_empty() {
            None
        } else {
            Some(vc)
        }
    };

    Ok(PublishedDefinition {
        hash,
        source,
        module,
        is_pub,
        params,
        return_ty,
        dependencies,
        requires,
        ensures,
        name,
        tags,
        verified,
        verification_cert,
    })
}

fn extract_params_array(json: &str) -> Vec<(String, String)> {
    let needle = "\"params\":";
    let mut results = Vec::new();
    if let Some(pos) = find_toplevel_key(json, "params") {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        if after.starts_with('[') {
            let bracket_end = find_matching_bracket(after);
            let inner = &after[1..bracket_end];
            for obj in inner.split("},") {
                let name = extract_json_string(obj, "name");
                let ty = extract_json_string(obj, "type");
                if !name.is_empty() {
                    results.push((name, ty));
                }
            }
        }
    }
    results
}

fn parse_pull_response(body: &str) -> PullResult {
    PullResult {
        hash: extract_json_string(body, "hash"),
        source: extract_json_string(body, "source"),
        module: extract_json_string(body, "module"),
        params: extract_params_array(body),
        return_ty: {
            let rt = extract_json_string(body, "return_ty");
            if rt.is_empty() {
                None
            } else {
                Some(rt)
            }
        },
        dependencies: extract_json_array_strings(body, "dependencies"),
        requires: extract_json_array_strings(body, "requires"),
        ensures: extract_json_array_strings(body, "ensures"),
    }
}

fn parse_search_response(body: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let needle = "\"results\":[";
    if let Some(pos) = body.find(needle) {
        let after = &body[pos + needle.len() - 1..]; // include the [
        let bracket_end = find_matching_bracket(after);
        let inner = &after[1..bracket_end];

        for obj in inner.split("},{") {
            let name = extract_json_string(obj, "name");
            let hash = extract_json_string(obj, "hash");
            let module = extract_json_string(obj, "module");
            let signature = extract_json_string(obj, "signature");
            let verified = extract_json_bool(obj, "verified");
            let tags = extract_json_array_strings(obj, "tags");

            if !hash.is_empty() {
                results.push(SearchResult {
                    name,
                    hash,
                    module,
                    signature,
                    verified,
                    tags,
                });
            }
        }
    }
    results
}

// ─── URL / HTTP Helpers ───────────────────────────────────────────

fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let url = url.trim();
    let without_scheme = if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else if url.starts_with("https://") {
        return Err("HTTPS not supported (use HTTP for local registries)".to_string());
    } else {
        url
    };

    let (host_port, _path) = without_scheme
        .split_once('/')
        .unwrap_or((without_scheme, ""));
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let port: u16 = p.parse().map_err(|_| "invalid port".to_string())?;
        (h.to_string(), port)
    } else {
        (host_port.to_string(), 80)
    };

    Ok((host, port, host_port.to_string()))
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

// ─── Hex Hash Parsing ────────────────────────────────────────────

fn parse_hex_hash(hex: &str) -> Option<ContentHash> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 32 || chunk.len() < 2 {
            return None;
        }
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        bytes[i] = (hi << 4) | lo;
    }
    Some(ContentHash(bytes))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(parse_hex_hash(&hex).is_some());
    }

    #[test]
    fn test_parse_hex_hash_invalid_length() {
        assert!(parse_hex_hash("abc").is_none());
        assert!(parse_hex_hash(&"a".repeat(63)).is_none());
        assert!(parse_hex_hash(&"a".repeat(65)).is_none());
    }

    #[test]
    fn test_parse_hex_hash_invalid_chars() {
        let mut hex = "a".repeat(64);
        hex.replace_range(0..1, "g");
        assert!(parse_hex_hash(&hex).is_none());
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
}
