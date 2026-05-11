use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_type!(TenantId);
id_type!(UserId);
id_type!(OrderId);
id_type!(CardKeyId);
id_type!(DeviceId);
id_type!(EntitlementId);
id_type!(TaskId);
id_type!(ApprovalId);
id_type!(AuditEventId);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Email(String);

impl Email {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into().trim().to_lowercase();
        if value.len() < 5 || !value.contains('@') || value.contains(' ') {
            return Err(DomainError::InvalidEmail);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhoneNumber(String);

impl PhoneNumber {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
        let raw = value.into();
        let mut normalized = String::with_capacity(raw.len());
        for (index, ch) in raw.trim().chars().enumerate() {
            if ch.is_ascii_digit() || (ch == '+' && index == 0) {
                normalized.push(ch);
            } else if matches!(ch, ' ' | '-' | '(' | ')') {
                continue;
            } else {
                return Err(DomainError::InvalidPhoneNumber);
            }
        }

        let digit_count = normalized.chars().filter(|ch| ch.is_ascii_digit()).count();
        if !(6..=15).contains(&digit_count) || normalized == "+" {
            return Err(DomainError::InvalidPhoneNumber);
        }
        if normalized.contains('+') && !normalized.starts_with('+') {
            return Err(DomainError::InvalidPhoneNumber);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NebulaId(String);

impl NebulaId {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into().trim().to_uppercase();
        let parts: Vec<_> = value.split('-').collect();
        if parts.len() != 3
            || parts[0] != "NEBULA"
            || parts[1].len() != 4
            || !parts[1].chars().all(|ch| ch.is_ascii_digit())
            || !(8..=40).contains(&parts[2].len())
            || !parts[2].chars().all(|ch| ch.is_ascii_alphanumeric())
        {
            return Err(DomainError::InvalidNebulaId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub nebula_id: NebulaId,
    pub nebula_source: NebulaSource,
    pub skybridge_user_id: Option<Uuid>,
    pub email: Option<Email>,
    pub phone: Option<PhoneNumber>,
    pub name: Option<String>,
    pub password_hash: String,
    pub role: UserRole,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub phone_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NebulaSource {
    Skybridge,
    LocalDev,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    User,
    Admin,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Order {
    pub id: OrderId,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub plan_code: PlanCode,
    pub status: OrderStatus,
    pub payment_provider: PaymentProvider,
    pub payment_reference: String,
    pub amount_minor: i64,
    pub currency: String,
    pub checkout_session_id: Option<String>,
    pub payment_intent_id: Option<String>,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanCode(pub String);

impl PlanCode {
    pub fn standard_30d() -> Self {
        Self("standard_30d".to_string())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    PendingManualPayment,
    PendingProviderPayment,
    Confirmed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentProvider {
    Manual,
    Stripe,
    Alipay,
    WechatPay,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CardKey {
    pub id: CardKeyId,
    pub tenant_id: TenantId,
    pub plan_code: PlanCode,
    pub code_hash: String,
    pub code_fingerprint: String,
    pub duration_days: u16,
    pub max_devices: u8,
    pub status: CardKeyStatus,
    pub redeemed_by: Option<UserId>,
    pub redeemed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKeyStatus {
    Available,
    Redeemed,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Device {
    pub id: DeviceId,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub name: String,
    pub fingerprint_hash: String,
    pub status: DeviceStatus,
    pub activated_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Active,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Entitlement {
    pub id: EntitlementId,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub plan_code: PlanCode,
    pub source_card_key_id: CardKeyId,
    pub features: Vec<Feature>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    OzonRead,
    OzonWriteMock,
    DraftImport1688Mock,
    OpenClawBridge,
    LocalApproval,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EntitlementLease {
    pub lease_id: Uuid,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub entitlement_id: EntitlementId,
    pub features: Vec<Feature>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl EntitlementLease {
    pub fn new(
        tenant_id: TenantId,
        user_id: UserId,
        device_id: DeviceId,
        entitlement: &Entitlement,
    ) -> Self {
        let issued_at = Utc::now();
        let requested_expiry = issued_at + Duration::hours(24);
        Self {
            lease_id: Uuid::new_v4(),
            tenant_id,
            user_id,
            device_id,
            entitlement_id: entitlement.id,
            features: entitlement.features.clone(),
            issued_at,
            expires_at: requested_expiry.min(entitlement.expires_at),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Rub,
    Cny,
    Usd,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Quantity(pub u32);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub tenant_id: TenantId,
    pub shop_id: String,
    pub source: TaskSource,
    pub operation: OperationKind,
    pub state: TaskState,
    pub dry_run: DryRunDiff,
    pub risk: RiskLevel,
    pub idempotency_key: String,
    pub approval: Option<ApprovalRecord>,
    pub receipt: Option<ExecutionReceipt>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    LocalUi,
    OpenClaw,
    ChromeExtension,
    Admin,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    OzonProductsCount,
    OzonProductsList,
    OzonProductsGet,
    OzonUpdatePriceMock,
    OzonUpdateInventoryMock,
    OzonJoinPromotionMock,
    DraftUploadMock,
    Import1688Mock,
}

impl OperationKind {
    pub fn is_write(self) -> bool {
        !matches!(
            self,
            Self::OzonProductsCount | Self::OzonProductsList | Self::OzonProductsGet
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Draft,
    PendingApproval,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn can_transition_to(self, next: TaskState) -> bool {
        use TaskState::*;
        matches!(
            (self, next),
            (Draft, PendingApproval)
                | (Draft, Queued)
                | (PendingApproval, Queued)
                | (PendingApproval, Cancelled)
                | (Queued, Running)
                | (Queued, Cancelled)
                | (Running, Succeeded)
                | (Running, Failed)
                | (Running, Cancelled)
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DryRunDiff {
    pub summary: String,
    pub target_count: u32,
    pub changes: Vec<FieldChange>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FieldChange {
    pub object_id: String,
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApprovalRecord {
    pub id: ApprovalId,
    pub approved_by: String,
    pub approved_at: DateTime<Utc>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExecutionReceipt {
    pub external_request_id: Option<String>,
    pub executed_at: DateTime<Utc>,
    pub result_summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub tenant_id: Option<TenantId>,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("invalid email")]
    InvalidEmail,
    #[error("invalid phone number")]
    InvalidPhoneNumber,
    #[error("invalid nebula id")]
    InvalidNebulaId,
    #[error("invalid task transition from {from:?} to {to:?}")]
    InvalidTaskTransition { from: TaskState, to: TaskState },
    #[error("write operation requires approval")]
    WriteRequiresApproval,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_operations_are_detected() {
        assert!(!OperationKind::OzonProductsList.is_write());
        assert!(!OperationKind::OzonProductsGet.is_write());
        assert!(OperationKind::OzonUpdatePriceMock.is_write());
    }

    #[test]
    fn task_state_blocks_skipping_approval_for_write_flow() {
        assert!(TaskState::Draft.can_transition_to(TaskState::PendingApproval));
        assert!(!TaskState::Draft.can_transition_to(TaskState::Running));
        assert!(TaskState::PendingApproval.can_transition_to(TaskState::Queued));
    }

    #[test]
    fn email_is_normalized() {
        let email = Email::parse("  USER@Example.COM ").unwrap();
        assert_eq!(email.as_str(), "user@example.com");
    }

    #[test]
    fn phone_number_is_normalized() {
        let phone = PhoneNumber::parse(" +86 138-0013-8000 ").unwrap();
        assert_eq!(phone.as_str(), "+8613800138000");
    }

    #[test]
    fn nebula_id_is_normalized() {
        let nebula_id = NebulaId::parse(" nebula-2026-a1b2c3d4e5f6 ").unwrap();
        assert_eq!(nebula_id.as_str(), "NEBULA-2026-A1B2C3D4E5F6");
    }
}
