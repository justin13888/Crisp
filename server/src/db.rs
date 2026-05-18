//! The PostgreSQL schema: SeaORM entities and the migration that creates them.
//!
//! Five tables back the server:
//!
//! - `accounts` — one per crypto-bound user (the `usr…` id is the key).
//! - `sessions` — issued bearer tokens.
//! - `auth_challenges` — short-lived login nonces.
//! - `bundles` — every endorsed `.crisp` file, stored verbatim.
//! - `recipes_index` — denormalized per-recipe metadata with a PostgreSQL
//!   full-text `tsvector`, so search never has to open a bundle.

/// The `accounts` table — a crypto-bound user account.
pub mod account {
    use sea_orm::entity::prelude::*;

    /// A registered account, keyed by its derived `usr…` identifier.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "accounts")]
    pub struct Model {
        /// The `usr…` id, derived from the public key set.
        #[sea_orm(primary_key, auto_increment = false)]
        pub usr_id: String,
        /// The chosen username; also the Argon2id salt that binds the keyset.
        #[sea_orm(unique)]
        pub username: String,
        /// A human-readable display name.
        pub display_name: String,
        /// The Ed25519 public key (32 bytes).
        pub ed25519_pk: Vec<u8>,
        /// The ML-DSA-65 public key (1952 bytes).
        pub ml_dsa_65_pk: Vec<u8>,
        /// When the account was created (epoch seconds).
        pub created_at: i64,
    }

    /// This entity declares no SeaORM relations; joins are done explicitly.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The `sessions` table — an issued bearer token.
pub mod session {
    use sea_orm::entity::prelude::*;

    /// A live session granting a user authenticated access.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "sessions")]
    pub struct Model {
        /// The opaque bearer token.
        #[sea_orm(primary_key, auto_increment = false)]
        pub token: String,
        /// The account this session authenticates.
        pub usr_id: String,
        /// When the session was issued (epoch seconds).
        pub created_at: i64,
        /// When the session expires (epoch seconds).
        pub expires_at: i64,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The `auth_challenges` table — a short-lived login nonce.
pub mod challenge {
    use sea_orm::entity::prelude::*;

    /// A pending login challenge: a nonce the client must sign.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "auth_challenges")]
    pub struct Model {
        /// The random nonce the client signs.
        #[sea_orm(primary_key, auto_increment = false)]
        pub nonce: String,
        /// The account the challenge was issued to.
        pub usr_id: String,
        /// When the challenge was issued (epoch seconds).
        pub created_at: i64,
        /// When the challenge expires (epoch seconds).
        pub expires_at: i64,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The `bundles` table — a stored, endorsed `.crisp` file.
pub mod bundle {
    use sea_orm::entity::prelude::*;

    /// One published bundle, stored verbatim as the endorsed file bytes.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "bundles")]
    pub struct Model {
        /// `SHA-256` of the payload, hex-encoded — the natural content key.
        #[sea_orm(primary_key, auto_increment = false)]
        pub content_hash: String,
        /// The bundle's document id (a recipe id or a book id).
        pub doc_id: String,
        /// The stored revision: a recipe revision id, or the content hash
        /// for a book (which has no single revision).
        pub revision: String,
        /// `"recipe"` or `"book"`.
        pub kind: String,
        /// The declared format version.
        pub format_version: i64,
        /// The endorsed `.crisp` file bytes.
        pub raw: Vec<u8>,
        /// The `usr…` id of the publishing account.
        pub published_by: String,
        /// When the bundle was published (epoch seconds).
        pub published_at: i64,
        /// When the server endorsement was signed (epoch seconds).
        pub endorsed_at: i64,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The `recipes_index` table — denormalized, searchable recipe metadata.
pub mod recipe_index {
    use sea_orm::entity::prelude::*;

    /// One indexed recipe within a published bundle. `search_vector` is a
    /// generated `tsvector` column maintained by PostgreSQL and is therefore
    /// absent from this model — it is queried, never written.
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "recipes_index")]
    pub struct Model {
        /// The surrogate primary key.
        #[sea_orm(primary_key)]
        pub id: i64,
        /// The content hash of the owning bundle.
        pub bundle_hash: String,
        /// The owning bundle's document id.
        pub doc_id: String,
        /// The recipe's own `rcp…` id.
        pub recipe_id: String,
        /// The recipe's `rev…` revision id.
        pub recipe_revision: String,
        /// The recipe title, resolved to the bundle default language.
        pub title: String,
        /// The recipe's identity language.
        pub language: String,
        /// An optional resolved summary.
        pub summary: Option<String>,
        /// The authors as a JSON array of `{ uid, name }` objects.
        #[sea_orm(column_type = "JsonBinary")]
        pub authors: Json,
        /// Keyword tags, as a JSON string array.
        #[sea_orm(column_type = "JsonBinary")]
        pub tags: Json,
        /// Cuisine tags, as a JSON string array.
        #[sea_orm(column_type = "JsonBinary")]
        pub cuisine: Json,
        /// Course classifications, as a JSON string array.
        #[sea_orm(column_type = "JsonBinary")]
        pub course: Json,
        /// Diet classifications, as a JSON string array.
        #[sea_orm(column_type = "JsonBinary")]
        pub diet: Json,
        /// Declared allergens, as a JSON string array.
        #[sea_orm(column_type = "JsonBinary")]
        pub allergens: Json,
        /// The bundle's SPDX license identifier.
        pub license: String,
        /// The recipe creation time (epoch seconds).
        pub created_at: i64,
        /// The recipe modification time (epoch seconds).
        pub modified_at: i64,
        /// The concatenated text fed to the full-text `tsvector`.
        pub search_text: String,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub use migration::Migrator;

/// The schema migration runner.
mod migration {
    use sea_orm_migration::prelude::*;
    use sea_orm_migration::sea_orm::ConnectionTrait;

