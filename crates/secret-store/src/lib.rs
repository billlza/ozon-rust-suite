use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use keyring_core::{Entry, Error as KeyringError};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{sync::RwLock, time::timeout};

const PRIMARY_SECRET_STORE_TIMEOUT: Duration = Duration::from_millis(500);

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

#[derive(Clone)]
pub struct FileSecretStore {
    path: Arc<PathBuf>,
    lock: Arc<RwLock<()>>,
}

impl FileSecretStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            lock: Arc::new(RwLock::new(())),
        }
    }

    async fn read_map(&self) -> Result<HashMap<String, String>, SecretStoreError> {
        match fs::read_to_string(self.path.as_ref()) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|_| SecretStoreError::InvalidEncoding)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(_) => Err(SecretStoreError::BackendUnavailable),
        }
    }

    async fn write_map(&self, values: &HashMap<String, String>) -> Result<(), SecretStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| SecretStoreError::BackendUnavailable)?;
        }
        let payload = serde_json::to_vec(values).map_err(|_| SecretStoreError::InvalidEncoding)?;
        let temp_path = temp_path_for(self.path.as_ref());
        write_private_file(&temp_path, &payload).await?;
        fs::rename(&temp_path, self.path.as_ref())
            .map_err(|_| SecretStoreError::BackendUnavailable)?;
        Ok(())
    }
}

#[async_trait]
impl SecretStore for FileSecretStore {
    async fn put(&self, name: SecretName, value: SecretString) -> Result<(), SecretStoreError> {
        let _guard = self.lock.write().await;
        let mut values = self.read_map().await?;
        values.insert(name.as_str().to_string(), value.expose_secret().to_string());
        self.write_map(&values).await
    }

    async fn get(&self, name: &SecretName) -> Result<Option<SecretString>, SecretStoreError> {
        let _guard = self.lock.read().await;
        let values = self.read_map().await?;
        Ok(values
            .get(name.as_str())
            .map(|value| SecretString::from(value.clone())))
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        let _guard = self.lock.write().await;
        let mut values = self.read_map().await?;
        values.remove(name.as_str());
        self.write_map(&values).await
    }
}

#[derive(Clone)]
pub struct LayeredSecretStore {
    primary: Arc<dyn SecretStore>,
    fallback: Arc<dyn SecretStore>,
}

impl LayeredSecretStore {
    pub fn new(primary: Arc<dyn SecretStore>, fallback: Arc<dyn SecretStore>) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl SecretStore for LayeredSecretStore {
    async fn put(&self, name: SecretName, value: SecretString) -> Result<(), SecretStoreError> {
        // The primary store (OS keyring) is authoritative. Write it first; only fall back to
        // the on-disk store when the keyring is genuinely unavailable, so a working keyring
        // never leaves a plaintext copy of the secret on disk.
        let primary = timeout(
            PRIMARY_SECRET_STORE_TIMEOUT,
            self.primary.put(name.clone(), value.clone()),
        )
        .await;
        match primary {
            Ok(Ok(())) => {
                // Stored in the keyring; drop any stale plaintext copy from the fallback.
                let _ = self.fallback.delete(&name).await;
                Ok(())
            }
            Ok(Err(_)) | Err(_) => {
                // Keyring unavailable: persist to the 0600 fallback file so the secret is not lost.
                self.fallback.put(name, value).await
            }
        }
    }

    async fn get(&self, name: &SecretName) -> Result<Option<SecretString>, SecretStoreError> {
        // Read the authoritative keyring first; consult the fallback only on a keyring miss or
        // when the keyring is unavailable.
        let primary = timeout(PRIMARY_SECRET_STORE_TIMEOUT, self.primary.get(name)).await;
        match primary {
            Ok(Ok(Some(value))) => {
                // Authoritative hit; ensure no stale plaintext copy lingers in the fallback.
                let _ = self.fallback.delete(name).await;
                Ok(Some(value))
            }
            Ok(Ok(None)) => {
                // Not in the keyring. A value may exist in the fallback from a legacy write or a
                // period when the keyring was unavailable: migrate it into the keyring and drop
                // the plaintext copy so it stops living on disk.
                match self.fallback.get(name).await? {
                    Some(value) => {
                        let migrated = timeout(
                            PRIMARY_SECRET_STORE_TIMEOUT,
                            self.primary.put(name.clone(), value.clone()),
                        )
                        .await;
                        if matches!(migrated, Ok(Ok(()))) {
                            let _ = self.fallback.delete(name).await;
                        }
                        Ok(Some(value))
                    }
                    None => Ok(None),
                }
            }
            Ok(Err(_)) | Err(_) => {
                // Keyring errored or timed out: fall back to the on-disk store for this read.
                self.fallback.get(name).await
            }
        }
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        // Revocation must be reliable: remove the secret from both layers and report failure if
        // the authoritative keyring delete does not succeed, otherwise a revoked secret could be
        // resurrected from the keyring on the next read.
        let primary = timeout(PRIMARY_SECRET_STORE_TIMEOUT, self.primary.delete(name)).await;
        let primary = match primary {
            Ok(result) => result,
            Err(_) => Err(SecretStoreError::BackendUnavailable),
        };
        let fallback = self.fallback.delete(name).await;
        primary?;
        fallback?;
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

fn temp_path_for(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "secrets.json".into());
    file_name.push(".tmp");
    path.with_file_name(file_name)
}

#[cfg(unix)]
async fn write_private_file(path: &Path, payload: &[u8]) -> Result<(), SecretStoreError> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| SecretStoreError::BackendUnavailable)?;
    file.write_all(payload)
        .map_err(|_| SecretStoreError::BackendUnavailable)?;
    file.sync_all()
        .map_err(|_| SecretStoreError::BackendUnavailable)?;
    Ok(())
}

