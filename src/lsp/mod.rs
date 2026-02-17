//! Trident Language Server Protocol implementation.

mod actions;
mod builtins;
mod document;
mod folding;
mod hints;
mod incremental;
mod indent;
mod intelligence;
mod project;
mod references;
mod selection;
mod semantic;
mod server;
#[allow(dead_code)] // library API for Rust editor embedding
mod textobjects;
pub mod util;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LspService, Server};

use util::to_lsp_diagnostic;

pub(crate) struct TridentLsp {
    pub(crate) client: Client,
    pub(crate) documents: Mutex<BTreeMap<Url, document::DocumentData>>,
}

impl TridentLsp {
    async fn publish_diagnostics(&self, uri: Url, source: &str) {
        let file_path = PathBuf::from(uri.path());
        let result = crate::check_file_in_project(source, &file_path);

        let diagnostics = match result {
            Ok(()) => Vec::new(),
            Err(errors) => errors
                .into_iter()
                .map(|d| to_lsp_diagnostic(&d, source))
                .collect(),
        };

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

/// Start the LSP server on stdin/stdout.
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| TridentLsp {
        client,
        documents: Mutex::new(BTreeMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
