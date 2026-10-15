-- console_audit_log: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE console_audit_log (
        create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      id uuid NOT NULL,
      console_user_id uuid NOT NULL,
      console_username text NOT NULL,
      email text NOT NULL,
      action text NOT NULL,
      resource text NOT NULL,
      message text NOT NULL,
      metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
        CONSTRAINT console_audit_log_pkey PRIMARY KEY (create_time, console_username, action, resource, id),
      CONSTRAINT console_audit_log_id_key UNIQUE (id)
);

CREATE UNIQUE INDEX console_audit_log_id_key ON console_audit_log USING btree (id);
