use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;

use crate::db::queries::media as qm;
use crate::db::queries::post as qp;
use crate::db::queries::post_media as qpm;
use crate::engine::EngineResult;
use crate::model::{Media, Post, PostMedia};
use crate::util::sidecar::MediaSidecar;
use crate::util::{atomic_write_str, now_unix_ms};

/// Link a media item to a post and sync the media sidecar.
pub fn link_media_to_post(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    post_id: &str,
    media_id: &str,
    sort_order: i32,
) -> EngineResult<PostMedia> {
    let id = Uuid::new_v4().to_string();
    let now = now_unix_ms();
    let pm = PostMedia {
        id,
        project_id: project_id.to_string(),
        post_id: post_id.to_string(),
        media_id: media_id.to_string(),
        sort_order,
        created_at: now,
    };
    qpm::link_media(conn, &pm)?;
    sync_sidecar_linked_post_ids(conn, data_dir, media_id)?;
    Ok(pm)
}

/// Unlink a media item from a post and sync the media sidecar.
pub fn unlink_media_from_post(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    media_id: &str,
) -> EngineResult<()> {
    qpm::unlink_media(conn, post_id, media_id)?;
    sync_sidecar_linked_post_ids(conn, data_dir, media_id)?;
    Ok(())
}

/// Reorder media items for a post. `media_ids` contains the new ordering;
/// each entry gets sort_order = its index.
pub fn reorder_post_media(
    conn: &Connection,
    post_id: &str,
    media_ids: &[String],
) -> EngineResult<()> {
    for (i, media_id) in media_ids.iter().enumerate() {
        qpm::update_sort_order(conn, post_id, media_id, i as i32)?;
    }
    Ok(())
}

/// List media items currently linked to a post.
pub fn list_media_for_post(conn: &Connection, post_id: &str) -> EngineResult<Vec<Media>> {
    let links = qpm::list_post_media_by_post(conn, post_id)?;
    let mut media = Vec::with_capacity(links.len());
    for link in links {
        if let Ok(item) = qm::get_media_by_id(conn, &link.media_id) {
            media.push(item);
        }
    }
    Ok(media)
}

/// List posts currently linked to a media item.
pub fn list_posts_for_media(conn: &Connection, media_id: &str) -> EngineResult<Vec<Post>> {
    let links = qpm::list_post_media_by_media(conn, media_id)?;
    let mut posts = Vec::with_capacity(links.len());
    for link in links {
        if let Ok(post) = qp::get_post_by_id(conn, &link.post_id) {
            posts.push(post);
        }
    }
    Ok(posts)
}

