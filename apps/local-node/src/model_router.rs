//! Module 5: capability-registry / model-router foundation.
//!
//! This layer puts a thin indirection in front of provider configuration so that
//! capability call sites (image / text / video generation) resolve their provider
//! through a capability lookup instead of reaching for `load_openai_config`
//! directly. A persisted [`StoredModelRegistry`] lets an operator point each
//! capability at a concrete provider entry; when no entry is configured for the
//! image capability, the resolver falls back EXACTLY to the legacy
//! `load_openai_config` behavior so the image path is byte-for-byte preserved.
//!
//! PR2 adds the registry types, the `endpoint_for` / `apply_auth` utilities (pure,
//! unit-tested, and consumed when modules 3/6 are built), the `/config/registry`
//! save route (in `main.rs`), and the registry-aware resolver + status. It does
//! NOT add request/response codecs or HTTP handlers for chat/video — `Generic`
//! is returned and ready for them, and that is enough.
//!
//! The resolver performs NO auth or lease gating; callers gate before resolving.
//! Status surfaces fingerprints/presence only, never raw keys. The persisted
//! registry blob NEVER contains a raw key — only `secret_ref` strings.

use serde::{Deserialize, Serialize};

/// A capability that the local node can (potentially) serve.
// TextGen / VideoGen resolve to `Generic` once a registry entry is configured;
// with no entry they report "not configured" (modules 3 and 6 consume them).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ImageGen,
    TextGen,
    VideoGen,
}

/// The concrete provider shape behind a registry entry. Determines which
/// endpoint / request convention a future module will use.
// OpenAiCompatChat / CloudVideo are forward scaffolding: `endpoint_for` knows
// how to build their URLs, but no handler invokes them in PR2.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "openai_images")]
    OpenAiImages,
    #[serde(rename = "openai_images_edit")]
    OpenAiImagesEdit,
    #[serde(rename = "openai_compat_chat")]
    OpenAiCompatChat,
    #[serde(rename = "cloud_video")]
    CloudVideo,
}

/// How the API key is placed on an outbound request.
// `Header` / `Query` are wired through `apply_auth` for modules 3/6; PR2 image
// path always uses Bearer (the default).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStyle {
    Bearer,
    Header { name: String },
    Query { name: String },
}

impl Default for AuthStyle {
    fn default() -> Self {
        AuthStyle::Bearer
    }
}

fn default_enabled() -> bool {
    true
}

/// A single configured provider for a capability. The key itself is never stored
/// here — only a `secret_ref` that resolves to a key (e.g. `"openai_config"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub secret_ref: String,
    #[serde(default)]
    pub auth: AuthStyle,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// The persisted registry: ordered provider entries per capability. The first
/// enabled entry for a capability wins.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredModelRegistry {
    #[serde(default)]
    pub image_gen: Vec<ProviderEntry>,
    #[serde(default)]
    pub text_gen: Vec<ProviderEntry>,
    #[serde(default)]
    pub video_gen: Vec<ProviderEntry>,
}

/// The provider resolved for a given capability.
// `Generic` is consumed by modules 3/6; PR2 returns it but no handler reads it
// yet. `NotConfigured.capability` is carried for status/registry consumers.
#[allow(dead_code)]
pub enum ResolvedProvider {
    /// The OpenAI Images provider, backed by a `StoredOpenAiConfig`-shaped value.
    OpenAiImage(crate::StoredOpenAiConfig),
    /// A generic provider (chat / video), ready for module 3 / 6 codecs.
    Generic {
        kind: ProviderKind,
        base_url: String,
        model: String,
        api_key: String,
        auth: AuthStyle,
    },
    /// No provider is configured for this capability.
    NotConfigured { capability: String, reason: String },
}

impl ResolvedProvider {
    /// Unwrap to an OpenAI image config, mirroring the client-error behavior of
    /// the existing `load_openai_config` path (a `bad_request` precondition
    /// failure) when no provider is configured.
    pub fn expect_openai_image(self) -> Result<crate::StoredOpenAiConfig, crate::ApiError> {
        match self {
            ResolvedProvider::OpenAiImage(config) => Ok(config),
            ResolvedProvider::NotConfigured { reason, .. } => {
                Err(crate::ApiError::bad_request(reason))
            }
            ResolvedProvider::Generic { .. } => Err(crate::ApiError::bad_request(
                "resolved provider is not an OpenAI image provider",
            )),
        }
    }
}

/// Build the request endpoint for a provider kind from its base URL.
///
/// For `OpenAiImages` / `OpenAiImagesEdit` this is byte-identical to the legacy
/// `openai_images_endpoint` / `openai_images_edit_endpoint` helpers (same `/v1`
/// handling). `OpenAiCompatChat` and `CloudVideo` follow the same `/v1`
/// convention. Pure; consumed by modules 3/6.
#[allow(dead_code)]
pub fn endpoint_for(kind: ProviderKind, base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let has_v1 = base.ends_with("/v1");
    let suffix = match kind {
        ProviderKind::OpenAiImages => "images/generations",
        ProviderKind::OpenAiImagesEdit => "images/edits",
        ProviderKind::OpenAiCompatChat => "chat/completions",
        ProviderKind::CloudVideo => "video/generations",
    };
    if has_v1 {
        format!("{base}/{suffix}")
    } else {
        format!("{base}/v1/{suffix}")
    }
}

