-- user_tombstone: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE user_tombstone (
        user_id uuid NOT NULL,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT user_tombstone_pkey PRIMARY KEY (create_time, user_id),
      CONSTRAINT user_tombstone_user_id_key UNIQUE (user_id)
);

CREATE UNIQUE INDEX user_tombstone_user_id_key ON user_tombstone USING btree (user_id);
