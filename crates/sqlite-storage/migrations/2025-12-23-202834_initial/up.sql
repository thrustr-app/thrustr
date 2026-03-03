CREATE TABLE extension_data (
    component_id TEXT,
    key TEXT,
    value BLOB NOT NULL,
    PRIMARY KEY (component_id, key)
);

CREATE TABLE extension_config (
    component_id TEXT,
    field_id TEXT,
    value TEXT NOT NULL,
    PRIMARY KEY (component_id, field_id)
);
