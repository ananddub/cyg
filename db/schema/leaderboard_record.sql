-- leaderboard_record: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE leaderboard_record (
        leaderboard_id VARCHAR(128) NOT NULL,
      owner_id uuid NOT NULL,
      username VARCHAR(128),
      score bigint NOT NULL DEFAULT 0,
      subscore bigint NOT NULL DEFAULT 0,
      num_score integer NOT NULL DEFAULT 1,
      metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      expiry_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
      max_num_score integer NOT NULL DEFAULT 1000000,
        CONSTRAINT leaderboard_record_score_check CHECK ((score >= 0)),
      CONSTRAINT leaderboard_record_subscore_check CHECK ((subscore >= 0)),
      CONSTRAINT leaderboard_record_num_score_check CHECK ((num_score >= 0)),
      CONSTRAINT leaderboard_record_pkey PRIMARY KEY (leaderboard_id, expiry_time, score, subscore, owner_id),
      CONSTRAINT leaderboard_record_owner_id_leaderboard_id_expiry_time_key UNIQUE (owner_id, leaderboard_id, expiry_time),
      CONSTRAINT leaderboard_record_leaderboard_id_fkey FOREIGN KEY (leaderboard_id) REFERENCES leaderboard(id) ON DELETE CASCADE,
      CONSTRAINT leaderboard_record_max_num_score_check CHECK ((max_num_score > 0))
);

CREATE UNIQUE INDEX leaderboard_record_owner_id_leaderboard_id_expiry_time_key ON leaderboard_record USING btree (owner_id, leaderboard_id, expiry_time);
CREATE INDEX owner_id_expiry_time_leaderboard_id_idx ON leaderboard_record USING btree (owner_id, expiry_time, leaderboard_id);
