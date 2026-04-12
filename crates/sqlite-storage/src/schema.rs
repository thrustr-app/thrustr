diesel::table! {
    games (id) {
        id -> BigInt,
        name -> Text,
        source_id -> Text,
        lookup_id -> Text,
        external_ids -> Json,
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
