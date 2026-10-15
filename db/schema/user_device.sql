-- user_device: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE user_device (
        id VARCHAR(128) NOT NULL,
      user_id uuid NOT NULL,
      preferences jsonb NOT NULL DEFAULT '{}'::jsonb,
      push_token_amazon VARCHAR(512) NOT NULL DEFAULT ''::varchar,
      push_token_android VARCHAR(512) NOT NULL DEFAULT ''::varchar,
      push_token_huawei VARCHAR(512) NOT NULL DEFAULT ''::varchar,
      push_token_ios VARCHAR(512) NOT NULL DEFAULT ''::varchar,
      push_token_web VARCHAR(512) NOT NULL DEFAULT ''::varchar,
      provider VARCHAR(128) NOT NULL DEFAULT ''::varchar,
        CONSTRAINT user_device_pkey PRIMARY KEY (id),
      CONSTRAINT user_device_user_id_id_key UNIQUE (user_id, id),
      CONSTRAINT user_device_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX user_device_user_id_id_key ON user_device USING btree (user_id, id);
