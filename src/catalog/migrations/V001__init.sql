-- V001__init.sql
-- Initial catalog schema for realraw.
-- Designed to scale to ~1M photos per catalog.

CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY
);

CREATE TABLE folders (
    id        INTEGER PRIMARY KEY,
    path      TEXT    NOT NULL UNIQUE,
    parent_id INTEGER REFERENCES folders(id) ON DELETE CASCADE
);
CREATE INDEX folders_parent_id ON folders(parent_id);

CREATE TABLE photos (
    id          INTEGER PRIMARY KEY,
    folder_id   INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    path        TEXT    NOT NULL UNIQUE,
    file_size   INTEGER,
    mtime       INTEGER,
    sha1        BLOB,
    width       INTEGER,
    height      INTEGER,
    imported_at INTEGER NOT NULL,
    rating      INTEGER NOT NULL DEFAULT 0,
    pick_flag   INTEGER NOT NULL DEFAULT 0,
    color_label INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX photos_folder_id   ON photos(folder_id);
CREATE INDEX photos_imported_at ON photos(imported_at);
CREATE INDEX photos_rating      ON photos(rating);
CREATE INDEX photos_pick_flag   ON photos(pick_flag);

CREATE TABLE collections (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    parent_id  INTEGER REFERENCES collections(id) ON DELETE CASCADE,
    is_smart   INTEGER NOT NULL DEFAULT 0,
    smart_query TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX collections_parent_id ON collections(parent_id);
CREATE UNIQUE INDEX collections_name_in_parent
    ON collections(parent_id, name) WHERE parent_id IS NOT NULL;
CREATE UNIQUE INDEX collections_root_name
    ON collections(name) WHERE parent_id IS NULL;

CREATE TABLE collection_photos (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    photo_id      INTEGER NOT NULL REFERENCES photos(id)      ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    added_at      INTEGER NOT NULL,
    PRIMARY KEY (collection_id, photo_id)
);
CREATE INDEX collection_photos_photo_id_position
    ON collection_photos(photo_id, collection_id, position);

CREATE TABLE keywords (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE photo_keywords (
    photo_id   INTEGER NOT NULL REFERENCES photos(id)   ON DELETE CASCADE,
    keyword_id INTEGER NOT NULL REFERENCES keywords(id) ON DELETE CASCADE,
    PRIMARY KEY (photo_id, keyword_id)
);
CREATE INDEX photo_keywords_keyword_id ON photo_keywords(keyword_id);

CREATE VIRTUAL TABLE photos_fts USING fts5(
    path,
    caption,
    content='photos',
    content_rowid='id'
);
