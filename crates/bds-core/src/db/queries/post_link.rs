use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::from_row::PostLinkRecord;
use crate::db::schema::post_links;
use crate::model::PostLink;

pub fn insert_post_link(conn: &DbConnection, link: &PostLink) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(post_links::table)
            .values(PostLinkRecord::from(link))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_links_by_source(conn: &DbConnection, source_post_id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(post_links::table.filter(post_links::source_post_id.eq(source_post_id)))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_links_by_target(conn: &DbConnection, target_post_id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(post_links::table.filter(post_links::target_post_id.eq(target_post_id)))
            .execute(c)
            .map(|_| ())
    })
}

pub fn list_links_by_source(
    conn: &DbConnection,
    source_post_id: &str,
) -> QueryResult<Vec<PostLink>> {
    conn.with(|c| {
        post_links::table
            .filter(post_links::source_post_id.eq(source_post_id))
            .order(post_links::created_at)
            .select(PostLinkRecord::as_select())
            .load(c)
            .map(|rows: Vec<PostLinkRecord>| rows.into_iter().map(Into::into).collect())
    })
}

pub fn list_links_by_target(
    conn: &DbConnection,
    target_post_id: &str,
) -> QueryResult<Vec<PostLink>> {
    conn.with(|c| {
        post_links::table
            .filter(post_links::target_post_id.eq(target_post_id))
            .order(post_links::created_at)
            .select(PostLinkRecord::as_select())
            .load(c)
            .map(|rows: Vec<PostLinkRecord>| rows.into_iter().map(Into::into).collect())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::post::{insert_post, make_test_post};
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        insert_post(db.conn(), &make_test_post("a", "p1", "a")).unwrap();
        insert_post(db.conn(), &make_test_post("b", "p1", "b")).unwrap();
        insert_post(db.conn(), &make_test_post("c", "p1", "c")).unwrap();
        db
    }

    fn make_link(id: &str, src: &str, tgt: &str) -> PostLink {
        PostLink {
            id: id.to_string(),
            source_post_id: src.to_string(),
            target_post_id: tgt.to_string(),
            link_text: Some("see also".into()),
            created_at: 1000,
        }
    }

    #[test]
    fn insert_and_list_by_source() {
        let db = setup();
        insert_post_link(db.conn(), &make_link("l1", "a", "b")).unwrap();
        insert_post_link(db.conn(), &make_link("l2", "a", "c")).unwrap();
        let links = list_links_by_source(db.conn(), "a").unwrap();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn list_by_target() {
        let db = setup();
        insert_post_link(db.conn(), &make_link("l1", "a", "c")).unwrap();
        insert_post_link(db.conn(), &make_link("l2", "b", "c")).unwrap();
        let links = list_links_by_target(db.conn(), "c").unwrap();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn delete_by_source() {
        let db = setup();
        insert_post_link(db.conn(), &make_link("l1", "a", "b")).unwrap();
        insert_post_link(db.conn(), &make_link("l2", "a", "c")).unwrap();
        delete_links_by_source(db.conn(), "a").unwrap();
        let links = list_links_by_source(db.conn(), "a").unwrap();
        assert!(links.is_empty());
    }
}
