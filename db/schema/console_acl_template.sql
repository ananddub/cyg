-- console_acl_template: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE console_acl_template (
        id uuid NOT NULL,
      name VARCHAR(64) NOT NULL,
      description VARCHAR(64) NOT NULL DEFAULT ''::varchar,
      acl jsonb NOT NULL DEFAULT '{}'::jsonb,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT console_acl_template_name_check CHECK ((length((name)::text) > 0)),
      CONSTRAINT console_acl_template_pkey PRIMARY KEY (id),
      CONSTRAINT template_name_uniq UNIQUE (name)
);

CREATE UNIQUE INDEX template_name_uniq ON console_acl_template USING btree (name);
