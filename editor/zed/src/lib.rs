use zed_extension_api::{self as zed, Result};

struct TridentExtension;

impl zed::Extension for TridentExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let env = worktree.shell_env();
        let path = worktree.which("trident").or_else(|| {
            env.iter()
                .find(|(k, _)| k == "HOME")
                .map(|(_, home)| format!("{}/.cargo/bin/trident", home))
        });

        let path = path
            .ok_or_else(|| "trident not found in PATH. Run: cargo install --path <trident-repo>")?;

        Ok(zed::Command {
            command: path,
            args: vec!["lsp".into()],
            env,
        })
    }
}

zed::register_extension!(TridentExtension);
