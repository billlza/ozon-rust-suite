CREATE TABLE tenants (
    id uuid PRIMARY KEY,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    email text NOT NULL,
    name text,
    password_hash text NOT NULL,
    role text NOT NULL CHECK (role IN ('user', 'admin')),
    created_at timestamptz NOT NULL,
    CONSTRAINT uq_users_email UNIQUE (email)
);

CREATE TABLE orders (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    plan_code text NOT NULL,
    status text NOT NULL CHECK (status IN ('pending_manual_payment', 'confirmed', 'cancelled')),
    payment_reference text NOT NULL,
    created_at timestamptz NOT NULL,
    confirmed_at timestamptz,
    CONSTRAINT uq_orders_payment_reference UNIQUE (payment_reference)
);

CREATE TABLE card_keys (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    order_id uuid REFERENCES orders(id) ON DELETE SET NULL,
    plan_code text NOT NULL,
    code_hash text NOT NULL,
    code_fingerprint text NOT NULL,
    duration_days integer NOT NULL CHECK (duration_days > 0 AND duration_days <= 65535),
    max_devices integer NOT NULL CHECK (max_devices > 0 AND max_devices <= 255),
    status text NOT NULL CHECK (status IN ('available', 'redeemed', 'revoked')),
    redeemed_by uuid REFERENCES users(id) ON DELETE SET NULL,
    redeemed_at timestamptz,
    created_at timestamptz NOT NULL,
    CONSTRAINT uq_card_keys_order_id UNIQUE (order_id),
    CONSTRAINT uq_card_keys_code_fingerprint UNIQUE (code_fingerprint)
);

CREATE TABLE devices (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    name text NOT NULL,
    fingerprint_hash text NOT NULL,
    status text NOT NULL CHECK (status IN ('active', 'revoked')),
    activated_at timestamptz NOT NULL,
    last_seen_at timestamptz,
    CONSTRAINT uq_devices_user_fingerprint UNIQUE (user_id, fingerprint_hash)
);

CREATE TABLE entitlements (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    plan_code text NOT NULL,
    source_card_key_id uuid NOT NULL REFERENCES card_keys(id) ON DELETE RESTRICT,
    features text[] NOT NULL,
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    CONSTRAINT uq_entitlements_source_card_key UNIQUE (source_card_key_id),
    CHECK (features <@ ARRAY[
        'ozon_read',
        'ozon_write_mock',
        'draft_import1688_mock',
        'open_claw_bridge',
        'local_approval'
    ]::text[])
);

CREATE TABLE audit_events (
    id uuid PRIMARY KEY,
    tenant_id uuid REFERENCES tenants(id) ON DELETE SET NULL,
    actor text NOT NULL,
    action text NOT NULL,
    target text NOT NULL,
    summary text NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE INDEX idx_orders_user_id ON orders(user_id);
CREATE INDEX idx_card_keys_fingerprint ON card_keys(code_fingerprint);
CREATE INDEX idx_devices_user_id ON devices(user_id);
CREATE INDEX idx_entitlements_user_id ON entitlements(user_id);
CREATE INDEX idx_audit_events_created_at ON audit_events(created_at);
