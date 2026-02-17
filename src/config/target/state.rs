use super::*;

// ─── State Configuration ───────────────────────────────────────────

/// A state is a sovereign chain instance within a union (network).
///
/// States share their union's protocol and engine, but have
/// independent ledgers, validators, and economies. Ethereum
/// Mainnet and Optimism are different states in the Ethereum union.
///
/// State config is purely deployment metadata — no impact on
/// compilation. Only relevant for deploy, run, prove, verify.
#[derive(Clone, Debug)]
pub struct StateConfig {
    /// State name (e.g. "mainnet").
    pub name: String,
    /// Display name (e.g. "Neptune Mainnet").
    pub display_name: String,
    /// Parent union name (e.g. "neptune").
    pub union: String,
    /// Chain identifier (e.g. "1").
    pub chain_id: String,
    /// RPC endpoint URL.
    pub rpc_url: String,
    /// Block explorer URL.
    pub explorer_url: String,
    /// Native currency symbol (e.g. "NEPT").
    pub currency_symbol: String,
    /// Whether this is the default state for its union.
    pub is_default: bool,
}

impl StateConfig {
    /// Try to resolve a state config by union and state name.
    ///
    /// Searches for `os/<union>/states/<name>.toml` relative to the
    /// compiler binary and the current working directory.
    /// Returns `Ok(None)` if no state config file exists.
    /// Returns `Err` if the file exists but is malformed.
    pub fn resolve(union: &str, state_name: &str) -> Result<Option<Self>, Diagnostic> {
        // Reject path traversal
        if union.contains('/')
            || union.contains('\\')
            || union.contains("..")
            || union.starts_with('.')
        {
            return Ok(None);
        }
        if state_name.contains('/')
            || state_name.contains('\\')
            || state_name.contains("..")
            || state_name.starts_with('.')
        {
            return Ok(None);
        }

        let target_path = format!("os/{}/states/{}.toml", union, state_name);

        // 1. Relative to compiler binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for ancestor in &[
                    Some(dir.to_path_buf()),
                    dir.parent().map(|p| p.to_path_buf()),
                    dir.parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf()),
                ] {
                    if let Some(base) = ancestor {
                        let path = base.join(&target_path);
                        if path.exists() {
                            return Self::load(&path).map(Some);
                        }
                    }
                }
            }
        }

        // 2. Current working directory
        let cwd_path = std::path::PathBuf::from(&target_path);
        if cwd_path.exists() {
            return Self::load(&cwd_path).map(Some);
        }

        Ok(None)
    }

    /// Find the default state for a union.
    ///
    /// Scans `os/<union>/states/` for any TOML with `is_default = true`.
    /// Returns the first default found, or `Ok(None)` if none.
    pub fn default_for_union(union: &str) -> Result<Option<Self>, Diagnostic> {
        // Reject path traversal
        if union.contains('/')
            || union.contains('\\')
            || union.contains("..")
            || union.starts_with('.')
        {
            return Ok(None);
        }

        let states_dir = format!("os/{}/states", union);

        // Collect candidate directories from search paths
        let mut dirs_to_scan: Vec<std::path::PathBuf> = Vec::new();

        // 1. Relative to compiler binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for ancestor in &[
                    Some(dir.to_path_buf()),
                    dir.parent().map(|p| p.to_path_buf()),
                    dir.parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf()),
                ] {
                    if let Some(base) = ancestor {
                        let path = base.join(&states_dir);
                        if path.is_dir() {
                            dirs_to_scan.push(path);
                        }
                    }
                }
            }
        }

        // 2. Current working directory
        let cwd_path = std::path::PathBuf::from(&states_dir);
        if cwd_path.is_dir() {
            dirs_to_scan.push(cwd_path);
        }

        for dir in dirs_to_scan {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                        if let Ok(config) = Self::load(&path) {
                            if config.is_default {
                                return Ok(Some(config));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// List available state names for a union.
    ///
    /// Scans `os/<union>/states/*.toml` and returns filenames (stem only).
    pub fn list_states(union: &str) -> Vec<String> {
        // Reject path traversal
        if union.contains('/')
            || union.contains('\\')
            || union.contains("..")
            || union.starts_with('.')
        {
            return Vec::new();
        }

        let states_dir = format!("os/{}/states", union);
        let mut names = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        let mut dirs_to_scan: Vec<std::path::PathBuf> = Vec::new();

        // 1. Relative to compiler binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for ancestor in &[
                    Some(dir.to_path_buf()),
                    dir.parent().map(|p| p.to_path_buf()),
                    dir.parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf()),
                ] {
                    if let Some(base) = ancestor {
                        let path = base.join(&states_dir);
                        if path.is_dir() {
                            dirs_to_scan.push(path);
                        }
                    }
                }
            }
        }

        // 2. Current working directory
        let cwd_path = std::path::PathBuf::from(&states_dir);
        if cwd_path.is_dir() {
            dirs_to_scan.push(cwd_path);
        }

        for dir in dirs_to_scan {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            let name = stem.to_string();
                            if seen.insert(name.clone()) {
                                names.push(name);
                            }
                        }
                    }
                }
            }
        }

        names.sort();
        names
    }

    /// Load a state config from a TOML file.
    pub fn load(path: &Path) -> Result<Self, Diagnostic> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::error(
                format!("cannot read state config '{}': {}", path.display(), e),
                Span::dummy(),
            )
        })?;
        Self::parse_toml(&content, path)
    }

    fn parse_toml(content: &str, path: &Path) -> Result<Self, Diagnostic> {
        let err =
            |msg: String| Diagnostic::error(format!("{}: {}", path.display(), msg), Span::dummy());

        let mut name = String::new();
        let mut display_name = String::new();
        let mut union = String::new();
        let mut chain_id = String::new();
        let mut is_default = false;
        let mut rpc_url = String::new();
        let mut explorer_url = String::new();
        let mut currency_symbol = String::new();

        let mut section = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].trim().to_string();
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                let unquoted = value.trim_matches('"');

                match (section.as_str(), key) {
                    ("state", "name") => name = unquoted.to_string(),
                    ("state", "display_name") => display_name = unquoted.to_string(),
                    ("state", "union") => union = unquoted.to_string(),
                    ("state", "chain_id") => chain_id = unquoted.to_string(),
                    ("state", "is_default") => is_default = value == "true",
                    ("endpoints", "rpc_url") => rpc_url = unquoted.to_string(),
                    ("endpoints", "explorer_url") => explorer_url = unquoted.to_string(),
                    ("currency", "symbol") => currency_symbol = unquoted.to_string(),
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return Err(err("missing state.name".to_string()));
        }
        if union.is_empty() {
            return Err(err("missing state.union".to_string()));
        }

        Ok(Self {
            name,
            display_name,
            union,
            chain_id,
            rpc_url,
            explorer_url,
            currency_symbol,
            is_default,
        })
    }
}
