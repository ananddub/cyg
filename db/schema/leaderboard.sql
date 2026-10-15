-- leaderboard: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE leaderboard (
        id VARCHAR(128) NOT NULL,
      authoritative boolean NOT NULL DEFAULT false,
      sort_order smallint NOT NULL DEFAULT 1,
      operator smallint NOT NULL DEFAULT 0,
      reset_schedule VARCHAR(64),
      metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      category smallint NOT NULL DEFAULT 0,
      description VARCHAR(255) NOT NULL DEFAULT ''::varchar,
      duration integer NOT NULL DEFAULT 0,
      end_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
      join_required boolean NOT NULL DEFAULT false,
      max_size integer NOT NULL DEFAULT 100000000,
      max_num_score integer NOT NULL DEFAULT 1000000,
      title VARCHAR(255) NOT NULL DEFAULT ''::varchar,
      size integer NOT NULL DEFAULT 0,
      start_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      enable_ranks boolean DEFAULT true,
        CONSTRAINT leaderboard_pkey PRIMARY KEY (id),
      CONSTRAINT leaderboard_category_check CHECK ((category >= 0)),
      CONSTRAINT leaderboard_duration_check CHECK ((duration >= 0)),
      CONSTRAINT leaderboard_max_size_check CHECK ((max_size > 0)),
      CONSTRAINT leaderboard_max_num_score_check CHECK ((max_num_score > 0))
);

CREATE INDEX duration_start_time_end_time_category_idx ON leaderboard USING btree (duration, start_time, end_time DESC, category);
CREATE INDEX leaderboard_create_time_id_idx ON leaderboard USING btree (create_time, id);
