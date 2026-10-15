-- subscription: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE subscription (
        original_transaction_id VARCHAR(512) NOT NULL,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      environment smallint NOT NULL DEFAULT 0,
      product_id VARCHAR(512) NOT NULL,
      purchase_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      store smallint NOT NULL DEFAULT 0,
      user_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'::uuid,
      expire_time TIMESTAMPTZ NOT NULL,
      raw_response jsonb NOT NULL DEFAULT '{}'::jsonb,
      raw_notification jsonb NOT NULL DEFAULT '{}'::jsonb,
      refund_time TIMESTAMPTZ NOT NULL DEFAULT '1970-01-01 00:00:00+00'::TIMESTAMPTZ,
        CONSTRAINT subscription_original_transaction_id_check CHECK ((length((original_transaction_id)::text) > 0)),
      CONSTRAINT subscription_pkey PRIMARY KEY (original_transaction_id),
      CONSTRAINT subscription_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET DEFAULT
);

CREATE INDEX subscription_time_user_id_transaction_id_idx ON subscription USING btree (purchase_time DESC, user_id DESC, original_transaction_id DESC);
