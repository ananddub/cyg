-- purchase: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE purchase (
        create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      environment smallint NOT NULL DEFAULT 0,
      product_id VARCHAR(512) NOT NULL,
      purchase_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      raw_response jsonb NOT NULL DEFAULT '{}'::jsonb,
      store smallint NOT NULL DEFAULT 0,
      transaction_id VARCHAR(512) NOT NULL,
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      user_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'::uuid,
      refund_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
        CONSTRAINT purchase_transaction_id_check CHECK ((length((transaction_id)::text) > 0)),
      CONSTRAINT purchase_pkey PRIMARY KEY (transaction_id),
      CONSTRAINT purchase_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET DEFAULT
);

CREATE INDEX purchase_time_user_id_transaction_id_idx ON purchase USING btree (purchase_time DESC, user_id DESC, transaction_id DESC);