/// Apply the configured auth style to an outbound request. `Bearer` is identical
/// to `reqwest::RequestBuilder::bearer_auth`. Pure; consumed by modules 3/6.
#[allow(dead_code)]
pub fn apply_auth(
    req: reqwest::RequestBuilder,
    auth: &AuthStyle,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match auth {
        AuthStyle::Bearer => req.bearer_auth(api_key),
        AuthStyle::Header { name } => req.header(name, api_key),
        AuthStyle::Query { name } => req.query(&[(name.as_str(), api_key)]),
    }
}

/// Resolve the provider for a capability. Performs no auth/lease gating.
///
/// The first `enabled` entry for the capability wins. For `ImageGen` with no
/// enabled entry, falls back EXACTLY to `OpenAiImage(load_openai_config(..))` —
/// byte-for-byte legacy behavior. For `TextGen` / `VideoGen` with no entry,
/// returns `NotConfigured`.
pub async fn resolve_capability(
    state: &crate::LocalState,
    cap: Capability,
) -> Result<ResolvedProvider, crate::ApiError> {
    let registry = crate::load_persisted_model_registry(state).await;
    let entries = match cap {
        Capability::ImageGen => &registry.image_gen,
        Capability::TextGen => &registry.text_gen,
        Capability::VideoGen => &registry.video_gen,
    };
    let entry = entries.iter().find(|entry| entry.enabled);

    match cap {
        Capability::ImageGen => match entry {
            Some(entry) => {
                let api_key = crate::resolve_secret_ref(state, &entry.secret_ref).await?;
                Ok(ResolvedProvider::OpenAiImage(
                    crate::StoredOpenAiConfig::for_registry(
                        api_key,
                        entry.base_url.clone(),
                        entry.model.clone(),
                    ),
                ))
            }
            None => Ok(ResolvedProvider::OpenAiImage(
                crate::load_openai_config(state).await?,
            )),
        },
        Capability::TextGen | Capability::VideoGen => match entry {
            Some(entry) => {
                let api_key = crate::resolve_secret_ref(state, &entry.secret_ref).await?;
                Ok(ResolvedProvider::Generic {
                    kind: entry.kind,
                    base_url: entry.base_url.clone(),
                    model: entry.model.clone(),
                    api_key,
                    auth: entry.auth.clone(),
                })
            }
            None => Ok(ResolvedProvider::NotConfigured {
                capability: match cap {
                    Capability::TextGen => "text_gen".into(),
                    _ => "video_gen".into(),
                },
                reason: match cap {
                    Capability::TextGen => "no text provider configured".into(),
                    _ => "no video provider configured".into(),
                },
            }),
        },
    }
}

/// Additive, read-only status for a single capability. Fingerprint-only: a key
/// is reported as present iff it resolves; the raw key is never exposed.
#[derive(Debug, Serialize)]
pub struct CapabilityStatus {
    capability: String,
    ready: bool,
    provider_kind: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    secret_present: bool,
    issue: Option<String>,
}

fn provider_kind_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiImages => "openai_images",
        ProviderKind::OpenAiImagesEdit => "openai_images_edit",
        ProviderKind::OpenAiCompatChat => "openai_compat_chat",
        ProviderKind::CloudVideo => "cloud_video",
    }
}

/// Build the status for a generic (text/video) capability from its registry
/// entries, or a "not configured" placeholder when none is enabled.
async fn generic_capability_status(
    state: &crate::LocalState,
    capability: &str,
    entries: &[ProviderEntry],
) -> CapabilityStatus {
    match entries.iter().find(|entry| entry.enabled) {
        Some(entry) => {
            let secret_present = crate::resolve_secret_ref(state, &entry.secret_ref)
                .await
                .is_ok();
            CapabilityStatus {
                capability: capability.to_string(),
                ready: secret_present,
                provider_kind: Some(provider_kind_label(entry.kind).to_string()),
                base_url: Some(entry.base_url.clone()),
                model: Some(entry.model.clone()),
                secret_present,
                issue: if secret_present {
                    None
                } else {
                    Some("provider secret is not available".to_string())
                },
            }
        }
        None => CapabilityStatus {
            capability: capability.to_string(),
            ready: false,
            provider_kind: None,
            base_url: None,
            model: None,
            secret_present: false,
            issue: Some("not configured".into()),
        },
    }
}

