CREATE TABLE games (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  source_id TEXT NOT NULL,
  lookup_id TEXT NOT NULL,
  external_ids JSON NOT NULL DEFAULT '{}',
  UNIQUE (source_id, lookup_id)
);

CREATE TABLE component_data (
  component_id TEXT,
  key TEXT,
  value BLOB NOT NULL,
  PRIMARY KEY (component_id, key)
);

CREATE TABLE component_config (
  component_id TEXT,
  field_id TEXT,
  value TEXT NOT NULL,
  PRIMARY KEY (component_id, field_id)
);

CREATE INDEX idx_games_name ON games (name);
