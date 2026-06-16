-- H2: device-bound entitlement leases.
--
-- Device ids are now derived deterministically from (user_id, raw device fingerprint) via
-- ozon_domain::device_id_for, so the local node can verify that a signed lease was actually issued
-- for the machine it runs on (preventing a leaked/shared lease from being replayed on another
-- device). Rows created before this change carry random ids that the node cannot reproduce, and
-- the raw fingerprint is not stored (only its hash), so the new id cannot be recomputed in SQL.
-- We therefore clear the table; clients re-activate automatically on the next portal login and
-- receive a fresh, device-bound lease. No table references devices.id, so this delete is safe.
DELETE FROM devices;
