CREATE TABLE game_entries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  primary_game_id INTEGER NOT NULL REFERENCES games (id) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE games (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id INTEGER NOT NULL REFERENCES game_entries (id) ON DELETE CASCADE,
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

CREATE INDEX idx_game_entries_primary_game_id ON game_entries (primary_game_id);

CREATE INDEX idx_games_name ON games (name);

CREATE INDEX idx_games_entry_id ON games (entry_id);
