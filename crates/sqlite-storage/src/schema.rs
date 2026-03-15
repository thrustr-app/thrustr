diesel::table! {
    game_entries (id) {
        id -> Integer,
        primary_game_id -> Integer,
    }
}

diesel::table! {
    games (id) {
        id -> Integer,
        entry_id -> Integer,
        name -> Text,
        source_id -> Text,
        lookup_id -> Text,
    }
}

diesel::table! {
    game_external_ids (game_id, key) {
        game_id -> Integer,
        key -> Text,
        value -> Text,
    }
}

diesel::joinable!(games -> game_entries (entry_id));
diesel::joinable!(game_external_ids -> games (game_id));
diesel::allow_tables_to_appear_in_same_query!(game_entries, games, game_external_ids,);

diesel::table! {
    component_data (component_id, key) {
        component_id -> Text,
        key -> Text,
        value -> Binary,
    }
}

diesel::table! {
    component_config (component_id, field_id) {
        component_id -> Text,
        field_id -> Text,
        value -> Text,
    }
}
