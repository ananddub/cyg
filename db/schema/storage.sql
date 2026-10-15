-- storage: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE storage (
        collection VARCHAR(128) NOT NULL,
      key VARCHAR(128) NOT NULL,
      user_id uuid NOT NULL,
      value jsonb NOT NULL DEFAULT '{}'::jsonb,
      version VARCHAR(32) NOT NULL,
      read smallint NOT NULL DEFAULT 1,
      write smallint NOT NULL DEFAULT 1,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT storage_read_check CHECK ((read >= 0)),
      CONSTRAINT storage_write_check CHECK ((write >= 0)),
      CONSTRAINT storage_pkey PRIMARY KEY (collection, key, user_id),
      CONSTRAINT storage_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX collection_read_user_id_key_idx ON storage USING btree (collection, read, user_id, key);
CREATE INDEX collection_read_key_user_id_idx ON storage USING btree (collection, read, key, user_id);
CREATE INDEX collection_user_id_read_key_idx ON storage USING btree (collection, user_id, read, key);
CREATE INDEX storage_auto_index_fk_user_id_ref_users ON storage USING btree (user_id);
