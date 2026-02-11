#[tokio::main]
async fn main() {
    trident::lsp::run_server().await;
}
