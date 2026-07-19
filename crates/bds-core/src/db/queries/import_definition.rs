use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::import_definitions;
use crate::model::ImportDefinition;

pub fn insert_import_definition(
    conn: &DbConnection,
    definition: &ImportDefinition,
) -> QueryResult<()> {
    conn.with(|connection| {
        diesel::insert_into(import_definitions::table)
            .values(definition.clone())
            .execute(connection)
            .map(|_| ())
    })
}

pub fn get_import_definition(conn: &DbConnection, id: &str) -> QueryResult<ImportDefinition> {
    conn.with(|connection| {
        import_definitions::table
            .filter(import_definitions::id.eq(id))
            .select(ImportDefinition::as_select())
            .first(connection)
    })
}

pub fn list_import_definitions(
    conn: &DbConnection,
    project_id: &str,
) -> QueryResult<Vec<ImportDefinition>> {
    conn.with(|connection| {
        import_definitions::table
            .filter(import_definitions::project_id.eq(project_id))
            .order((
                import_definitions::updated_at.desc(),
                import_definitions::created_at.desc(),
            ))
            .select(ImportDefinition::as_select())
            .load(connection)
    })
}

pub fn update_import_definition(
    conn: &DbConnection,
    definition: &ImportDefinition,
) -> QueryResult<()> {
    conn.with(|connection| {
        diesel::update(import_definitions::table.filter(import_definitions::id.eq(&definition.id)))
            .set(definition.clone())
            .execute(connection)
            .map(|_| ())
    })
}

pub fn delete_import_definition(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|connection| {
        diesel::delete(import_definitions::table.filter(import_definitions::id.eq(id)))
            .execute(connection)
            .map(|_| ())
    })
}