/// Inspect every capability for status reporting. Order: image_gen, text_gen,
/// video_gen. The image capability with no registry entry falls back to the
/// `inspect_openai_config`-derived status (legacy behavior).
pub async fn inspect_capabilities(state: &crate::LocalState) -> Vec<CapabilityStatus> {
    let registry = crate::load_persisted_model_registry(state).await;

    let image = match registry.image_gen.iter().find(|entry| entry.enabled) {
        Some(entry) => {
            let secret_present = crate::resolve_secret_ref(state, &entry.secret_ref)
                .await
                .is_ok();
            CapabilityStatus {
                capability: "image_gen".into(),
                ready: secret_present,
                provider_kind: Some(provider_kind_label(entry.kind).to_string()),
                base_url: Some(entry.base_url.clone()),
                model: Some(entry.model.clone()),
                secret_present,
                issue: if secret_present {
                    None
                } else {
                    Some("provider secret is not available".to_string())
                },
            }
        }
        None => {
            let status = crate::inspect_openai_config(state).await;
            CapabilityStatus {
                capability: "image_gen".into(),
                ready: status.configured,
                provider_kind: Some("openai_images".into()),
                base_url: Some(status.base_url),
                model: Some(status.image_model),
                secret_present: status.api_key_fingerprint.is_some(),
                issue: status.issue,
            }
        }
    };

    vec![
        image,
        generic_capability_status(state, "text_gen", &registry.text_gen).await,
        generic_capability_status(state, "video_gen", &registry.video_gen).await,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Local mirrors of the legacy helpers in main.rs, kept private to main.rs.
    // `endpoint_for` must stay byte-identical to these.
    fn legacy_images_endpoint(base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{base}/images/generations")
        } else {
            format!("{base}/v1/images/generations")
        }
    }

    fn legacy_images_edit_endpoint(base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{base}/images/edits")
        } else {
            format!("{base}/v1/images/edits")
        }
    }

    #[test]
    fn endpoint_for_image_matches_legacy_helpers() {
        for base in ["https://api.openai.com", "https://relay.example.com/v1"] {
            assert_eq!(
                endpoint_for(ProviderKind::OpenAiImages, base),
                legacy_images_endpoint(base),
                "images generations mismatch for {base}"
            );
            assert_eq!(
                endpoint_for(ProviderKind::OpenAiImagesEdit, base),
                legacy_images_edit_endpoint(base),
                "images edits mismatch for {base}"
            );
        }
    }

    #[test]
    fn endpoint_for_chat_and_video_follow_v1_convention() {
        assert_eq!(
            endpoint_for(ProviderKind::OpenAiCompatChat, "https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint_for(ProviderKind::OpenAiCompatChat, "https://relay.example.com/v1"),
            "https://relay.example.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint_for(ProviderKind::CloudVideo, "https://video.example.com"),
            "https://video.example.com/v1/video/generations"
        );
    }

    #[test]
    fn apply_auth_bearer_matches_reqwest_bearer_auth() {
        let client = reqwest::Client::new();
        let expected = client
            .post("https://example.com")
            .bearer_auth("sk-key")
            .build()
            .expect("expected request");
        let actual = apply_auth(
            client.post("https://example.com"),
            &AuthStyle::Bearer,
            "sk-key",
        )
        .build()
        .expect("actual request");
        assert_eq!(
            actual.headers().get(reqwest::header::AUTHORIZATION),
            expected.headers().get(reqwest::header::AUTHORIZATION),
        );
    }

    #[test]
    fn apply_auth_header_and_query_place_key() {
        let client = reqwest::Client::new();
        let header_req = apply_auth(
            client.post("https://example.com"),
            &AuthStyle::Header {
                name: "x-api-key".to_string(),
            },
            "sk-key",
        )
        .build()
        .expect("header request");
        assert_eq!(
            header_req.headers().get("x-api-key").unwrap(),
            "sk-key"
        );

        let query_req = apply_auth(
            client.post("https://example.com"),
            &AuthStyle::Query {
                name: "key".to_string(),
            },
            "sk-key",
        )
        .build()
        .expect("query request");
        assert_eq!(query_req.url().query(), Some("key=sk-key"));
    }

    #[test]
    fn registry_serde_round_trips_with_defaults() {
        // An entry written without `auth`/`enabled` must default to Bearer/true.
        let json = r#"{
            "image_gen": [
                {
                    "kind": "openai_images",
                    "base_url": "https://relay.example.com",
                    "model": "gpt-image-1",
                    "secret_ref": "openai_config"
                }
            ]
        }"#;
        let registry: StoredModelRegistry =
            serde_json::from_str(json).expect("deserialize registry");
        assert_eq!(registry.image_gen.len(), 1);
        let entry = &registry.image_gen[0];
        assert_eq!(entry.auth, AuthStyle::Bearer);
        assert!(entry.enabled);
        assert!(registry.text_gen.is_empty());
        assert!(registry.video_gen.is_empty());

        let serialized = serde_json::to_string(&registry).expect("serialize");
        let back: StoredModelRegistry =
            serde_json::from_str(&serialized).expect("round trip");
        assert_eq!(registry, back);
    }

    #[test]
    fn default_auth_style_is_bearer() {
        assert_eq!(AuthStyle::default(), AuthStyle::Bearer);
    }
}
