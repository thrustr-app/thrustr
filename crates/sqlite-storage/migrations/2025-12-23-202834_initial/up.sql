CREATE TABLE plugin_data (
    plugin_id TEXT,
    key TEXT,
    value BLOB,
    PRIMARY KEY (plugin_id, key)
);
