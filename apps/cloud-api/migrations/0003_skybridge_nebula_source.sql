ALTER TABLE users
    ADD COLUMN nebula_source text NOT NULL DEFAULT 'local_dev',
    ADD COLUMN skybridge_user_id uuid,
    ADD CONSTRAINT chk_users_nebula_source CHECK (nebula_source IN ('skybridge', 'local_dev')),
    ADD CONSTRAINT uq_users_skybridge_user_id UNIQUE (skybridge_user_id);

CREATE INDEX idx_users_nebula_source ON users(nebula_source);
