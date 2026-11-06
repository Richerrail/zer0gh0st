#[tokio::main]
async fn main() -> anyhow::Result<()> {
    zer0::cli::entry().await
}
