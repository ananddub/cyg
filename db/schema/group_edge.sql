-- group_edge: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE group_edge (
        source_id uuid NOT NULL,
      position bigint NOT NULL,
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      destination_id uuid NOT NULL,
      state smallint NOT NULL DEFAULT 0,
        CONSTRAINT group_edge_source_id_check CHECK ((source_id <> '00000000-0000-0000-0000-000000000000'::uuid)),
      CONSTRAINT group_edge_destination_id_check CHECK ((destination_id <> '00000000-0000-0000-0000-000000000000'::uuid)),
      CONSTRAINT group_edge_pkey PRIMARY KEY (source_id, state, "position"),
      CONSTRAINT group_edge_source_id_destination_id_key UNIQUE (source_id, destination_id)
);

CREATE UNIQUE INDEX group_edge_source_id_destination_id_key ON group_edge USING btree (source_id, destination_id);
