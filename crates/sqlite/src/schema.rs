diesel::table! {
    games (id) {
        id -> BigInt,
        name -> Text,
        sort_name -> Text,
        search_text -> Text,
        source_id -> Text,
        lookup_id -> Text,
        external_ids -> Json,
        cover_url -> Nullable<Text>,
        summary -> Nullable<Text>,
        description -> Nullable<Text>,
        removed -> Bool,
    }
}

diesel::table! {
    artwork (game_id, kind, position) {
        game_id -> BigInt,
        kind -> Text,
        position -> Integer,
        hash -> Text,
        accent -> Nullable<Integer>,
    }
}

diesel::table! {
    search_vocab (word) {
        word -> Text,
    }
}

// spellfix1 virtual table.
diesel::table! {
    games_spellfix (word) {
        word -> Text,
    }
}

diesel::table! {
    component_data (component_id, key) {
        component_id -> Text,
        key -> Text,
        value -> Binary,
        expires_at -> Nullable<BigInt>,
    }
}

diesel::table! {
    component_config (component_id, field_id) {
        component_id -> Text,
        field_id -> Text,
        value -> Json,
    }
}

diesel::table! {
    app_state (key) {
        key -> Text,
        value -> Json,
    }
}

diesel::table! {
    downloads (id) {
        id -> BigInt,
        game_id -> BigInt,
        position -> Integer,
        version_path -> Json,
        target_dir -> Text,
        install_size -> Nullable<BigInt>,
        download_size -> Nullable<BigInt>,
        phase -> Text,
        processed -> BigInt,
        total -> Nullable<BigInt>,
        error -> Nullable<Text>,
    }
}

diesel::joinable!(artwork -> games (game_id));
diesel::joinable!(downloads -> games (game_id));

diesel::allow_tables_to_appear_in_same_query!(games, artwork);
