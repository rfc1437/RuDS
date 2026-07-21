use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::post_media;
use crate::model::PostMedia;

pub fn link_media(conn: &DbConnection, pm: &PostMedia) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(post_media::table)
            .values(pm.clone())
            .execute(c)
            .map(|_| ())
    })
}

pub fn unlink_media(conn: &DbConnection, post_id: &str, media_id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(
            post_media::table
                .filter(post_media::post_id.eq(post_id))
                .filter(post_media::media_id.eq(media_id)),
        )
        .execute(c)
        .map(|_| ())
    })
}

pub fn delete_post_media_by_post(conn: &DbConnection, post_id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(post_media::table.filter(post_media::post_id.eq(post_id)))
            .execute(c)
            .map(|_| ())
    })
}

pub fn list_post_media_by_post(conn: &DbConnection, post_id: &str) -> QueryResult<Vec<PostMedia>> {
    conn.with(|c| {
        post_media::table
            .filter(post_media::post_id.eq(post_id))
            .order((post_media::sort_order.asc(), post_media::media_id.asc()))
            .select(PostMedia::as_select())
            .load(c)
    })
}

pub fn list_post_media_by_media(
    conn: &DbConnection,
    media_id: &str,
) -> QueryResult<Vec<PostMedia>> {
    conn.with(|c| {
        post_media::table
            .filter(post_media::media_id.eq(media_id))
            .order(post_media::created_at)
            .select(PostMedia::as_select())
            .load(c)
    })
}

pub fn update_sort_order(
    conn: &DbConnection,
    post_id: &str,
    media_id: &str,
    sort_order: i32,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(
            post_media::table
                .filter(post_media::post_id.eq(post_id))
                .filter(post_media::media_id.eq(media_id)),
        )
        .set(post_media::sort_order.eq(sort_order))
        .execute(c)
        .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::media::{insert_media, make_test_media};
    use crate::db::queries::post::{insert_post, make_test_post};
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        insert_post(db.conn(), &make_test_post("post1", "p1", "hello")).unwrap();
        insert_media(db.conn(), &make_test_media("m1", "p1")).unwrap();
        insert_media(db.conn(), &make_test_media("m2", "p1")).unwrap();
        db
    }

    fn make_pm(id: &str, media_id: &str, order: i32) -> PostMedia {
        PostMedia {
            id: id.to_string(),
            project_id: "p1".to_string(),
            post_id: "post1".to_string(),
            media_id: media_id.to_string(),
            sort_order: order,
            created_at: 1000,
        }
    }

    #[test]
    fn link_and_list_by_post() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 1)).unwrap();
        link_media(db.conn(), &make_pm("pm2", "m2", 0)).unwrap();
        let list = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].media_id, "m2"); // sort_order 0 first
        assert_eq!(list[1].media_id, "m1");
    }

    #[test]
    fn list_by_media() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        let list = list_post_media_by_media(db.conn(), "m1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].post_id, "post1");
    }

    #[test]
    fn unlink_removes_association() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        unlink_media(db.conn(), "post1", "m1").unwrap();
        let list = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn update_sort_order_changes_value() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        update_sort_order(db.conn(), "post1", "m1", 10).unwrap();
        let list = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert_eq!(list[0].sort_order, 10);
    }

    #[test]
    fn duplicate_post_media_rejected() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        let result = link_media(db.conn(), &make_pm("pm2", "m1", 1));
        assert!(result.is_err());
    }
}
