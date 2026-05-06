use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use url::Url;

#[derive(Clone)]
pub struct OzonCredentials {
    pub client_id: String,
    pub api_key: SecretString,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonProductSummary {
    pub product_id: String,
    pub offer_id: String,
    pub name: Option<String>,
    pub visibility: Option<String>,
    pub archived: Option<bool>,
    pub has_fbo_stocks: Option<bool>,
    pub has_fbs_stocks: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonProductListPage {
    pub products: Vec<OzonProductSummary>,
    pub total: u32,
    pub last_id: Option<String>,
}

#[async_trait]
pub trait OzonReadConnector: Send + Sync {
    async fn validate_credentials(
        &self,
        credentials: &OzonCredentials,
    ) -> Result<(), OzonConnectorError>;
    async fn product_count(&self, credentials: &OzonCredentials)
    -> Result<u32, OzonConnectorError>;
    async fn product_list(
        &self,
        credentials: &OzonCredentials,
        limit: u16,
    ) -> Result<Vec<OzonProductSummary>, OzonConnectorError>;
    async fn product_list_page(
        &self,
        credentials: &OzonCredentials,
        limit: u16,
        last_id: Option<String>,
    ) -> Result<OzonProductListPage, OzonConnectorError>;
}

#[derive(Clone)]
pub struct OzonHttpClient {
    client: reqwest::Client,
    base_url: Url,
}

impl OzonHttpClient {
    pub fn new() -> Self {
        Self {
            client: Self::build_client(),
            base_url: Url::parse("https://api-seller.ozon.ru").expect("static url"),
        }
    }

    pub fn with_base_url(base_url: Url) -> Self {
        Self {
            client: Self::build_client(),
            base_url,
        }
    }

    fn build_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .user_agent("ozon-rust-suite/0.1")
            .build()
            .expect("valid reqwest client")
    }
}

impl Default for OzonHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ProductListFilter {
    fn default() -> Self {
        Self {
            visibility: "ALL".to_string(),
        }
    }
}

#[async_trait]
impl OzonReadConnector for OzonHttpClient {
    async fn validate_credentials(
        &self,
        credentials: &OzonCredentials,
    ) -> Result<(), OzonConnectorError> {
        let _ = self.product_count(credentials).await?;
        Ok(())
    }

    async fn product_count(
        &self,
        credentials: &OzonCredentials,
    ) -> Result<u32, OzonConnectorError> {
        let page = self.product_list_page(credentials, 1, None).await?;
        Ok(page.total)
    }

    async fn product_list(
        &self,
        credentials: &OzonCredentials,
        limit: u16,
    ) -> Result<Vec<OzonProductSummary>, OzonConnectorError> {
        Ok(self
            .product_list_page(credentials, limit, None)
            .await?
            .products)
    }

    async fn product_list_page(
        &self,
        credentials: &OzonCredentials,
        limit: u16,
        last_id: Option<String>,
    ) -> Result<OzonProductListPage, OzonConnectorError> {
        let url = self
            .base_url
            .join("/v3/product/list")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let response = self
            .client
            .post(url)
            .header("Client-Id", &credentials.client_id)
            .header("Api-Key", credentials.api_key.expose_secret())
            .json(&ProductListRequest {
                filter: ProductListFilter::default(),
                limit: limit.clamp(1, 1000),
                last_id: last_id.unwrap_or_default(),
            })
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read Ozon error body".to_string());
            return Err(OzonConnectorError::ApiStatus {
                status,
                body: sanitize_api_error_body(&body),
            });
        }

        let body: ProductListResponse = response.json().await?;
        let total = body
            .result
            .total
            .ok_or_else(|| OzonConnectorError::UnexpectedResponse("missing result.total".into()))?;
        let products = body
            .result
            .items
            .into_iter()
            .map(|item| OzonProductSummary {
                product_id: item.product_id.to_string(),
                offer_id: item.offer_id,
                name: item.name,
                visibility: item.visibility,
                archived: item.archived,
                has_fbo_stocks: item.has_fbo_stocks,
                has_fbs_stocks: item.has_fbs_stocks,
            })
            .collect();
        Ok(OzonProductListPage {
            products,
            total,
            last_id: body.result.last_id.filter(|value| !value.is_empty()),
        })
    }
}

#[derive(Clone, Default)]
pub struct MockOzonConnector;

#[async_trait]
impl OzonReadConnector for MockOzonConnector {
    async fn validate_credentials(
        &self,
        _credentials: &OzonCredentials,
    ) -> Result<(), OzonConnectorError> {
        Ok(())
    }

    async fn product_count(
        &self,
        _credentials: &OzonCredentials,
    ) -> Result<u32, OzonConnectorError> {
        Ok(3)
    }

    async fn product_list(
        &self,
        _credentials: &OzonCredentials,
        limit: u16,
    ) -> Result<Vec<OzonProductSummary>, OzonConnectorError> {
        Ok(self
            .product_list_page(_credentials, limit, None)
            .await?
            .products)
    }

    async fn product_list_page(
        &self,
        _credentials: &OzonCredentials,
        limit: u16,
        _last_id: Option<String>,
    ) -> Result<OzonProductListPage, OzonConnectorError> {
        let products = (0..limit.min(3))
            .map(|idx| OzonProductSummary {
                product_id: format!("mock-product-{idx}"),
                offer_id: format!("SKU-MOCK-{idx}"),
                name: Some(format!("Mock Ozon product {}", idx + 1)),
                visibility: Some("visible".to_string()),
                archived: Some(false),
                has_fbo_stocks: Some(idx % 2 == 0),
                has_fbs_stocks: Some(true),
            })
            .collect();
        Ok(OzonProductListPage {
            products,
            total: 3,
            last_id: None,
        })
    }
}

#[derive(Debug, Error)]
pub enum OzonConnectorError {
    #[error("invalid Ozon API base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("Ozon API returned status {status}: {body}")]
    ApiStatus { status: u16, body: String },
    #[error("unexpected Ozon API response: {0}")]
    UnexpectedResponse(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

#[derive(Serialize)]
struct ProductListRequest {
    filter: ProductListFilter,
    limit: u16,
    last_id: String,
}

#[derive(Serialize)]
struct ProductListFilter {
    visibility: String,
}

#[derive(Deserialize)]
struct ProductListResponse {
    result: ProductListResult,
}

#[derive(Deserialize)]
struct ProductListResult {
    items: Vec<ProductListItem>,
    total: Option<u32>,
    last_id: Option<String>,
}

#[derive(Deserialize)]
struct ProductListItem {
    product_id: u64,
    offer_id: String,
    name: Option<String>,
    visibility: Option<String>,
    archived: Option<bool>,
    has_fbo_stocks: Option<bool>,
    has_fbs_stocks: Option<bool>,
}

fn sanitize_api_error_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_page_exposes_total_without_counting_page_size() {
        let connector = MockOzonConnector;
        let credentials = OzonCredentials {
            client_id: "mock-client-id".to_string(),
            api_key: SecretString::from("mock-api-key"),
        };

        let page = connector
            .product_list_page(&credentials, 1, None)
            .await
            .expect("mock page");

        assert_eq!(page.products.len(), 1);
        assert_eq!(page.total, 3);
        assert_eq!(connector.product_count(&credentials).await.unwrap(), 3);
    }

    #[test]
    fn api_error_body_is_compacted_and_bounded() {
        let body = format!("{}\n{}", "x".repeat(600), "secret-looking-noise");
        let sanitized = sanitize_api_error_body(&body);

        assert_eq!(sanitized.chars().count(), 500);
        assert!(!sanitized.contains('\n'));
    }
}
