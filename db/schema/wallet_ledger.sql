-- wallet_ledger: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE wallet_ledger (
        id uuid NOT NULL,
      user_id uuid NOT NULL,
      changeset jsonb NOT NULL,
      metadata jsonb NOT NULL,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT wallet_ledger_pkey PRIMARY KEY (user_id, create_time, id),
      CONSTRAINT wallet_ledger_id_key UNIQUE (id),
      CONSTRAINT wallet_ledger_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX wallet_ledger_id_key ON wallet_ledger USING btree (id);
