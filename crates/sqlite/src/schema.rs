diesel::table! {
    games (id) {
        id -> BigInt,
        name -> Text,
        source_id -> Text,
        lookup_id -> Text,
        external_ids -> Json,
        cover_url -> Nullable<Text>,
    }
}

diesel::table! {
    artwork (game_id, kind, position) {
        game_id -> BigInt,
        kind -> Text,
        position -> Integer,
        hash -> Text,
        accent_color -> Nullable<Integer>,
    }
}

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

diesel::joinable!(artwork -> games (game_id));

diesel::allow_tables_to_appear_in_same_query!(games, artwork);
