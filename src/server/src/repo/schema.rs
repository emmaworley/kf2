// @generated automatically by Diesel CLI.

diesel::table! {
    session (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    session_provider_config (session_id, provider_id) {
        session_id -> Text,
        provider_id -> Text,
        config_json -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::joinable!(session_provider_config -> session (session_id));

diesel::allow_tables_to_appear_in_same_query!(session, session_provider_config,);
