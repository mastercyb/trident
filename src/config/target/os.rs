use super::*;

// ─── OS Target Configuration ───────────────────────────────────────

/// OS target configuration parsed from `os/<name>/target.toml`.
///
/// An OS target describes a blockchain or runtime environment that
/// runs on top of a VM. The `vm` field maps the OS to its underlying
/// VM (e.g. "neptune" → "triton", "starknet" → "cairo").
#[derive(Clone, Debug)]
pub struct UnionConfig {
    /// OS name (e.g. "neptune").
    pub name: String,
    /// Display name (e.g. "Neptune").
    pub display_name: String,
    /// Underlying VM name (e.g. "triton").
    pub vm: String,
    /// Runtime binding prefix (e.g. "os.neptune").
    pub binding_prefix: String,
    /// Account model (e.g. "utxo", "account").
    pub account_model: String,
    /// Storage model (e.g. "merkle-authenticated", "key-value").
    pub storage_model: String,
    /// Transaction model (e.g. "proof-based", "signed").
    pub transaction_model: String,
}

impl UnionConfig {
    /// Try to resolve an OS config by name.
    ///
    /// Searches for `os/<name>/target.toml` relative to the compiler
    /// binary and the current working directory.
    /// Returns `Ok(None)` if no OS config file exists for this name.
    /// Returns `Err` if the file exists but is malformed.
    pub fn resolve(name: &str) -> Result<Option<Self>, Diagnostic> {
        // Reject path traversal
        if name.contains('/') || name.contains('\\') || name.contains("..") || name.starts_with('.')
        {
            return Ok(None);
        }

        let target_path = format!("os/{}/target.toml", name);

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

    /// Load an OS config from a TOML file.
    pub fn load(path: &Path) -> Result<Self, Diagnostic> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::error(
                format!("cannot read OS config '{}': {}", path.display(), e),
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
        let mut vm = String::new();
        let mut binding_prefix = String::new();
        let mut account_model = String::new();
        let mut storage_model = String::new();
        let mut transaction_model = String::new();

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
                let unquoted = value.trim().trim_matches('"');

                match (section.as_str(), key) {
                    ("os", "name") => name = unquoted.to_string(),
                    ("os", "display_name") => display_name = unquoted.to_string(),
                    ("os", "vm") => vm = unquoted.to_string(),
                    ("runtime", "binding_prefix") => binding_prefix = unquoted.to_string(),
                    ("runtime", "account_model") => account_model = unquoted.to_string(),
                    ("runtime", "storage_model") => storage_model = unquoted.to_string(),
                    ("runtime", "transaction_model") => transaction_model = unquoted.to_string(),
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return Err(err("missing os.name".to_string()));
        }
        if vm.is_empty() {
            return Err(err("missing os.vm".to_string()));
        }

        Ok(Self {
            name,
            display_name,
            vm,
            binding_prefix,
            account_model,
            storage_model,
            transaction_model,
        })
    }
}

// ─── Combined Target Resolution ────────────────────────────────────

/// Resolved target: either a bare VM or an OS+VM combination.
///
/// When the user passes `--target neptune`, we load the OS config first
/// (which tells us the VM is "triton"), then load the VM config. When
/// they pass `--target triton`, we load the VM config directly.
#[derive(Clone, Debug)]
pub struct ResolvedTarget {
    /// VM configuration (always present).
    pub vm: TerrainConfig,
    /// OS configuration (present only if the target name was an OS).
    pub os: Option<UnionConfig>,
    /// State configuration (present only if explicitly specified).
    pub state: Option<StateConfig>,
}

impl ResolvedTarget {
    /// Resolve a target name: try OS first, then fall back to VM.
    ///
    /// This matches the resolution order from `reference/targets.md`:
    /// 1. Is `<name>` an OS? Load `os/<name>/target.toml`, derive VM.
    /// 2. Is `<name>` a VM? Load `vm/<name>/target.toml`.
    /// 3. Neither? Error.
    pub fn resolve(name: &str) -> Result<Self, Diagnostic> {
        // 1. Try OS
        if let Some(os_config) = UnionConfig::resolve(name)? {
            let vm = TerrainConfig::resolve(&os_config.vm)?;
            return Ok(ResolvedTarget {
                vm,
                os: Some(os_config),
                state: None,
            });
        }

        // 2. Try VM
        let vm = TerrainConfig::resolve(name)?;
        Ok(ResolvedTarget {
            vm,
            os: None,
            state: None,
        })
    }

    /// Resolve a target with an explicit state.
    ///
    /// A state requires a union target — bare terrain (VM) targets
    /// cannot have states because states are chain instances within
    /// a network.
    pub fn resolve_with_state(target: &str, state_name: Option<&str>) -> Result<Self, Diagnostic> {
        let mut resolved = Self::resolve(target)?;
        if let Some(state) = state_name {
            let union = resolved
                .os
                .as_ref()
                .map(|os| os.name.as_str())
                .ok_or_else(|| {
                    Diagnostic::error(
                        format!(
                            "--state requires a union target, not bare terrain '{}'",
                            target
                        ),
                        Span::dummy(),
                    )
                })?;
            resolved.state = StateConfig::resolve(union, state)?;
            if resolved.state.is_none() {
                return Err(Diagnostic::error(
                    format!("unknown state '{}' for union '{}'", state, union),
                    Span::dummy(),
                )
                .with_help(format!(
                    "available states: {}",
                    StateConfig::list_states(union).join(", ")
                )));
            }
        }
        Ok(resolved)
    }
}

/// Parse a minimal TOML string array: `["a", "b", "c"]` → `vec!["a", "b", "c"]`.
pub fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Vec::new();
    }
    let inner = &s[1..s.len() - 1];
    inner
        .split(',')
        .map(|part| part.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