#[cfg(not(unix))]
async fn write_private_file(path: &Path, payload: &[u8]) -> Result<(), SecretStoreError> {
    fs::write(path, payload).map_err(|_| SecretStoreError::BackendUnavailable)
}

pub fn fingerprint_secret(secret: &SecretString) -> String {
    let digest = Sha256::digest(secret.expose_secret().as_bytes());
    let hex = format!("{digest:x}");
    hex[..12].to_string()
}

pub fn redact(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= 8 {
        return "********".to_string();
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}…{suffix}")
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
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[tokio::test]
    async fn file_store_round_trips_secret() {
        let path = unique_test_secret_path("round-trip");
        let store = FileSecretStore::new(&path);
        let name = SecretName::new("openai_config");

        store
            .put(name.clone(), SecretString::from("stored-value"))
            .await
            .expect("file secret write");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let value = store.get(&name).await.unwrap().unwrap();
        assert_eq!(value.expose_secret(), "stored-value");
        store.delete(&name).await.expect("delete secret");
        assert!(store.get(&name).await.unwrap().is_none());
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn layered_store_reads_from_fallback_when_primary_misses() {
        let primary = Arc::new(MemorySecretStore::default());
        let fallback = Arc::new(MemorySecretStore::default());
        let store = LayeredSecretStore::new(primary, fallback.clone());
        let name = SecretName::new("cloud_lease");
        fallback
            .put(name.clone(), SecretString::from("lease"))
            .await
            .unwrap();

        let value = store.get(&name).await.unwrap().unwrap();
        assert_eq!(value.expose_secret(), "lease");
    }

    #[tokio::test]
    async fn layered_store_put_keeps_secret_out_of_fallback_when_primary_ok() {
        let primary = Arc::new(MemorySecretStore::default());
        let fallback = Arc::new(MemorySecretStore::default());
        let store = LayeredSecretStore::new(primary.clone(), fallback.clone());
        let name = SecretName::new("ozon_api_key");
        store
            .put(name.clone(), SecretString::from("k"))
            .await
            .unwrap();
        // Authoritative keyring holds it; no plaintext copy remains in the fallback.
        assert!(fallback.get(&name).await.unwrap().is_none());
        assert_eq!(
            primary.get(&name).await.unwrap().unwrap().expose_secret(),
            "k"
        );
        // A delete clears both layers.
        store.delete(&name).await.unwrap();
        assert!(primary.get(&name).await.unwrap().is_none());
        assert!(fallback.get(&name).await.unwrap().is_none());
    }

    #[test]
    fn redaction_keeps_shape_without_leaking_full_value() {
        assert_eq!(redact("abcdef123456"), "abcd…3456");
        assert_eq!(redact("short"), "********");
    }

    fn unique_test_secret_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("ozon-secret-store-{label}-{nanos}.json"))
    }
}
