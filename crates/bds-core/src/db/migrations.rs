use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use crate::db::DbConnection;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Run all embedded migrations against the given connection.
pub fn run_migrations(conn: &DbConnection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    conn.with_migrations(|conn| conn.run_pending_migrations(MIGRATIONS).map(|_| ()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::schema::{
        ai_catalog_meta, ai_model_modalities, ai_models, ai_providers, chat_conversations,
        chat_messages, db_notifications, dismissed_duplicate_pairs, embedding_keys,
        generated_file_hashes, import_definitions, mcp_proposals, media, media_translations,
        post_links, post_media, post_translations, posts, projects, scripts, settings, tags,
        templates,
    };
    use diesel::prelude::*;
    use diesel_migrations::MigrationHarness;

    fn migrated_database() -> Database {
        let db = Database::open_in_memory().unwrap();
        run_migrations(db.conn()).unwrap();
        db
    }

    #[test]
    fn migrations_create_schema_and_track_version() {
        let db = migrated_database();
        let applied = db
            .conn()
            .with_migrations(|conn| conn.applied_migrations().unwrap().len());
        assert_eq!(applied, 6);
    }

    #[test]
    fn migrations_create_every_persisted_table() {
        let db = migrated_database();
        macro_rules! assert_empty {
            ($($table:expr),+ $(,)?) => {
                $(
                    let count = db
                        .conn()
                        .with(|conn| ($table).count().get_result::<i64>(conn))
                        .unwrap();
                    assert_eq!(count, 0, "{} should start empty", stringify!($table));
                )+
            };
        }

        assert_empty!(
            projects::table,
            posts::table,
            post_translations::table,
            media::table,
            media_translations::table,
            tags::table,
            templates::table,
            scripts::table,
            post_links::table,
            post_media::table,
            settings::table,
            generated_file_hashes::table,
            chat_conversations::table,
            chat_messages::table,
            ai_providers::table,
            ai_models::table,
            ai_model_modalities::table,
            ai_catalog_meta::table,
            embedding_keys::table,
            dismissed_duplicate_pairs::table,
            import_definitions::table,
            mcp_proposals::table,
            db_notifications::table,
        );
    }

    #[test]
    fn migrations_expose_current_ai_catalog_and_usage_columns() {
        let db = migrated_database();
        let (provider_ref, model_ref, usage_tokens) = db
            .conn()
            .with(|conn| {
                diesel::insert_into(ai_providers::table)
                    .values((
                        ai_providers::id.eq("openai"),
                        ai_providers::name.eq("OpenAI"),
                        ai_providers::package_ref.eq(Some("provider-package")),
                        ai_providers::updated_at.eq(1_i64),
                    ))
                    .execute(conn)?;
                diesel::insert_into(ai_models::table)
                    .values((
                        ai_models::provider.eq("openai"),
                        ai_models::model_id.eq("model"),
                        ai_models::name.eq("Model"),
                        ai_models::attachment.eq(0),
                        ai_models::reasoning.eq(0),
                        ai_models::tool_call.eq(0),
                        ai_models::structured_output.eq(0),
                        ai_models::temperature.eq(1),
                        ai_models::open_weights.eq(0),
                        ai_models::context_window.eq(0),
                        ai_models::max_input_tokens.eq(0),
                        ai_models::max_output_tokens.eq(0),
                        ai_models::provider_package_ref.eq(Some("model-package")),
                        ai_models::updated_at.eq(1_i64),
                    ))
                    .execute(conn)?;
                diesel::insert_into(chat_conversations::table)
                    .values((
                        chat_conversations::id.eq("conversation"),
                        chat_conversations::title.eq("Test"),
                        chat_conversations::created_at.eq(1_i64),
                        chat_conversations::updated_at.eq(1_i64),
                    ))
                    .execute(conn)?;
                diesel::insert_into(chat_messages::table)
                    .values((
                        chat_messages::conversation_id.eq("conversation"),
                        chat_messages::role.eq("assistant"),
                        chat_messages::created_at.eq(1_i64),
                        chat_messages::token_usage_input.eq(Some(56)),
                        chat_messages::token_usage_output.eq(Some(78)),
                        chat_messages::cache_read_tokens.eq(Some(12)),
                        chat_messages::cache_write_tokens.eq(Some(34)),
                    ))
                    .execute(conn)?;

                Ok((
                    ai_providers::table
                        .select(ai_providers::package_ref)
                        .first::<Option<String>>(conn)?,
                    ai_models::table
                        .select(ai_models::provider_package_ref)
                        .first::<Option<String>>(conn)?,
                    chat_messages::table
                        .select((
                            chat_messages::token_usage_input,
                            chat_messages::token_usage_output,
                            chat_messages::cache_read_tokens,
                            chat_messages::cache_write_tokens,
                        ))
                        .first::<(Option<i32>, Option<i32>, Option<i32>, Option<i32>)>(conn)?,
                ))
            })
            .unwrap();

        assert_eq!(provider_ref.as_deref(), Some("provider-package"));
        assert_eq!(model_ref.as_deref(), Some("model-package"));
        assert_eq!(usage_tokens, (Some(56), Some(78), Some(12), Some(34)));
    }

    #[test]
    fn existing_chat_schema_is_upgraded_with_cache_token_columns() {
        let db = Database::open_in_memory().unwrap();
        db.conn().with_migrations(|conn| {
            for _ in 0..4 {
                conn.run_next_migration(MIGRATIONS).unwrap();
            }
        });
        db.conn()
            .with(|conn| {
                diesel::insert_into(chat_conversations::table)
                    .values((
                        chat_conversations::id.eq("existing"),
                        chat_conversations::title.eq("Existing chat"),
                        chat_conversations::created_at.eq(1_i64),
                        chat_conversations::updated_at.eq(1_i64),
                    ))
                    .execute(conn)?;
                diesel::insert_into(chat_messages::table)
                    .values((
                        chat_messages::conversation_id.eq("existing"),
                        chat_messages::role.eq("assistant"),
                        chat_messages::created_at.eq(1_i64),
                        chat_messages::token_usage_input.eq(Some(8)),
                        chat_messages::token_usage_output.eq(Some(5)),
                    ))
                    .execute(conn)
            })
            .unwrap();

        run_migrations(db.conn()).unwrap();

        let usage = db
            .conn()
            .with(|conn| {
                chat_messages::table
                    .select((
                        chat_messages::token_usage_input,
                        chat_messages::token_usage_output,
                        chat_messages::cache_read_tokens,
                        chat_messages::cache_write_tokens,
                    ))
                    .first::<(Option<i32>, Option<i32>, Option<i32>, Option<i32>)>(conn)
            })
            .unwrap();
        assert_eq!(usage, (Some(8), Some(5), None, None));
    }

    #[test]
    fn existing_conversations_are_preserved_when_surface_state_is_added() {
        let db = Database::open_in_memory().unwrap();
        db.conn().with_migrations(|conn| {
            for _ in 0..5 {
                conn.run_next_migration(MIGRATIONS).unwrap();
            }
        });
        db.conn()
            .with(|conn| {
                diesel::insert_into(chat_conversations::table)
                    .values((
                        chat_conversations::id.eq("existing-surface-chat"),
                        chat_conversations::title.eq("Keep me"),
                        chat_conversations::created_at.eq(1_i64),
                        chat_conversations::updated_at.eq(1_i64),
                    ))
                    .execute(conn)
            })
            .unwrap();

        run_migrations(db.conn()).unwrap();

        let (title, state) = db
            .conn()
            .with(|conn| {
                chat_conversations::table
                    .filter(chat_conversations::id.eq("existing-surface-chat"))
                    .select((chat_conversations::title, chat_conversations::surface_state))
                    .first::<(String, Option<String>)>(conn)
            })
            .unwrap();
        assert_eq!(title, "Keep me");
        assert_eq!(state, None);
    }
}
