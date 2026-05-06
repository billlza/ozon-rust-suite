ALTER TABLE users
    DROP CONSTRAINT chk_users_has_login_identifier;

ALTER TABLE users
    ADD CONSTRAINT chk_users_identity_anchor CHECK (
        (
            nebula_source = 'skybridge'
            AND skybridge_user_id IS NOT NULL
        )
        OR (
            nebula_source = 'local_dev'
            AND skybridge_user_id IS NULL
            AND (email IS NOT NULL OR phone IS NOT NULL)
        )
    );
