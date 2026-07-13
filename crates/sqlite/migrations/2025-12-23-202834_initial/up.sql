CREATE TABLE games (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  source_id TEXT NOT NULL,
  lookup_id TEXT NOT NULL,
  external_ids JSON NOT NULL DEFAULT '{}',
  cover_url TEXT,
  summary TEXT,
  description TEXT,
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

CREATE TABLE artwork (
  game_id INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  position INTEGER NOT NULL DEFAULT 0,
  hash TEXT NOT NULL,
  accent_color INTEGER,
  PRIMARY KEY (game_id, kind, position),
  CHECK (
    kind = 'screenshot'
    OR position = 0
  )
);

CREATE INDEX idx_games_name ON games (name);

CREATE INDEX idx_artwork_hash ON artwork (hash);
