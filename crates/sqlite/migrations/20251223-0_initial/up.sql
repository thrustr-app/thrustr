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

CREATE VIRTUAL TABLE games_fts USING fts5 (
  name,
  summary,
  description,
  content = 'games',
  content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER games_fts_after_insert AFTER INSERT ON games BEGIN
INSERT INTO
  games_fts (rowid, name, summary, description)
VALUES
  (new.id, new.name, new.summary, new.description);

END;

CREATE TRIGGER games_fts_after_delete AFTER DELETE ON games BEGIN
INSERT INTO
  games_fts (games_fts, rowid, name, summary, description)
VALUES
  (
    'delete',
    old.id,
    old.name,
    old.summary,
    old.description
  );

END;

CREATE TRIGGER games_fts_after_update AFTER
UPDATE ON games BEGIN
INSERT INTO
  games_fts (games_fts, rowid, name, summary, description)
VALUES
  (
    'delete',
    old.id,
    old.name,
    old.summary,
    old.description
  );

INSERT INTO
  games_fts (rowid, name, summary, description)
VALUES
  (new.id, new.name, new.summary, new.description);

END;
