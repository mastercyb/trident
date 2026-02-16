use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::target::parse_string_array;
use crate::diagnostic::Diagnostic;
use crate::manifest::Manifest;
use crate::span::Span;

/// Maximum allowed length for a project name.
const MAX_PROJECT_NAME_LEN: usize = 128;

/// Validate a project name from trident.toml.
///
/// Rejects names that contain path separators (`/`, `\`), parent-directory
/// traversals (`..`), control characters, or that exceed 128 bytes.
pub fn validate_project_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("project name must not be empty".to_string());
    }
    if name.len() > MAX_PROJECT_NAME_LEN {
        return Err(format!(
            "project name exceeds {} characters",
            MAX_PROJECT_NAME_LEN
        ));
    }
    if name.contains('/') || name.contains('\\') {
        return Err("project name must not contain path separators ('/' or '\\')".to_string());
    }
    if name.contains("..") {
        return Err("project name must not contain '..'".to_string());
    }
    if name.chars().any(|c| c.is_control()) {
        return Err("project name must not contain control characters".to_string());
    }
    Ok(())
}

/// Minimal project configuration from trident.toml.
#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub version: String,
    pub entry: PathBuf,
    pub root_dir: PathBuf,
    /// VM target name (e.g. "triton"). If set, overrides --target default.
    pub target: Option<String>,
    /// Custom profile definitions: profile_name → list of cfg flags.
    /// E.g. `[targets.debug]` with `flags = ["debug", "verbose"]`.
    pub targets: BTreeMap<String, Vec<String>>,
    /// Parsed [dependencies] section.
    pub dependencies: Manifest,
}

impl Project {
    /// Load project from a trident.toml file.
    pub fn load(toml_path: &Path) -> Result<Project, Diagnostic> {
        let content = std::fs::read_to_string(toml_path).map_err(|e| {
            Diagnostic::error(
                format!("cannot read '{}': {}", toml_path.display(), e),
                Span::dummy(),
            )
        })?;

        let root_dir = toml_path.parent().unwrap_or(Path::new(".")).to_path_buf();

        // Section-aware minimal TOML parsing
        let mut name = String::new();
        let mut version = String::new();
        let mut entry = String::new();
        let mut vm_target: Option<String> = None;
        let mut targets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut current_section = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            // Section headers: [project], [targets.debug], etc.
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                current_section = trimmed[1..trimmed.len() - 1].trim().to_string();
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim().trim_matches('"');
                let value = value.trim();

                if current_section == "project" {
                    let value = value.trim_matches('"');
                    match key {
                        "name" => name = value.to_string(),
                        "version" => version = value.to_string(),
                        "entry" => entry = value.to_string(),
                        "target" => vm_target = Some(value.to_string()),
                        _ => {}
                    }
                } else if let Some(target_name) = current_section.strip_prefix("targets.") {
                    if key == "flags" {
                        // Parse array: ["flag1", "flag2"]
                        let flags = parse_string_array(value);
                        targets.insert(target_name.to_string(), flags);
                    }
                }
            }
        }

        if name.is_empty() {
            return Err(Diagnostic::error(
                "missing 'name' in trident.toml".to_string(),
                Span::dummy(),
            ));
        }

        if let Err(reason) = validate_project_name(&name) {
            return Err(Diagnostic::error(
                format!("invalid project name '{}': {}", name, reason),
                Span::dummy(),
            ));
        }

        if entry.is_empty() {
            entry = "main.tri".to_string();
        }

        let dependencies = crate::manifest::parse_dependencies(&content);

        Ok(Project {
            name,
            version,
            entry: root_dir.join(&entry),
            root_dir,
            target: vm_target,
            targets,
            dependencies,
        })
    }

    /// Try to find a trident.toml in the given directory or its ancestors.
    pub fn find(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = start_dir.to_path_buf();
        loop {
            let candidate = dir.join("trident.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_project() {
        let dir = tempfile::tempdir().unwrap();
        let toml_path = dir.path().join("trident.toml");
        fs::write(
            &toml_path,
            r#"[project]
name = "test_project"
version = "0.1.0"
entry = "main.tri"
"#,
        )
        .unwrap();

        let project = Project::load(&toml_path).unwrap();
        assert_eq!(project.name, "test_project");
        assert_eq!(project.version, "0.1.0");
        assert!(project.entry.ends_with("main.tri"));
        assert!(project.targets.is_empty());
    }

    #[test]
    fn test_load_project_with_targets() {
        let dir = tempfile::tempdir().unwrap();
        let toml_path = dir.path().join("trident.toml");
        fs::write(
            &toml_path,
            r#"[project]
name = "my_app"
version = "0.2.0"
entry = "main.tri"

[targets.debug]
flags = ["debug", "verbose"]

[targets.release]
flags = ["release"]
"#,
        )
        .unwrap();

        let project = Project::load(&toml_path).unwrap();
        assert_eq!(project.name, "my_app");
        assert_eq!(project.targets.len(), 2);

        let debug_flags = project.targets.get("debug").unwrap();
        assert_eq!(
            debug_flags,
            &vec!["debug".to_string(), "verbose".to_string()]
        );

        let release_flags = project.targets.get("release").unwrap();
        assert_eq!(release_flags, &vec!["release".to_string()]);
    }

    #[test]
    fn test_parse_string_array() {
        assert_eq!(
            parse_string_array(r#"["a", "b", "c"]"#),
            vec!["a", "b", "c"]
        );
        assert_eq!(parse_string_array(r#"["single"]"#), vec!["single"]);
        assert!(parse_string_array("not_an_array").is_empty());
    }

    #[test]
    fn validate_project_name_rejects_path_separators() {
        assert!(validate_project_name("foo/bar").is_err());
        assert!(validate_project_name("foo\\bar").is_err());
    }

    #[test]
    fn validate_project_name_rejects_dot_dot() {
        assert!(validate_project_name("..sneaky").is_err());
        assert!(validate_project_name("a..b").is_err());
    }

    #[test]
    fn validate_project_name_rejects_control_chars() {
        assert!(validate_project_name("bad\x00name").is_err());
        assert!(validate_project_name("bad\nname").is_err());
    }

    #[test]
    fn validate_project_name_rejects_overlong() {
        let long = "a".repeat(129);
        assert!(validate_project_name(&long).is_err());
    }

    #[test]
    fn validate_project_name_accepts_valid() {
        assert!(validate_project_name("my_project").is_ok());
        assert!(validate_project_name("hello-world").is_ok());
        assert!(validate_project_name("a").is_ok());
        let exactly_128 = "x".repeat(128);
        assert!(validate_project_name(&exactly_128).is_ok());
    }

    #[test]
    fn load_rejects_invalid_project_name() {
        let dir = tempfile::tempdir().unwrap();
        let toml_path = dir.path().join("trident.toml");
        fs::write(
            &toml_path,
            r#"[project]
name = "bad/name"
version = "0.1.0"
"#,
        )
        .unwrap();
        assert!(Project::load(&toml_path).is_err());
    }
}
