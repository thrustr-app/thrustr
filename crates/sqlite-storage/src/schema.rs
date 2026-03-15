diesel::table! {
    game_entries (id) {
        id -> BigInt,
        primary_game_id -> BigInt,
    }
}

diesel::table! {
    games (id) {
        id -> BigInt,
        entry_id -> BigInt,
        name -> Text,
        source_id -> Text,
        lookup_id -> Text,
        external_ids -> Json,
    }
}

diesel::joinable!(games -> game_entries (entry_id));
diesel::allow_tables_to_appear_in_same_query!(game_entries, games);

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
