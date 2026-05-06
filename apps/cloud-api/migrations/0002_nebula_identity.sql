ALTER TABLE users
    ADD COLUMN nebula_id text,
    ADD COLUMN phone text,
    ADD COLUMN email_verified_at timestamptz,
    ADD COLUMN phone_verified_at timestamptz;

UPDATE users
SET nebula_id = concat(
    'NEBULA-',
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY'),
    '-',
    upper(replace(id::text, '-', ''))
)
WHERE nebula_id IS NULL;

ALTER TABLE users
    ALTER COLUMN nebula_id SET NOT NULL,
    ALTER COLUMN email DROP NOT NULL,
    ADD CONSTRAINT uq_users_nebula_id UNIQUE (nebula_id),
    ADD CONSTRAINT uq_users_phone UNIQUE (phone),
    ADD CONSTRAINT chk_users_has_login_identifier CHECK (email IS NOT NULL OR phone IS NOT NULL);
