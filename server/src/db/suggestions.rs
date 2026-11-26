//! The `ingredient_suggestions` table (`docs/ingredient-index.md` §6) and
//! the migration that creates it.
//!
//! A suggestion proposes a change to the ingredient index: a missing entry,
//! a name or alias in a locale shard, a regional spelling, a merge, or
//! something else. Administrators triage it to `accepted`, `declined`, or
//! `duplicate`; accepted ones are exported as override patches for the
//! repository, never applied to this server's index. The suggester is the
//! only column tied to `accounts`, and it becomes `NULL` when that account is
//! deleted, so the suggestion and its triage outlive it.

/// The `ingredient_suggestions` table — one proposed index change.
pub mod ingredient_suggestion {
    use sea_orm::entity::prelude::*;

    /// One filed suggestion.
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "ingredient_suggestions")]
    pub struct Model {
        /// The surrogate primary key.
        #[sea_orm(primary_key)]
        pub id: i64,
        /// The suggesting account's `usr…` id; `None` once that account is
        /// deleted.
        pub suggester: Option<String>,
        /// `new_entry`, `name`, `regional_spelling`, `merge`, or `other`.
        pub kind: String,
        /// The index entry the suggestion is about, when it names one.
        pub slug: Option<String>,
        /// The locale shard the suggestion is for, e.g. `en-GB`.
        pub locale: String,
        /// The proposed text: a name, an alias, a spelling, the successor
        /// slug of a merge, or a description.
        pub text: String,
        /// The suggester's explanation (possibly empty).
        pub note: String,
        /// `open`, `accepted`, `declined`, or `duplicate`.
        pub status: String,
        /// When it was filed (epoch seconds).
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

pub(super) use migration::IngredientSuggestions;

/// The migration creating `ingredient_suggestions`.
mod migration {
    use sea_orm_migration::prelude::*;
    use sea_orm_migration::sea_orm::ConnectionTrait;

    /// Creates `ingredient_suggestions` and the indexes its queue, its
    /// per-account cap, and its export read.
    pub struct IngredientSuggestions;

    impl MigrationName for IngredientSuggestions {
        fn name(&self) -> &str {
            "m20261125_000007_ingredient_suggestions"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for IngredientSuggestions {
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
        CREATE TABLE ingredient_suggestions (
            id           BIGSERIAL PRIMARY KEY,
            suggester    TEXT REFERENCES accounts(usr_id) ON DELETE SET NULL,
            kind         TEXT NOT NULL CHECK (kind IN
                             ('new_entry', 'name', 'regional_spelling', 'merge', 'other')),
            slug         TEXT,
            locale       TEXT NOT NULL,
            text         TEXT NOT NULL,
            note         TEXT NOT NULL,
            status       TEXT NOT NULL CHECK (status IN
                             ('open', 'accepted', 'declined', 'duplicate')),
            created_at   BIGINT NOT NULL,
            resolved_at  BIGINT,
            resolved_by  TEXT
        );
        CREATE INDEX ingredient_suggestions_status_idx
            ON ingredient_suggestions (status, id);
        CREATE INDEX ingredient_suggestions_open_suggester_idx
            ON ingredient_suggestions (suggester) WHERE status = 'open';
        CREATE INDEX ingredient_suggestions_accepted_idx
            ON ingredient_suggestions (resolved_at, id) WHERE status = 'accepted';
    ";

    /// The reverse migration.
    const DOWN: &str = "
        DROP TABLE IF EXISTS ingredient_suggestions;
    ";
}
