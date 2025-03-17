use lunachat::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    lunachat::versioning::migrate().await?;
    Ok(())
}
