//! Dev-only: run the importer against a local `json-assets.zip` (no network).
//! `cargo run --example import_local -- <zip> <release-tag>` with `HIRELING_DATABASE_URL` set.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(zip), Some(release)) = (args.next(), args.next()) else {
        anyhow::bail!("usage: import_local <json-assets.zip> <pf2e-N.N.N>");
    };
    let bytes = std::fs::read(&zip)?;
    let url = std::env::var("HIRELING_DATABASE_URL")?;
    let pool = sqlx::PgPool::connect(&url).await?;
    hireling::import::import_from_bytes(&pool, &release, &bytes, hireling::import::SEED_JSON_PUB)
        .await?;
    Ok(())
}
