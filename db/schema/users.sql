-- users: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE users (
        id uuid NOT NULL,
      username VARCHAR(128) NOT NULL,
      display_name VARCHAR(255),
      avatar_url VARCHAR(512),
      lang_tag VARCHAR(18) NOT NULL DEFAULT 'en'::varchar,
      location VARCHAR(255),
      timezone VARCHAR(255),
      metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
      wallet jsonb NOT NULL DEFAULT '{}'::jsonb,
      email VARCHAR(255),
      password bytea,
      facebook_id VARCHAR(128),
      google_id VARCHAR(128),
      gamecenter_id VARCHAR(128),
      steam_id VARCHAR(128),
      custom_id VARCHAR(128),
      edge_count integer NOT NULL DEFAULT 0,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      verify_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
      disable_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
      facebook_instant_game_id VARCHAR(128),
      apple_id VARCHAR(128),
      game_id uuid,
        CONSTRAINT users_password_check CHECK ((length(password) < 32000)),
      CONSTRAINT users_edge_count_check CHECK ((edge_count >= 0)),
      CONSTRAINT users_pkey PRIMARY KEY (id),
      CONSTRAINT users_username_key UNIQUE (username),
      CONSTRAINT users_email_key UNIQUE (email),
      CONSTRAINT users_facebook_id_key UNIQUE (facebook_id),
      CONSTRAINT users_google_id_key UNIQUE (google_id),
      CONSTRAINT users_gamecenter_id_key UNIQUE (gamecenter_id),
      CONSTRAINT users_steam_id_key UNIQUE (steam_id),
      CONSTRAINT users_custom_id_key UNIQUE (custom_id),
      CONSTRAINT users_facebook_instant_game_id_key UNIQUE (facebook_instant_game_id),
      CONSTRAINT users_apple_id_key UNIQUE (apple_id)
);

CREATE UNIQUE INDEX users_username_key ON users USING btree (username);
CREATE UNIQUE INDEX users_email_key ON users USING btree (email);
CREATE UNIQUE INDEX users_facebook_id_key ON users USING btree (facebook_id);
CREATE UNIQUE INDEX users_google_id_key ON users USING btree (google_id);
CREATE UNIQUE INDEX users_gamecenter_id_key ON users USING btree (gamecenter_id);
CREATE UNIQUE INDEX users_steam_id_key ON users USING btree (steam_id);
CREATE UNIQUE INDEX users_custom_id_key ON users USING btree (custom_id);
CREATE UNIQUE INDEX users_facebook_instant_game_id_key ON users USING btree (facebook_instant_game_id);
CREATE UNIQUE INDEX users_apple_id_key ON users USING btree (apple_id);
CREATE INDEX users_display_name_idx ON users USING gin (display_name gin_trgm_ops);
CREATE INDEX idx_users_game ON users USING btree (game_id);
