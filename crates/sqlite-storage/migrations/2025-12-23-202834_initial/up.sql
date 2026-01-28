CREATE TABLE plugin_data (
    plugin_id TEXT,
    key TEXT,
    value BLOB NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

CREATE TABLE plugin_config (
    plugin_id TEXT,
    field_id TEXT,
    value TEXT NOT NULL,
    PRIMARY KEY (plugin_id, field_id)
);
