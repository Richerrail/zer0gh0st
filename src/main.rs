//! Lanceur du binaire `zer0`. Tout le code vit dans la bibliothèque `zer0`
//! (voir `src/lib.rs`) : c'est ce qui rend le noyau testable depuis `tests/`.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    zer0::cli::entry().await
}
