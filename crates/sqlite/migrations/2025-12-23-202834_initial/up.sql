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
  component_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value BLOB NOT NULL,
  PRIMARY KEY (component_id, key)
) WITHOUT ROWID;

CREATE TABLE component_config (
  component_id TEXT NOT NULL,
  field_id TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (component_id, field_id)
) WITHOUT ROWID;

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

CREATE INDEX idx_games_name ON games (name, id);

CREATE INDEX idx_artwork_hash ON artwork (hash);
