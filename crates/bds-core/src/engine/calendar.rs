//! Calendar JSON generation — counts posts by year, month, day.

use std::collections::BTreeMap;
use std::path::Path;

use crate::db::DbConnection as Connection;

use crate::db::queries::post as post_q;
use crate::engine::EngineResult;
use crate::model::PostStatus;
use crate::util::{atomic_write, timestamp};

/// Generate `html/calendar.json` from published posts.
///
/// The JSON has three top-level objects: `years`, `months`, `days`,
/// each mapping a date key to a count of posts.
pub fn regenerate_calendar(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<()> {
    let posts = post_q::list_posts_by_project(conn, project_id)?;

    let mut years: BTreeMap<String, u32> = BTreeMap::new();
    let mut months: BTreeMap<String, u32> = BTreeMap::new();
    let mut days: BTreeMap<String, u32> = BTreeMap::new();

    for post in &posts {
        if post.status != PostStatus::Published {
            continue;
        }
        let ts = post.created_at;
        let (y, m, d) = timestamp::year_month_day_from_unix_ms(ts);

        let year_key = y.clone();
        let month_key = format!("{y}-{m}");
        let day_key = format!("{y}-{m}-{d}");

        *years.entry(year_key).or_insert(0) += 1;
        *months.entry(month_key).or_insert(0) += 1;
        *days.entry(day_key).or_insert(0) += 1;
    }

    let calendar = serde_json::json!({
        "years": years,
        "months": months,
        "days": days,
    });

    let html_dir = data_dir.join("html");
    std::fs::create_dir_all(&html_dir)?;

    let json_str = serde_json::to_string_pretty(&calendar)?;

    atomic_write::atomic_write(&html_dir.join("calendar.json"), json_str.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::engine;

    #[test]
    fn calendar_empty_project() {
        let db = Database::open_in_memory().unwrap();
        let _ = db.migrate();

        let tmp = tempfile::tempdir().unwrap();
        let p =
            engine::project::create_project(db.conn(), "Test", Some(tmp.path().to_str().unwrap()))
                .unwrap();

        regenerate_calendar(db.conn(), tmp.path(), &p.id).unwrap();

        let cal_path = tmp.path().join("html").join("calendar.json");
        assert!(cal_path.exists());

        let data: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&cal_path).unwrap()).unwrap();
        assert!(data["years"].as_object().unwrap().is_empty());
    }
}
