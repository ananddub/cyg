-- message: dumped from the live database with db/gen_schema.sql.
-- Type names are canonicalised (TIMESTAMPTZ/VARCHAR) so FlashORM
-- maps timestamptz columns to chrono::DateTime<chrono::Utc>.

CREATE TABLE message (
        id uuid NOT NULL,
      code smallint NOT NULL DEFAULT 0,
      sender_id uuid NOT NULL,
      username VARCHAR(128) NOT NULL,
      stream_mode smallint NOT NULL,
      stream_subject uuid NOT NULL,
      stream_descriptor uuid NOT NULL,
      stream_label VARCHAR(128) NOT NULL,
      content jsonb NOT NULL DEFAULT '{}'::jsonb,
      create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
      update_time TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT message_pkey PRIMARY KEY (stream_mode, stream_subject, stream_descriptor, stream_label, create_time, id),
      CONSTRAINT message_id_key UNIQUE (id),
      CONSTRAINT message_sender_id_id_key UNIQUE (sender_id, id),
      CONSTRAINT message_sender_id_fkey FOREIGN KEY (sender_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX message_id_key ON message USING btree (id);
CREATE UNIQUE INDEX message_sender_id_id_key ON message USING btree (sender_id, id);
