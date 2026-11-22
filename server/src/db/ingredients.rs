//! The migration adding the ingredient facet to `recipes_index`.
//!
//! `ingredient_codes` holds each recipe's distinct ingredient-index slugs
//! (spec §13) as a JSONB string array, as written in the bundle. The search
//! facet expands a requested slug through the hierarchy at query time
//! ([`crate::search`]), so an index upgrade that adds or merges entries needs
//! no reindex. A GIN index serves its `?|` match.
//!
//! Rows indexed before this migration get an empty array: the slugs live only
//! in the stored CBOR, which SQL cannot read, so such a recipe joins the facet
//! when it is next published.

pub(super) use migration::IngredientCodes;

/// The migration adding `recipes_index.ingredient_codes`.
mod migration {
    use sea_orm_migration::prelude::*;
    use sea_orm_migration::sea_orm::ConnectionTrait;

    /// Adds `recipes_index.ingredient_codes` and its GIN index.
    pub struct IngredientCodes;

    impl MigrationName for IngredientCodes {
        fn name(&self) -> &str {
            "m20261121_000006_ingredient_codes"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for IngredientCodes {
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
        ALTER TABLE recipes_index
            ADD COLUMN ingredient_codes JSONB NOT NULL DEFAULT '[]'::jsonb;
        CREATE INDEX recipes_index_ingredient_codes_idx
            ON recipes_index USING GIN (ingredient_codes);
    ";

    /// The reverse migration.
    const DOWN: &str = "
        DROP INDEX IF EXISTS recipes_index_ingredient_codes_idx;
        ALTER TABLE recipes_index DROP COLUMN IF EXISTS ingredient_codes;
    ";
}
