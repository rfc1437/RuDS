use rusqlite::{Connection, params};

use crate::db::from_row::{MEDIA_COLUMNS, media_from_row};
use crate::model::Media;
use crate::util::calendar_range_unix_ms;

fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".into())
}

pub fn insert_media(conn: &Connection, m: &Media) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO media (
            id, project_id, filename, original_name, mime_type, size,
            width, height, title, alt, caption, author, language,
            file_path, sidecar_path, checksum, tags, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19
         )",
        params![
            m.id,
            m.project_id,
            m.filename,
            m.original_name,
            m.mime_type,
            m.size,
            m.width,
            m.height,
            m.title,
            m.alt,
            m.caption,
            m.author,
            m.language,
            m.file_path,
            m.sidecar_path,
            m.checksum,
            tags_to_json(&m.tags),
            m.created_at,
            m.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_media_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Media> {
    conn.query_row(
        &format!("SELECT {MEDIA_COLUMNS} FROM media WHERE id = ?1"),
        params![id],
        media_from_row,
    )
}

pub fn list_media_by_project(conn: &Connection, project_id: &str) -> rusqlite::Result<Vec<Media>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLUMNS} FROM media WHERE project_id = ?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt.query_map(params![project_id], media_from_row)?;
    rows.collect()
}

pub fn list_media_by_project_limited(
    conn: &Connection,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<Media>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLUMNS} FROM media WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
    ))?;
    let rows = stmt.query_map(params![project_id, limit, offset], media_from_row)?;
    rows.collect()
}

pub fn count_media_by_project(conn: &Connection, project_id: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media WHERE project_id = ?1",
        params![project_id],
        |row| row.get(0),
    )
}

pub fn update_media(conn: &Connection, m: &Media) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE media SET
            filename = ?1, original_name = ?2, mime_type = ?3, size = ?4,
            width = ?5, height = ?6, title = ?7, alt = ?8, caption = ?9,
            author = ?10, language = ?11, file_path = ?12, sidecar_path = ?13,
            checksum = ?14, tags = ?15, updated_at = ?16
         WHERE id = ?17",
        params![
            m.filename,
            m.original_name,
            m.mime_type,
            m.size,
            m.width,
            m.height,
            m.title,
            m.alt,
            m.caption,
            m.author,
            m.language,
            m.file_path,
            m.sidecar_path,
            m.checksum,
            tags_to_json(&m.tags),
            m.updated_at,
            m.id,
        ],
    )?;
    Ok(())
}

pub fn delete_media(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM media WHERE id = ?1", params![id])?;
    Ok(())
}

// ── Filtered queries (per sidebar_views.allium MediaView) ───

/// Parameters for filtered media listing.
#[derive(Debug, Clone, Default)]
pub struct MediaFilterParams {
    /// FTS search query (empty = no search filter).
    pub search_query: String,
    /// Year filter from calendar archive.
    pub year: Option<i32>,
    /// Month filter (1-12) from calendar archive.
    pub month: Option<u32>,
    /// Tag filter (media must have ALL of these tags).
    pub tags: Vec<String>,
}

impl MediaFilterParams {
    pub fn has_active_filters(&self) -> bool {
        !self.search_query.is_empty() || self.year.is_some() || !self.tags.is_empty()
    }
}

/// List media with optional filters applied.
pub fn list_media_filtered(
    conn: &Connection,
    project_id: &str,
    filters: &MediaFilterParams,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<Media>> {
    let mut conditions = vec!["project_id = ?1".to_string()];
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(project_id.to_string()));

    if !filters.search_query.is_empty() {
        let idx = param_values.len() + 1;
        let pattern = format!("%{}%", filters.search_query.replace('%', "\\%"));
        conditions.push(format!(
            "(COALESCE(title, original_name) LIKE ?{idx} ESCAPE '\\')"
        ));
        param_values.push(Box::new(pattern));
    }

    if let Some(year) = filters.year {
        let (start, end) =
            calendar_range_unix_ms(year, filters.month).ok_or(rusqlite::Error::InvalidQuery)?;
        let idx1 = param_values.len() + 1;
        let idx2 = param_values.len() + 2;
        conditions.push(format!("(created_at >= ?{idx1} AND created_at < ?{idx2})"));
        param_values.push(Box::new(start));
        param_values.push(Box::new(end));
    }

    for tag in &filters.tags {
        let idx = param_values.len() + 1;
        let pattern = format!("%\"{}\"%", tag.replace('"', "\\\""));
        conditions.push(format!("(tags LIKE ?{idx})"));
        param_values.push(Box::new(pattern));
    }

    let where_clause = conditions.join(" AND ");
    let idx_limit = param_values.len() + 1;
    let idx_offset = param_values.len() + 2;
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let sql = format!(
        "SELECT {MEDIA_COLUMNS} FROM media WHERE {where_clause} ORDER BY created_at DESC LIMIT ?{idx_limit} OFFSET ?{idx_offset}"
    );

    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|b| b.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), media_from_row)?;
    rows.collect()
}

