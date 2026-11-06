//! The moderation tables (`docs/public-server-policy.md` §4) and the
//! migration that creates them.
//!
//! - `reports` — a report against a document, filed by an account. The
//!   reporter is the only column tied to `accounts`, and it becomes `NULL`
//!   when the reporter's account is deleted, so the report outlives it.
//! - `takedowns` — the tombstone an administrator's takedown leaves.
//! - `suspensions` — one row per suspended `usr…` id. Not a foreign key, so
//!   it outlives the account and keeps refusing the key at registration.
//! - `moderation_actions` — the append-only audit trail. Ids are plain text,
//!   never foreign keys, and a trigger refuses every `UPDATE` and `DELETE`.

/// The `reports` table — a report against a document or one revision.
pub mod report {
    use sea_orm::entity::prelude::*;

    /// One filed report.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "reports")]
    pub struct Model {
        /// The surrogate primary key.
        #[sea_orm(primary_key)]
        pub id: i64,
        /// The reporting account's `usr…` id; `None` once that account is
        /// deleted.
        pub reporter: Option<String>,
        /// The reported document id.
        pub doc_id: String,
        /// The reported revision, when the report names one.
        pub revision: Option<String>,
        /// The category: `copyright`, `illegal`, `spam`, `harassment`, or
        /// `other`.
        pub category: String,
        /// The reporter's explanation, at most 2000 characters.
        pub detail: String,
        /// `open`, `dismissed`, or `actioned`.
        pub status: String,
        /// When the report was filed (epoch seconds).
        pub created_at: i64,
        /// When an administrator resolved it (epoch seconds).
        pub resolved_at: Option<i64>,
        /// The `usr…` id of the administrator who resolved it.
        pub resolved_by: Option<String>,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The `takedowns` table — the tombstone of a takedown.
pub mod takedown {
    use sea_orm::entity::prelude::*;

    /// A document, or one revision of it, an administrator took down.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "takedowns")]
    pub struct Model {
        /// The surrogate primary key.
        #[sea_orm(primary_key)]
        pub id: i64,
        /// The removed document id.
        pub doc_id: String,
        /// The removed revision; `None` when the whole document was removed.
        pub revision: Option<String>,
        /// The `content_hash` of every bundle the takedown deleted, as a
        /// JSON string array.
        #[sea_orm(column_type = "JsonBinary")]
        pub content_hashes: Json,
        /// The reason category, one of the report categories.
        pub reason: String,
        /// The report that prompted the takedown, if any.
        pub report_id: Option<i64>,
        /// When the takedown happened (epoch seconds).
        pub created_at: i64,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The `suspensions` table — a suspended account key.
pub mod suspension {
    use sea_orm::entity::prelude::*;

    /// A suspension of one `usr…` id.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "suspensions")]
    pub struct Model {
        /// The suspended `usr…` id (plain text, not a foreign key).
        #[sea_orm(primary_key, auto_increment = false)]
        pub usr_id: String,
        /// The reason category.
        pub reason: String,
        /// The `usr…` id of the suspending administrator.
        pub actor: String,
        /// When the account was suspended (epoch seconds).
        pub created_at: i64,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The `moderation_actions` table — the append-only audit trail.
pub mod moderation_action {
    use sea_orm::entity::prelude::*;

    /// One administrator action.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "moderation_actions")]
    pub struct Model {
        /// The surrogate primary key, in the order actions were recorded.
        #[sea_orm(primary_key)]
        pub id: i64,
        /// The acting administrator's `usr…` id.
        pub actor: String,
        /// What was done: `takedown`, `lift_takedown`, `suspend`,
        /// `unsuspend`, or `resolve_report`.
        pub action: String,
        /// What it was done to: a document id, `doc_id/revision`, a `usr…`
        /// id, or `report/{id}`.
        pub target: String,
        /// The reason category, or a report's resolution.
        pub reason: Option<String>,
        /// The report the action answers, if any.
        pub report_id: Option<i64>,
        /// When the action was taken (epoch seconds).
        pub created_at: i64,
    }

    /// This entity declares no SeaORM relations.
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub(super) use migration::Moderation;

/// The migration creating the moderation tables.
mod migration {
    use sea_orm_migration::prelude::*;
    use sea_orm_migration::sea_orm::ConnectionTrait;

    /// Creates `reports`, `takedowns`, `suspensions`, and the append-only
    /// `moderation_actions`.
    pub struct Moderation;

    impl MigrationName for Moderation {
        fn name(&self) -> &str {
            "m20261104_000004_moderation"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for Moderation {
        async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager.get_connection().execute_unprepared(UP).await?;
            Ok(())
        }

        async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager.get_connection().execute_unprepared(DOWN).await?;
            Ok(())
        }
    }

    /// The forward moderation migration.
    const UP: &str = "
        CREATE TABLE reports (
            id           BIGSERIAL PRIMARY KEY,
            reporter     TEXT REFERENCES accounts(usr_id) ON DELETE SET NULL,
            doc_id       TEXT NOT NULL,
            revision     TEXT,
            category     TEXT NOT NULL,
            detail       TEXT NOT NULL,
            status       TEXT NOT NULL,
            created_at   BIGINT NOT NULL,
            resolved_at  BIGINT,
            resolved_by  TEXT
        );
        CREATE INDEX reports_status_idx ON reports (status, id);
        CREATE INDEX reports_open_reporter_idx ON reports (reporter) WHERE status = 'open';
        CREATE TABLE takedowns (
            id              BIGSERIAL PRIMARY KEY,
            doc_id          TEXT NOT NULL,
            revision        TEXT,
            content_hashes  JSONB NOT NULL,
            reason          TEXT NOT NULL,
            report_id       BIGINT,
            created_at      BIGINT NOT NULL
        );
        CREATE INDEX takedowns_doc_id_idx ON takedowns (doc_id);
        CREATE TABLE suspensions (
            usr_id      TEXT PRIMARY KEY,
            reason      TEXT NOT NULL,
            actor       TEXT NOT NULL,
            created_at  BIGINT NOT NULL
        );
        CREATE TABLE moderation_actions (
            id          BIGSERIAL PRIMARY KEY,
            actor       TEXT NOT NULL,
            action      TEXT NOT NULL,
            target      TEXT NOT NULL,
            reason      TEXT,
            report_id   BIGINT,
            created_at  BIGINT NOT NULL
        );
        CREATE FUNCTION moderation_actions_append_only() RETURNS trigger
            LANGUAGE plpgsql AS $$
            BEGIN
                RAISE EXCEPTION 'moderation_actions is append-only';
            END
            $$;
        CREATE TRIGGER moderation_actions_append_only
            BEFORE UPDATE OR DELETE ON moderation_actions
            FOR EACH ROW EXECUTE FUNCTION moderation_actions_append_only();
    ";

    /// The reverse moderation migration.
    const DOWN: &str = "
        DROP TABLE IF EXISTS moderation_actions;
        DROP FUNCTION IF EXISTS moderation_actions_append_only();
        DROP TABLE IF EXISTS suspensions;
        DROP TABLE IF EXISTS takedowns;
        DROP TABLE IF EXISTS reports;
    ";
}
