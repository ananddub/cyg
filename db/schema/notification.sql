-- notification: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE notification (
        id uuid NOT NULL,
      user_id uuid NOT NULL,
      subject VARCHAR(255) NOT NULL,
      content jsonb NOT NULL DEFAULT '{}'::jsonb,
      code smallint NOT NULL,
      sender_id uuid NOT NULL,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT notification_pkey PRIMARY KEY (user_id, create_time, id),
      CONSTRAINT notification_id_key UNIQUE (id),
      CONSTRAINT notification_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX notification_id_key ON notification USING btree (id);