/// Rebuild the media sidecar file so that `linkedPostIds` reflects the current
/// set of posts linked to this media item.
fn sync_sidecar_linked_post_ids(
    conn: &Connection,
    data_dir: &Path,
    media_id: &str,
) -> EngineResult<()> {
    let links = qpm::list_post_media_by_media(conn, media_id)?;
    let post_ids: Vec<String> = links.iter().map(|pm| pm.post_id.clone()).collect();
    let media = qm::get_media_by_id(conn, media_id)?;
    let sidecar = MediaSidecar::from_media(&media, &post_ids);
    let abs_path = data_dir.join(&media.sidecar_path);
    atomic_write_str(&abs_path, &sidecar.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    use crate::db::Database;
    use crate::db::queries::media::{insert_media, make_test_media};
    use crate::db::queries::post_media::list_post_media_by_post;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::util::sidecar::read_sidecar;

    fn setup() -> (Database, TempDir) {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        // Create a post
        db.conn()
            .execute(
                "INSERT INTO posts (id, project_id, title, slug, status, file_path, created_at, updated_at) VALUES ('post1', 'p1', 'Test', 'test', 'draft', '', 1000, 1000)",
                [],
            )
            .unwrap();
        (db, dir)
    }

    /// Insert a media item whose sidecar_path is inside the temp dir.
    fn insert_test_media(db: &Database, dir: &Path, id: &str) {
        let sidecar_rel = format!("media/{id}.jpg.meta");
        let mut media = make_test_media(id, "p1");
        media.file_path = format!("media/{id}.jpg");
        media.sidecar_path = sidecar_rel.clone();
        insert_media(db.conn(), &media).unwrap();

        // Write an initial sidecar so the directory exists
        let initial = MediaSidecar::from_media(&media, &[]);
        let abs_sidecar = dir.join(&sidecar_rel);
        fs::create_dir_all(abs_sidecar.parent().unwrap()).unwrap();
        fs::write(&abs_sidecar, initial.to_string()).unwrap();
    }

    fn read_linked_ids(dir: &Path, sidecar_rel: &str) -> Vec<String> {
        let content = fs::read_to_string(dir.join(sidecar_rel)).unwrap();
        let sc = read_sidecar(&content).unwrap();
        sc.linked_post_ids
    }

    #[test]
    fn link_and_verify_sidecar() {
        let (db, dir) = setup();
        insert_test_media(&db, dir.path(), "m1");

        link_media_to_post(db.conn(), dir.path(), "p1", "post1", "m1", 0).unwrap();

        let ids = read_linked_ids(dir.path(), "media/m1.jpg.meta");
        assert_eq!(ids, vec!["post1"]);
    }

    #[test]
    fn unlink_removes_from_sidecar() {
        let (db, dir) = setup();
        insert_test_media(&db, dir.path(), "m1");

        link_media_to_post(db.conn(), dir.path(), "p1", "post1", "m1", 0).unwrap();
        unlink_media_from_post(db.conn(), dir.path(), "post1", "m1").unwrap();

        let ids = read_linked_ids(dir.path(), "media/m1.jpg.meta");
        assert!(ids.is_empty());
    }

    #[test]
    fn reorder_updates_sort_order() {
        let (db, dir) = setup();
        insert_test_media(&db, dir.path(), "m1");
        insert_test_media(&db, dir.path(), "m2");

        link_media_to_post(db.conn(), dir.path(), "p1", "post1", "m1", 0).unwrap();
        link_media_to_post(db.conn(), dir.path(), "p1", "post1", "m2", 1).unwrap();

        // Reverse the order: m2 first, m1 second
        reorder_post_media(db.conn(), "post1", &["m2".to_string(), "m1".to_string()]).unwrap();

        let list = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].media_id, "m2");
        assert_eq!(list[0].sort_order, 0);
        assert_eq!(list[1].media_id, "m1");
        assert_eq!(list[1].sort_order, 1);
    }

    #[test]
    fn multiple_posts_linked() {
        let (db, dir) = setup();
        insert_test_media(&db, dir.path(), "m1");

        // Create a second post
        db.conn()
            .execute(
                "INSERT INTO posts (id, project_id, title, slug, status, file_path, created_at, updated_at) VALUES ('post2', 'p1', 'Test2', 'test2', 'draft', '', 2000, 2000)",
                [],
            )
            .unwrap();

        link_media_to_post(db.conn(), dir.path(), "p1", "post1", "m1", 0).unwrap();
        link_media_to_post(db.conn(), dir.path(), "p1", "post2", "m1", 0).unwrap();

        let ids = read_linked_ids(dir.path(), "media/m1.jpg.meta");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"post1".to_string()));
        assert!(ids.contains(&"post2".to_string()));
    }

    #[test]
    fn list_media_for_post_returns_resolved_media() {
        let (db, dir) = setup();
        insert_test_media(&db, dir.path(), "m1");

        link_media_to_post(db.conn(), dir.path(), "p1", "post1", "m1", 0).unwrap();

        let media = list_media_for_post(db.conn(), "post1").unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].id, "m1");
    }

    #[test]
    fn list_posts_for_media_returns_resolved_posts() {
        let (db, dir) = setup();
        insert_test_media(&db, dir.path(), "m1");

        link_media_to_post(db.conn(), dir.path(), "p1", "post1", "m1", 0).unwrap();

        let posts = list_posts_for_media(db.conn(), "m1").unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, "post1");
    }
}
