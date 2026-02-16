/// Maximum allowed HTTP response body size (10 MiB).
/// Responses exceeding this limit are rejected to prevent memory exhaustion
/// from malicious or misconfigured servers.
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use super::json::{
    extract_json_bool, extract_json_string, format_publish_json, parse_pull_response,
    parse_search_response,
};
use super::types::*;

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

        let sock_addr: std::net::SocketAddr = addr
            .parse()
            .or_else(|_| {
                use std::net::ToSocketAddrs;
                addr.to_socket_addrs()
                    .map_err(|e| format!("resolve {}: {}", addr, e))?
                    .next()
                    .ok_or_else(|| format!("no addresses for {}", addr))
            })
            .map_err(|e| format!("cannot resolve {}: {}", addr, e))?;
        let stream = TcpStream::connect_timeout(&sock_addr, std::time::Duration::from_secs(10))
            .map_err(|e| format!("cannot connect to {}: {}", addr, e))?;
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

        let sock_addr: std::net::SocketAddr = addr
            .parse()
            .or_else(|_| {
                use std::net::ToSocketAddrs;
                addr.to_socket_addrs()
                    .map_err(|e| format!("resolve {}: {}", addr, e))?
                    .next()
                    .ok_or_else(|| format!("no addresses for {}", addr))
            })
            .map_err(|e| format!("cannot resolve {}: {}", addr, e))?;
        let stream = TcpStream::connect_timeout(&sock_addr, std::time::Duration::from_secs(10))
            .map_err(|e| format!("cannot connect to {}: {}", addr, e))?;
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

    if content_length > MAX_RESPONSE_SIZE {
        return Err(format!(
            "response too large: Content-Length {} exceeds limit of {} bytes",
            content_length, MAX_RESPONSE_SIZE,
        ));
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
            if body.len() + chunk_size > MAX_RESPONSE_SIZE {
                return Err(format!(
                    "chunked response too large: exceeds limit of {} bytes",
                    MAX_RESPONSE_SIZE,
                ));
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
        let mut body = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = std::io::Read::read(&mut reader, &mut buf)
                .map_err(|e| format!("read body: {}", e))?;
            if n == 0 {
                break;
            }
            if body.len() + n > MAX_RESPONSE_SIZE {
                return Err(format!(
                    "response too large: exceeds limit of {} bytes",
                    MAX_RESPONSE_SIZE,
                ));
            }
            body.extend_from_slice(&buf[..n]);
        }
        String::from_utf8(body).unwrap_or_default()
    };

    Ok(ClientResponse { status, body })
}

pub(super) fn parse_url(url: &str) -> Result<(String, u16, String), String> {
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

pub(super) fn url_encode(s: &str) -> String {
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
