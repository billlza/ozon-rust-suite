use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use keyring_core::{Entry, Error as KeyringError};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SecretName(String);

impl SecretName {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn put(&self, name: SecretName, value: SecretString) -> Result<(), SecretStoreError>;
    async fn get(&self, name: &SecretName) -> Result<Option<SecretString>, SecretStoreError>;
    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError>;
}

#[derive(Clone, Default)]
pub struct MemorySecretStore {
    inner: Arc<RwLock<HashMap<SecretName, SecretString>>>,
}

#[async_trait]
impl SecretStore for MemorySecretStore {
    async fn put(&self, name: SecretName, value: SecretString) -> Result<(), SecretStoreError> {
        self.inner.write().await.insert(name, value);
        Ok(())
    }

    async fn get(&self, name: &SecretName) -> Result<Option<SecretString>, SecretStoreError> {
        Ok(self.inner.read().await.get(name).cloned())
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        self.inner.write().await.remove(name);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SystemSecretStore {
    service: String,
    account_prefix: String,
}

impl SystemSecretStore {
    pub fn new(
        service: impl Into<String>,
        account_prefix: impl Into<String>,
    ) -> Result<Self, SecretStoreError> {
        keyring::use_native_store(true).map_err(map_keyring_error)?;
        let store = Self {
            service: service.into(),
            account_prefix: account_prefix.into(),
        };
        store.validate()?;
        Ok(store)
    }

    fn validate(&self) -> Result<(), SecretStoreError> {
        if !is_safe_identifier(&self.service) || !is_safe_identifier(&self.account_prefix) {
            return Err(SecretStoreError::InvalidName);
        }
        Ok(())
    }

    fn account_for(&self, name: &SecretName) -> Result<String, SecretStoreError> {
        if !is_safe_identifier(name.as_str()) {
            return Err(SecretStoreError::InvalidName);
        }
        Ok(format!("{}:{}", self.account_prefix, name.as_str()))
    }
}

#[async_trait]
impl SecretStore for SystemSecretStore {
    async fn put(&self, name: SecretName, value: SecretString) -> Result<(), SecretStoreError> {
        let service = self.service.clone();
        let account = self.account_for(&name)?;
        let value = value.expose_secret().to_string();
        tokio::task::spawn_blocking(move || {
            Entry::new(&service, &account)
                .and_then(|entry| entry.set_password(&value))
                .map_err(map_keyring_error)
        })
        .await
        .map_err(|_| SecretStoreError::BackendUnavailable)??;
        Ok(())
    }

    async fn get(&self, name: &SecretName) -> Result<Option<SecretString>, SecretStoreError> {
        let service = self.service.clone();
        let account = self.account_for(name)?;
        tokio::task::spawn_blocking(move || {
            let entry = Entry::new(&service, &account).map_err(map_keyring_error)?;
            match entry.get_password() {
                Ok(value) => Ok(Some(SecretString::from(value))),
                Err(KeyringError::NoEntry) => Ok(None),
                Err(error) => Err(map_keyring_error(error)),
            }
        })
        .await
        .map_err(|_| SecretStoreError::BackendUnavailable)?
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        let service = self.service.clone();
        let account = self.account_for(name)?;
        tokio::task::spawn_blocking(move || {
            let entry = Entry::new(&service, &account).map_err(map_keyring_error)?;
            match entry.delete_credential() {
                Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
                Err(error) => Err(map_keyring_error(error)),
            }
        })
        .await
        .map_err(|_| SecretStoreError::BackendUnavailable)??;
        Ok(())
    }
}

pub fn fingerprint_secret(secret: &SecretString) -> String {
    let digest = Sha256::digest(secret.expose_secret().as_bytes());
    let hex = format!("{digest:x}");
    hex[..12].to_string()
}

pub fn redact(input: &str) -> String {
    if input.len() <= 8 {
        return "********".to_string();
    }
    format!("{}…{}", &input[..4], &input[input.len() - 4..])
}

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}

fn map_keyring_error(error: KeyringError) -> SecretStoreError {
    match error {
        KeyringError::NoEntry => SecretStoreError::BackendUnavailable,
        KeyringError::NoStorageAccess(_) => SecretStoreError::AccessDenied,
        KeyringError::BadEncoding(_) => SecretStoreError::InvalidEncoding,
        KeyringError::Invalid(_, _) | KeyringError::TooLong(_, _) => SecretStoreError::InvalidName,
        KeyringError::NoDefaultStore
        | KeyringError::NotSupportedByStore(_)
        | KeyringError::PlatformFailure(_)
        | KeyringError::BadDataFormat(_, _)
        | KeyringError::BadStoreFormat(_)
        | KeyringError::Ambiguous(_)
        | _ => SecretStoreError::BackendUnavailable,
    }
}

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("secret backend unavailable")]
    BackendUnavailable,
    #[error("secret backend access denied")]
    AccessDenied,
    #[error("invalid secret name")]
    InvalidName,
    #[error("secret value is not valid UTF-8")]
    InvalidEncoding,
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::*;

    #[tokio::test]
    async fn memory_store_round_trips_secret() {
        let store = MemorySecretStore::default();
        let name = SecretName::new("ozon");
        store
            .put(name.clone(), SecretString::from("super-secret"))
            .await
            .unwrap();
        let value = store.get(&name).await.unwrap().unwrap();
        assert_eq!(value.expose_secret(), "super-secret");
    }

    #[test]
    fn redaction_keeps_shape_without_leaking_full_value() {
        assert_eq!(redact("abcdef123456"), "abcd…3456");
        assert_eq!(redact("short"), "********");
    }
}
