-- groups: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE groups (
        id uuid NOT NULL,
      creator_id uuid NOT NULL,
      name VARCHAR(255) NOT NULL,
      description VARCHAR(255),
      avatar_url VARCHAR(512),
      lang_tag VARCHAR(18) NOT NULL DEFAULT 'en'::varchar,
      metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
      state smallint NOT NULL DEFAULT 0,
      edge_count integer NOT NULL DEFAULT 0,
      max_count integer NOT NULL DEFAULT 100,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      disable_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
        CONSTRAINT groups_state_check CHECK ((state >= 0)),
      CONSTRAINT groups_check CHECK (((edge_count >= 1) AND (edge_count <= max_count))),
      CONSTRAINT groups_max_count_check CHECK ((max_count >= 1)),
      CONSTRAINT groups_pkey PRIMARY KEY (disable_time, lang_tag, edge_count, id),
      CONSTRAINT groups_id_key UNIQUE (id),
      CONSTRAINT groups_name_key UNIQUE (name)
);

CREATE UNIQUE INDEX groups_id_key ON groups USING btree (id);
CREATE UNIQUE INDEX groups_name_key ON groups USING btree (name);
CREATE INDEX edge_count_update_time_id_idx ON groups USING btree (disable_time, edge_count, update_time, id);
CREATE INDEX update_time_edge_count_id_idx ON groups USING btree (disable_time, update_time, edge_count, id);
