-- users_notes: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE users_notes (
        id uuid NOT NULL,
      user_id uuid NOT NULL,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      note text NOT NULL,
      create_id uuid,
      update_id uuid,
        CONSTRAINT users_notes_pkey PRIMARY KEY (user_id, create_time, id),
      CONSTRAINT users_notes_id_key UNIQUE (id),
      CONSTRAINT users_notes_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX users_notes_id_key ON users_notes USING btree (id);
