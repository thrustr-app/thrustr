diesel::table! {
    extension_data (extension_id, key) {
        extension_id -> Text,
        key -> Text,
        value -> Binary,
    }
}

diesel::table! {
    extension_config (extension_id, field_id) {
        extension_id -> Text,
        field_id -> Text,
        value -> Text,
    }
}
