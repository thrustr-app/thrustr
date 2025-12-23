diesel::table! {
    plugin_data (plugin_id) {
        plugin_id -> Text,
        data -> Jsonb,
    }
}
