ALTER TABLE orders
    DROP CONSTRAINT IF EXISTS orders_status_check;

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS payment_provider text NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS amount_minor bigint NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS currency text NOT NULL DEFAULT 'cny',
    ADD COLUMN IF NOT EXISTS checkout_session_id text,
    ADD COLUMN IF NOT EXISTS payment_intent_id text,
    ADD COLUMN IF NOT EXISTS paid_at timestamptz;

ALTER TABLE orders
    ADD CONSTRAINT orders_status_check
    CHECK (status IN ('pending_manual_payment', 'pending_provider_payment', 'confirmed', 'cancelled'));

ALTER TABLE orders
    ADD CONSTRAINT orders_payment_provider_check
    CHECK (payment_provider IN ('manual', 'stripe', 'alipay', 'wechat_pay'));

ALTER TABLE orders
    ADD CONSTRAINT orders_amount_minor_check
    CHECK (amount_minor >= 0);

ALTER TABLE orders
    ADD CONSTRAINT orders_currency_check
    CHECK (currency ~ '^[a-z]{3}$');

CREATE UNIQUE INDEX IF NOT EXISTS uq_orders_checkout_session_id
    ON orders (checkout_session_id)
    WHERE checkout_session_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS payment_events (
    provider text NOT NULL CHECK (provider IN ('stripe', 'alipay', 'wechat_pay')),
    event_id text NOT NULL,
    event_type text NOT NULL,
    order_id uuid REFERENCES orders(id) ON DELETE SET NULL,
    payload_hash text NOT NULL,
    received_at timestamptz NOT NULL,
    PRIMARY KEY (provider, event_id)
);

CREATE INDEX IF NOT EXISTS idx_payment_events_order_id ON payment_events(order_id);