/// Year/month counts for the media calendar archive widget.
pub fn media_calendar_counts(
    conn: &Connection,
    project_id: &str,
) -> rusqlite::Result<Vec<(i32, u32, usize)>> {
    let mut stmt = conn.prepare(
        "SELECT
            CAST(strftime('%Y', created_at / 1000, 'unixepoch') AS INTEGER) AS y,
            CAST(strftime('%m', created_at / 1000, 'unixepoch') AS INTEGER) AS m,
            COUNT(*) AS cnt
         FROM media
         WHERE project_id = ?1
         GROUP BY y, m
         ORDER BY y DESC, m DESC",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, i32>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, usize>(2)?,
        ))
    })?;
    rows.collect()
}

/// Collect all distinct tag values across media for a project.
pub fn distinct_media_tags(conn: &Connection, project_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT tags FROM media WHERE project_id = ?1 AND tags != '[]'")?;
    let rows = stmt.query_map(params![project_id], |row| row.get::<_, String>(0))?;
    let mut all_tags = std::collections::BTreeSet::new();
    for json_str in rows {
        if let Ok(json_str) = json_str
            && let Ok(tags) = serde_json::from_str::<Vec<String>>(&json_str)
        {
            all_tags.extend(tags);
        }
    }
    Ok(all_tags.into_iter().collect())
}

/// Test helper: create a minimal Media value (available to sibling test modules).
#[cfg(test)]
pub fn make_test_media(id: &str, project_id: &str) -> Media {
    Media {
        id: id.to_string(),
        project_id: project_id.to_string(),
        filename: format!("{id}.jpg"),
        original_name: "photo.jpg".to_string(),
        mime_type: "image/jpeg".to_string(),
        size: 50000,
        width: Some(1920),
        height: Some(1080),
        title: None,
        alt: None,
        caption: None,
        author: None,
        language: None,
        file_path: format!("/media/{id}.jpg"),
        sidecar_path: format!("/media/{id}.jpg.meta"),
        checksum: None,
        tags: vec![],
        created_at: 1000,
        updated_at: 2000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db
    }

    fn make_media_item(id: &str) -> Media {
        Media {
            id: id.to_string(),
            project_id: "p1".to_string(),
            filename: "abc.jpg".to_string(),
            original_name: "photo.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size: 50000,
            width: Some(1920),
            height: Some(1080),
            title: Some("Sunset".into()),
            alt: Some("A sunset".into()),
            caption: None,
            author: Some("Bob".into()),
            language: Some("en".into()),
            file_path: "/media/abc.jpg".to_string(),
            sidecar_path: "/media/abc.jpg.meta".to_string(),
            checksum: Some("hash1".into()),
            tags: vec!["nature".into()],
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        let m = make_media_item("m1");
        insert_media(db.conn(), &m).unwrap();
        let fetched = get_media_by_id(db.conn(), "m1").unwrap();
        assert_eq!(fetched.original_name, "photo.jpg");
        assert_eq!(fetched.width, Some(1920));
        assert_eq!(fetched.height, Some(1080));
        assert_eq!(fetched.tags, vec!["nature"]);
        assert_eq!(fetched.size, 50000);
    }

    #[test]
    fn list_by_project() {
        let db = setup();
        let mut m1 = make_media_item("m1");
        m1.created_at = 1000;
        let mut m2 = make_media_item("m2");
        m2.filename = "def.jpg".into();
        m2.created_at = 2000;
        insert_media(db.conn(), &m1).unwrap();
        insert_media(db.conn(), &m2).unwrap();
        let list = list_media_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "m2");
    }

    #[test]
    fn update_media_fields() {
        let db = setup();
        let mut m = make_media_item("m1");
        insert_media(db.conn(), &m).unwrap();
        m.title = Some("New Title".into());
        m.tags = vec!["updated".into()];
        m.updated_at = 9999;
        update_media(db.conn(), &m).unwrap();
        let fetched = get_media_by_id(db.conn(), "m1").unwrap();
        assert_eq!(fetched.title.as_deref(), Some("New Title"));
        assert_eq!(fetched.tags, vec!["updated"]);
    }

    #[test]
    fn delete_removes_media() {
        let db = setup();
        insert_media(db.conn(), &make_media_item("m1")).unwrap();
        delete_media(db.conn(), "m1").unwrap();
        assert!(get_media_by_id(db.conn(), "m1").is_err());
    }
}
