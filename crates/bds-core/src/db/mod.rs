mod connection;
pub mod fts;
mod migrations;
pub mod queries;
pub mod schema;
#[doc(hidden)]
pub mod types;

pub use connection::{Database, DatabaseError, DbConnection};
pub use diesel::result::Error as DbQueryError;
pub use migrations::run_migrations;
