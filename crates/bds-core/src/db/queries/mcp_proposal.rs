use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::mcp_proposals;
use crate::model::{McpProposal, ProposalStatus};

pub fn insert_proposal(conn: &DbConnection, proposal: &McpProposal) -> QueryResult<()> {
    conn.with(|connection| {
        diesel::insert_into(mcp_proposals::table)
            .values(proposal)
            .execute(connection)
            .map(|_| ())
    })
}

pub fn get_proposal(conn: &DbConnection, id: &str) -> QueryResult<McpProposal> {
    conn.with(|connection| {
        mcp_proposals::table
            .filter(mcp_proposals::id.eq(id))
            .select(McpProposal::as_select())
            .first(connection)
    })
}

pub fn list_proposals(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<McpProposal>> {
    conn.with(|connection| {
        mcp_proposals::table
            .filter(mcp_proposals::project_id.eq(project_id))
            .order(mcp_proposals::created_at.desc())
            .select(McpProposal::as_select())
            .load(connection)
    })
}

pub fn list_pending_proposals(
    conn: &DbConnection,
    project_id: &str,
) -> QueryResult<Vec<McpProposal>> {
    conn.with(|connection| {
        mcp_proposals::table
            .filter(mcp_proposals::project_id.eq(project_id))
            .filter(mcp_proposals::status.eq(ProposalStatus::Pending))
            .order(mcp_proposals::created_at.asc())
            .select(McpProposal::as_select())
            .load(connection)
    })
}

pub fn expire_pending(conn: &DbConnection, now: i64) -> QueryResult<usize> {
    conn.with(|connection| {
        diesel::update(
            mcp_proposals::table
                .filter(mcp_proposals::status.eq(ProposalStatus::Pending))
                .filter(mcp_proposals::expires_at.le(now)),
        )
        .set((
            mcp_proposals::status.eq(ProposalStatus::Expired),
            mcp_proposals::resolved_at.eq(Some(now)),
            mcp_proposals::result.eq(Some("{\"message\":\"expired\"}".to_string())),
        ))
        .execute(connection)
    })
}

pub fn claim_pending(conn: &DbConnection, id: &str, now: i64) -> QueryResult<bool> {
    conn.with(|connection| {
        diesel::update(
            mcp_proposals::table
                .filter(mcp_proposals::id.eq(id))
                .filter(mcp_proposals::status.eq(ProposalStatus::Pending))
                .filter(mcp_proposals::expires_at.gt(now)),
        )
        .set(mcp_proposals::status.eq(ProposalStatus::Executing))
        .execute(connection)
        .map(|changed| changed == 1)
    })
}

pub fn resolve_claimed(
    conn: &DbConnection,
    id: &str,
    status: ProposalStatus,
    result: &str,
    resolved_at: i64,
) -> QueryResult<bool> {
    debug_assert!(matches!(
        status,
        ProposalStatus::Accepted | ProposalStatus::Rejected
    ));
    conn.with(|connection| {
        diesel::update(
            mcp_proposals::table
                .filter(mcp_proposals::id.eq(id))
                .filter(mcp_proposals::status.eq(ProposalStatus::Executing)),
        )
        .set((
            mcp_proposals::status.eq(status),
            mcp_proposals::result.eq(Some(result.to_string())),
            mcp_proposals::resolved_at.eq(Some(resolved_at)),
        ))
        .execute(connection)
        .map(|changed| changed == 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::model::ProposalKind;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db
    }

    fn proposal(id: &str, expires_at: i64) -> McpProposal {
        McpProposal {
            id: id.into(),
            project_id: "p1".into(),
            kind: ProposalKind::DraftPost,
            status: ProposalStatus::Pending,
            entity_id: None,
            data: "{}".into(),
            result: None,
            created_at: 1,
            expires_at,
            resolved_at: None,
        }
    }

    #[test]
    fn lifecycle_claims_once_and_expires_pending_rows() {
        let db = setup();
        insert_proposal(db.conn(), &proposal("p1", 10)).unwrap();
        insert_proposal(db.conn(), &proposal("p2", 1)).unwrap();
        assert_eq!(expire_pending(db.conn(), 5).unwrap(), 1);
        assert!(claim_pending(db.conn(), "p1", 5).unwrap());
        assert!(!claim_pending(db.conn(), "p1", 5).unwrap());
        assert!(resolve_claimed(db.conn(), "p1", ProposalStatus::Accepted, "{}", 6).unwrap());
        assert_eq!(
            get_proposal(db.conn(), "p1").unwrap().status,
            ProposalStatus::Accepted
        );
        assert_eq!(
            get_proposal(db.conn(), "p2").unwrap().status,
            ProposalStatus::Expired
        );
    }
}
