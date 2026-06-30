-- V002__add_exif_columns.sql
-- Adds EXIF / IPTC / XMP / capture metadata to the photos table.
-- Existing rows get NULL for every new column.

ALTER TABLE photos ADD COLUMN orientation    INTEGER;
ALTER TABLE photos ADD COLUMN date_taken     INTEGER;        -- unix timestamp
ALTER TABLE photos ADD COLUMN camera_make    TEXT;
ALTER TABLE photos ADD COLUMN camera_model   TEXT;
ALTER TABLE photos ADD COLUMN lens           TEXT;
ALTER TABLE photos ADD COLUMN focal_length   REAL;
ALTER TABLE photos ADD COLUMN aperture       REAL;           -- f-number (e.g. 2.8)
ALTER TABLE photos ADD COLUMN shutter_speed  REAL;           -- exposure time in seconds (e.g. 0.004)
ALTER TABLE photos ADD COLUMN iso            INTEGER;
ALTER TABLE photos ADD COLUMN gps_lat        REAL;
ALTER TABLE photos ADD COLUMN gps_lon        REAL;
ALTER TABLE photos ADD COLUMN gps_altitude   REAL;
ALTER TABLE photos ADD COLUMN copyright      TEXT;
ALTER TABLE photos ADD COLUMN file_format    TEXT;           -- "CR2", "DNG", "JPEG", ...
ALTER TABLE photos ADD COLUMN file_extension TEXT;           -- ".cr2", ".dng", ".jpg", ...
ALTER TABLE photos ADD COLUMN error          TEXT;           -- last import error, if any

-- Useful indexes for browsing the library.
CREATE INDEX photos_date_taken ON photos(date_taken);
CREATE INDEX photos_camera_make_model ON photos(camera_make, camera_model);
CREATE INDEX photos_iso ON photos(iso);
CREATE INDEX photos_file_format ON photos(file_format);
