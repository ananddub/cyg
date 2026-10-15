-- user_edge: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE user_edge (
        source_id uuid NOT NULL,
      position bigint NOT NULL,
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      destination_id uuid NOT NULL,
      state smallint NOT NULL DEFAULT 0,
      metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
        CONSTRAINT user_edge_source_id_check CHECK ((source_id <> '00000000-0000-0000-0000-000000000000'::uuid)),
      CONSTRAINT user_edge_destination_id_check CHECK ((destination_id <> '00000000-0000-0000-0000-000000000000'::uuid)),
      CONSTRAINT user_edge_pkey PRIMARY KEY (source_id, state, "position"),
      CONSTRAINT user_edge_source_id_destination_id_key UNIQUE (source_id, destination_id),
      CONSTRAINT user_edge_source_id_fkey FOREIGN KEY (source_id) REFERENCES users(id) ON DELETE CASCADE,
      CONSTRAINT user_edge_destination_id_fkey FOREIGN KEY (destination_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX user_edge_source_id_destination_id_key ON user_edge USING btree (source_id, destination_id);
CREATE INDEX user_edge_auto_index_fk_destination_id_ref_users ON user_edge USING btree (destination_id);
