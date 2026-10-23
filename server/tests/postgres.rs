//! PostgreSQL-backed tests, gated on `CRISP_TEST_DATABASE_URL`.

/// Connects to a real PostgreSQL and runs the migration, when
/// `CRISP_TEST_DATABASE_URL` is set. Skipped (passing) otherwise.
#[tokio::test]
async fn postgres_smoke() {
    let Ok(url) = std::env::var("CRISP_TEST_DATABASE_URL") else {
        eprintln!("postgres_smoke skipped: set CRISP_TEST_DATABASE_URL to run it");
        return;
    };
    use sea_orm_migration::MigratorTrait;
    let db = sea_orm::Database::connect(&url).await.expect("connect");
    crisp_server::db::Migrator::up(&db, None)
        .await
        .expect("migrate");
}
