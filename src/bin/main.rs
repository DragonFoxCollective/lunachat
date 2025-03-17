use lunachat::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("lunachat=trace")
        .init();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8002").await?;
    let app = lunachat::app().await?;
    axum::serve(listener, app).await?;

    Ok(())
}
