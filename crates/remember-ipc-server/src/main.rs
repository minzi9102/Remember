#[tokio::main]
async fn main() -> anyhow::Result<()> {
    remember_ipc_server::run().await
}
