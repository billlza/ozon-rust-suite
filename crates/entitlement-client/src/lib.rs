use chrono::Utc;
use ozon_domain::{EntitlementLease, Feature};
use thiserror::Error;

pub fn require_feature(lease: &EntitlementLease, feature: Feature) -> Result<(), EntitlementError> {
    if lease.expires_at <= Utc::now() {
        return Err(EntitlementError::LeaseExpired);
    }
    if !lease.features.contains(&feature) {
        return Err(EntitlementError::FeatureMissing);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum EntitlementError {
    #[error("entitlement lease expired")]
    LeaseExpired,
    #[error("required feature missing")]
    FeatureMissing,
}
