//! Entitlement client.
//!
//! NOTE: The previous `require_feature(lease, feature)` helper was removed. It
//! authorized purely on `lease.expires_at` and `lease.features` WITHOUT
//! verifying the lease signature, so a forged/self-constructed
//! `EntitlementLease` could bypass the feature gate. It was unused outside this
//! crate. Feature gating must go through the signature-verifying path
//! (`verify_cloud_lease_signature` in `apps/local-node`), which authenticates
//! the lease before any feature/expiry check.
