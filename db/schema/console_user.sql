-- console_user: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE console_user (
        create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      disable_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
      email VARCHAR(255) NOT NULL,
      id uuid NOT NULL,
      metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
      password bytea,
      role smallint NOT NULL DEFAULT 4,
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      username VARCHAR(128) NOT NULL,
      mfa_secret bytea,
      mfa_recovery_codes bytea,
      mfa_required boolean DEFAULT false,
      acl jsonb NOT NULL DEFAULT '{"admin": false}'::jsonb,
        CONSTRAINT console_user_pkey PRIMARY KEY (id),
      CONSTRAINT console_user_email_uniq UNIQUE (email),
      CONSTRAINT console_user_username_uniq UNIQUE (username),
      CONSTRAINT console_user_password_check CHECK ((length(password) < 32000)),
      CONSTRAINT console_user_role_check CHECK ((role >= 1))
);

CREATE UNIQUE INDEX console_user_email_uniq ON console_user USING btree (email);
CREATE UNIQUE INDEX console_user_username_uniq ON console_user USING btree (username);
