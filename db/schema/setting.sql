-- setting: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE setting (
        name VARCHAR(64) NOT NULL,
      value jsonb NOT NULL DEFAULT '{}'::jsonb,
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT setting_name_check CHECK ((length((name)::text) > 0)),
      CONSTRAINT setting_name_uniq PRIMARY KEY (name)
);
