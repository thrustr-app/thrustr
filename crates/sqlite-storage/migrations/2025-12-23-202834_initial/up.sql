CREATE TABLE extension_data (
    extension_id TEXT,
    key TEXT,
    value BLOB NOT NULL,
    PRIMARY KEY (extension_id, key)
);

CREATE TABLE extension_config (
    extension_id TEXT,
    field_id TEXT,
    value TEXT NOT NULL,
    PRIMARY KEY (extension_id, field_id)
);
