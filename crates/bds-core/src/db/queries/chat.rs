use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::{chat_conversations, chat_messages};
use crate::model::{ChatConversation, ChatMessage, NewChatConversation, NewChatMessage};

pub fn insert_conversation(
    conn: &DbConnection,
    conversation: &NewChatConversation<'_>,
) -> QueryResult<ChatConversation> {
    conn.with(|connection| {
        diesel::insert_into(chat_conversations::table)
            .values(conversation)
            .execute(connection)?;
        chat_conversations::table
            .find(conversation.id)
            .select(ChatConversation::as_select())
            .first(connection)
    })
}

pub fn get_conversation(conn: &DbConnection, id: &str) -> QueryResult<ChatConversation> {
    conn.with(|connection| {
        chat_conversations::table
            .find(id)
            .select(ChatConversation::as_select())
            .first(connection)
    })
}

pub fn list_conversations(conn: &DbConnection) -> QueryResult<Vec<ChatConversation>> {
    conn.with(|connection| {
        chat_conversations::table
            .order((
                chat_conversations::updated_at.desc(),
                chat_conversations::id.desc(),
            ))
            .select(ChatConversation::as_select())
            .load(connection)
    })
}

pub fn rename_conversation(
    conn: &DbConnection,
    id: &str,
    title: &str,
    updated_at: i64,
) -> QueryResult<ChatConversation> {
    conn.with(|connection| {
        diesel::update(chat_conversations::table.find(id))
            .set((
                chat_conversations::title.eq(title),
                chat_conversations::updated_at.eq(updated_at),
            ))
            .execute(connection)?;
        chat_conversations::table
            .find(id)
            .select(ChatConversation::as_select())
            .first(connection)
    })
}

pub fn set_conversation_model(
    conn: &DbConnection,
    id: &str,
    model: &str,
    updated_at: i64,
) -> QueryResult<usize> {
    conn.with(|connection| {
        diesel::update(chat_conversations::table.find(id))
            .set((
                chat_conversations::model.eq(model),
                chat_conversations::updated_at.eq(updated_at),
            ))
            .execute(connection)
    })
}

pub fn set_session_id(
    conn: &DbConnection,
    id: &str,
    session_id: Option<&str>,
    updated_at: i64,
) -> QueryResult<usize> {
    conn.with(|connection| {
        diesel::update(chat_conversations::table.find(id))
            .set((
                chat_conversations::copilot_session_id.eq(session_id),
                chat_conversations::updated_at.eq(updated_at),
            ))
            .execute(connection)
    })
}

pub fn delete_conversation(conn: &DbConnection, id: &str) -> QueryResult<usize> {
    conn.with(|connection| {
        connection.transaction(|connection| {
            diesel::delete(chat_messages::table.filter(chat_messages::conversation_id.eq(id)))
                .execute(connection)?;
            diesel::delete(chat_conversations::table.find(id)).execute(connection)
        })
    })
}

pub fn insert_message(
    conn: &DbConnection,
    message: &NewChatMessage<'_>,
    updated_at: i64,
) -> QueryResult<ChatMessage> {
    conn.with(|connection| {
        connection.transaction(|connection| {
            diesel::insert_into(chat_messages::table)
                .values(message)
                .execute(connection)?;
            diesel::update(chat_conversations::table.find(message.conversation_id))
                .set(chat_conversations::updated_at.eq(updated_at))
                .execute(connection)?;
            chat_messages::table
                .order(chat_messages::id.desc())
                .select(ChatMessage::as_select())
                .first(connection)
        })
    })
}

pub fn list_messages(conn: &DbConnection, conversation_id: &str) -> QueryResult<Vec<ChatMessage>> {
    conn.with(|connection| {
        chat_messages::table
            .filter(chat_messages::conversation_id.eq(conversation_id))
            .order((chat_messages::created_at.asc(), chat_messages::id.asc()))
            .select(ChatMessage::as_select())
            .load(connection)
    })
}
