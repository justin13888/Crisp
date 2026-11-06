//! The `revocations` table (spec §8.8) and the migration that creates it.
//!
//! One row per bundle whose endorsement the server revoked, keyed by the
//! bundle's hex `content_hash`. A takedown writes the rows; nothing deletes
//! them, because a revocation is permanent: lifting a takedown lets the
//! document be published again, as a new bundle with a new endorsement.

/// The `revocations` table — one revoked endorsement per row.
pub mod revocation {
    use sea_orm::entity::prelude::*;

    /// A bundle whose endorsement this server revoked.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "revocations")]
    pub struct Model {
        /// The revoked bundle's `content_hash`, hex-encoded.
        #[sea_orm(primary_key, auto_increment = false)]
        pub content_hash: String,
        /// When the endorsement was revoked (epoch seconds).
        pub revoked_at: i64,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub(super) use migration::Revocations;

/// The migration creating `revocations`.
mod migration {
    use sea_orm_migration::prelude::*;
    use sea_orm_migration::sea_orm::ConnectionTrait;

    /// Creates `revocations`.
    pub struct Revocations;

    impl MigrationName for Revocations {
        fn name(&self) -> &str {
            "m20261106_000005_revocations"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for Revocations {
        async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager.get_connection().execute_unprepared(UP).await?;
            Ok(())
        }

        async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager.get_connection().execute_unprepared(DOWN).await?;
            Ok(())
        }
    }

    /// The forward migration.
    const UP: &str = "
        CREATE TABLE revocations (
            content_hash  TEXT PRIMARY KEY,
            revoked_at    BIGINT NOT NULL
        );
    ";

    /// The reverse migration.
    const DOWN: &str = "DROP TABLE IF EXISTS revocations;";
}
