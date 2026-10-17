-- storage_index: player-declared JSON-path indexes over `storage`.
-- A definition names the value keys a query may filter (`fields`) and the keys an
-- order may sort (`sortable_fields`); indexed reads run as JSONB expressions over
-- the storage rows, so no copy of the objects is kept and versioned reads stay live.

CREATE TABLE storage_index (
    name            VARCHAR(128)    NOT NULL,
    collection      VARCHAR(128)    NOT NULL,
    key             VARCHAR(128)    NOT NULL DEFAULT '',
    fields          JSONB           NOT NULL DEFAULT '[]'::jsonb,
    sortable_fields JSONB           NOT NULL DEFAULT '[]'::jsonb,
    max_entries     INT             NOT NULL DEFAULT 10000 CHECK (max_entries >= 1),
    index_only      BOOLEAN         NOT NULL DEFAULT false,
    user_id         uuid            NOT NULL,
    create_time     TIMESTAMPTZ     NOT NULL DEFAULT now(),
    CONSTRAINT storage_index_pkey PRIMARY KEY (name),
    CONSTRAINT storage_index_fields_check CHECK (jsonb_typeof(fields) = 'array'),
    CONSTRAINT storage_index_sortable_check CHECK (jsonb_typeof(sortable_fields) = 'array'),
    CONSTRAINT storage_index_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX storage_index_collection_idx ON storage_index USING btree (collection);
