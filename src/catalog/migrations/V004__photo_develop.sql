-- V004__photo_develop.sql
-- Per-photo non-destructive develop settings (Lightroom-style basic panel).

CREATE TABLE photo_develop (
    photo_id   INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    exposure   REAL    NOT NULL DEFAULT 0,
    contrast   REAL    NOT NULL DEFAULT 0,
    highlights REAL    NOT NULL DEFAULT 0,
    shadows    REAL    NOT NULL DEFAULT 0,
    whites     REAL    NOT NULL DEFAULT 0,
    blacks     REAL    NOT NULL DEFAULT 0,
    clarity    REAL    NOT NULL DEFAULT 0,
    vibrance   REAL    NOT NULL DEFAULT 0,
    saturation REAL    NOT NULL DEFAULT 0,
    temp       REAL    NOT NULL DEFAULT 0,
    tint       REAL    NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