    /// Applies and reverts the Crisp schema migrations.
    pub struct Migrator;

    #[async_trait::async_trait]
    impl MigratorTrait for Migrator {
        fn migrations() -> Vec<Box<dyn MigrationTrait>> {
            vec![Box::new(Init)]
        }
    }

    /// The single initial migration: it creates the whole schema.
    struct Init;

    impl MigrationName for Init {
        fn name(&self) -> &str {
            "m20260101_000001_init"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for Init {
        async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager
                .get_connection()
                .execute_unprepared(SCHEMA_UP)
                .await?;
            Ok(())
        }

        async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager
                .get_connection()
                .execute_unprepared(SCHEMA_DOWN)
                .await?;
            Ok(())
        }
    }

    /// The forward schema. `search_vector` is a stored generated column so
    /// PostgreSQL keeps the full-text index in lock-step with `search_text`.
    const SCHEMA_UP: &str = "
        CREATE TABLE accounts (
            usr_id        TEXT PRIMARY KEY,
            username      TEXT NOT NULL UNIQUE,
            display_name  TEXT NOT NULL,
            ed25519_pk    BYTEA NOT NULL,
            ml_dsa_65_pk  BYTEA NOT NULL,
            created_at    BIGINT NOT NULL
        );
        CREATE TABLE sessions (
            token       TEXT PRIMARY KEY,
            usr_id      TEXT NOT NULL REFERENCES accounts(usr_id) ON DELETE CASCADE,
            created_at  BIGINT NOT NULL,
            expires_at  BIGINT NOT NULL
        );
        CREATE INDEX sessions_usr_id_idx ON sessions (usr_id);
        CREATE TABLE auth_challenges (
            nonce       TEXT PRIMARY KEY,
            usr_id      TEXT NOT NULL REFERENCES accounts(usr_id) ON DELETE CASCADE,
            created_at  BIGINT NOT NULL,
            expires_at  BIGINT NOT NULL
        );
        CREATE TABLE bundles (
            content_hash    TEXT PRIMARY KEY,
            doc_id          TEXT NOT NULL,
            revision        TEXT NOT NULL,
            kind            TEXT NOT NULL,
            format_version  BIGINT NOT NULL,
            raw             BYTEA NOT NULL,
            published_by    TEXT NOT NULL REFERENCES accounts(usr_id),
            published_at    BIGINT NOT NULL,
            endorsed_at     BIGINT NOT NULL,
            UNIQUE (doc_id, revision)
        );
        CREATE INDEX bundles_doc_id_idx ON bundles (doc_id);
        CREATE INDEX bundles_doc_published_idx ON bundles (doc_id, published_at DESC);
        CREATE TABLE recipes_index (
            id               BIGSERIAL PRIMARY KEY,
            bundle_hash      TEXT NOT NULL REFERENCES bundles(content_hash) ON DELETE CASCADE,
            doc_id           TEXT NOT NULL,
            recipe_id        TEXT NOT NULL,
            recipe_revision  TEXT NOT NULL,
            title            TEXT NOT NULL,
            language         TEXT NOT NULL,
            summary          TEXT,
            authors          JSONB NOT NULL,
            tags             JSONB NOT NULL,
            cuisine          JSONB NOT NULL,
            course           JSONB NOT NULL,
            diet             JSONB NOT NULL,
            allergens        JSONB NOT NULL,
            license          TEXT NOT NULL,
            created_at       BIGINT NOT NULL,
            modified_at      BIGINT NOT NULL,
            search_text      TEXT NOT NULL,
            search_vector    TSVECTOR GENERATED ALWAYS AS
                                 (to_tsvector('simple', search_text)) STORED
        );
        CREATE INDEX recipes_index_fts_idx ON recipes_index USING GIN (search_vector);
        CREATE INDEX recipes_index_doc_idx ON recipes_index (doc_id);
        CREATE INDEX recipes_index_recipe_idx ON recipes_index (recipe_id, recipe_revision);
    ";

    /// The reverse schema, dropping tables in foreign-key-safe order.
    const SCHEMA_DOWN: &str = "
        DROP TABLE IF EXISTS recipes_index;
        DROP TABLE IF EXISTS bundles;
        DROP TABLE IF EXISTS auth_challenges;
        DROP TABLE IF EXISTS sessions;
        DROP TABLE IF EXISTS accounts;
    ";
}
