diesel::table! {
    plugin_data (plugin_id, key) {
        plugin_id -> Text,
        key -> Text,
        value -> Binary,
    }
}

diesel::table! {
    plugin_config (plugin_id, field_id) {
        plugin_id -> Text,
        field_id -> Text,
        value -> Text,
    }
}
