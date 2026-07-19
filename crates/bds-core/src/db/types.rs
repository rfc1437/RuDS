use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::{Integer, Text};
use diesel::sqlite::{Sqlite, SqliteValue};

use crate::model::{
    NotificationAction, NotificationEntity, PostStatus, ProposalKind, ProposalStatus, ScriptKind,
    ScriptStatus, TemplateKind, TemplateStatus,
};

#[derive(Debug, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub struct DbBool(bool);

impl From<bool> for DbBool {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

impl From<DbBool> for bool {
    fn from(value: DbBool) -> Self {
        value.0
    }
}

impl ToSql<Integer, Sqlite> for DbBool {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(i32::from(self.0));
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for DbBool {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(Self(i32::from_sql(value)? != 0))
    }
}

#[derive(Debug, AsExpression, FromSqlRow)]
#[diesel(sql_type = Text)]
pub struct DbStringList(Vec<String>);

impl From<Vec<String>> for DbStringList {
    fn from(value: Vec<String>) -> Self {
        Self(value)
    }
}

impl From<DbStringList> for Vec<String> {
    fn from(value: DbStringList) -> Self {
        value.0
    }
}

impl ToSql<Text, Sqlite> for DbStringList {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(serde_json::to_string(&self.0)?);
        Ok(IsNull::No)
    }
}

impl FromSql<Text, Sqlite> for DbStringList {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let value = <String as FromSql<Text, Sqlite>>::from_sql(value)?;
        Ok(Self(serde_json::from_str(&value)?))
    }
}

macro_rules! text_enum_sql {
    ($type:ty) => {
        impl ToSql<Text, Sqlite> for $type {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
                out.set_value(self.as_str().to_owned());
                Ok(IsNull::No)
            }
        }

        impl FromSql<Text, Sqlite> for $type {
            fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
                let value = <String as FromSql<Text, Sqlite>>::from_sql(value)?;
                value.parse().map_err(Into::into)
            }
        }
    };
}

text_enum_sql!(PostStatus);
text_enum_sql!(TemplateKind);
text_enum_sql!(TemplateStatus);
text_enum_sql!(ScriptKind);
text_enum_sql!(ScriptStatus);
text_enum_sql!(NotificationEntity);
text_enum_sql!(NotificationAction);
text_enum_sql!(ProposalKind);
text_enum_sql!(ProposalStatus);
