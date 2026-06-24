use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OzonProductLookup {
    pub product_id: Option<String>,
    pub offer_id: Option<String>,
    pub sku: Option<String>,
}

impl OzonProductLookup {
    pub fn normalized(self) -> Self {
        Self {
            product_id: normalize_optional(self.product_id),
            offer_id: normalize_optional(self.offer_id),
            sku: normalize_optional(self.sku),
        }
    }

    fn selected_identifier_count(&self) -> usize {
        [
            self.product_id.as_ref(),
            self.offer_id.as_ref(),
            self.sku.as_ref(),
        ]
        .into_iter()
        .flatten()
        .count()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonProductDetail {
    pub lookup: OzonProductLookup,
    pub product_id: String,
    pub offer_id: String,
    pub sku: Option<String>,
    pub name: Option<String>,
    pub description_category_id: Option<u64>,
    pub type_id: Option<u64>,
    pub description: Option<String>,
    pub barcodes: Vec<String>,
    pub primary_image: Option<String>,
    pub images: Vec<OzonProductImage>,
    pub gallery_images: Vec<String>,
    pub images360: Vec<String>,
    pub color_image: Option<String>,
    pub attributes: Vec<OzonProductAttribute>,
    pub visibility: Option<String>,
    pub archived: Option<bool>,
    pub autoarchived: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub statuses: Option<Value>,
    pub source_endpoints: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonProductImage {
    pub url: String,
    pub role: OzonProductImageRole,
    pub position: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OzonProductImageRole {
    Primary,
    Gallery,
    Color,
    Spin360,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonProductAttribute {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub values: Vec<String>,
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
    async fn product_list_page_with_visibility(
        &self,
        credentials: &OzonCredentials,
        limit: u16,
        last_id: Option<String>,
        visibility: Option<String>,
    ) -> Result<OzonProductListPage, OzonConnectorError> {
        let _ = visibility;
        self.product_list_page(credentials, limit, last_id).await
    }
    async fn product_get(
        &self,
        credentials: &OzonCredentials,
        lookup: OzonProductLookup,
    ) -> Result<OzonProductDetail, OzonConnectorError>;
}

/// Write-side Ozon operations. Kept as a separate trait from the read connector
/// so read-only callers cannot accidentally mutate a live store. v1 covers
/// image replacement only — the proven-reliable write path (Ozon fetches each
/// URL we pass and re-hosts it on its own CDN, with `images[0]` as the primary).
#[async_trait]
pub trait OzonWriteConnector: Send + Sync {
    /// Replace a product's image list. `images[0]` becomes the primary image.
    async fn pictures_import(
        &self,
        credentials: &OzonCredentials,
        product_id: &str,
        images: Vec<String>,
    ) -> Result<OzonPicturesImport, OzonConnectorError>;

    /// Read the category attribute dictionary for a `description_category_id` +
    /// `type_id`. Returns the attribute definitions (id, name, whether the
    /// attribute is dictionary-backed and its `dictionary_id`). This is a READ
    /// call but lives on the write connector because it is only ever needed to
    /// safely resolve names -> numeric ids before a write.
    async fn description_category_attributes(
        &self,
        credentials: &OzonCredentials,
        description_category_id: u64,
        type_id: u64,
    ) -> Result<Vec<OzonCategoryAttribute>, OzonConnectorError>;

    /// Read the paginated dictionary values for one attribute's `dictionary_id`.
    /// `attribute_id` is required by the Ozon endpoint to scope the dictionary.
    async fn description_category_attribute_values(
        &self,
        credentials: &OzonCredentials,
        description_category_id: u64,
        type_id: u64,
        attribute_id: u64,
    ) -> Result<Vec<OzonCategoryAttributeValue>, OzonConnectorError>;

    /// Update a product's title / description / attributes by ALREADY-RESOLVED
    /// numeric ids. This method does NO name/value matching — it sends exactly
    /// the `attributes` (and optional `name`) it is given. Keyed by `offer_id`
    /// (Ozon merges the import by offer_id).
    async fn product_update_copy(
        &self,
        credentials: &OzonCredentials,
        update: OzonProductCopyUpdate,
    ) -> Result<OzonProductCopyUpdateResult, OzonConnectorError>;
}

/// One attribute definition from the description-category dictionary.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonCategoryAttribute {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub is_collection: bool,
    /// `0` means "free-text" (no dictionary); any non-zero id is a dictionary.
    #[serde(default)]
    pub dictionary_id: u64,
    #[serde(default)]
    pub attribute_type: Option<String>,
}

impl OzonCategoryAttribute {
    /// A dictionary-typed attribute requires a `value_id` match; a free-text
    /// attribute passes a raw string through.
    pub fn is_dictionary(&self) -> bool {
        self.dictionary_id != 0
    }
}

/// One dictionary value (value_id + display value) for an attribute.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonCategoryAttributeValue {
    pub value_id: u64,
    pub value: String,
}

/// A fully-resolved single value to write for an attribute: either a numeric
/// dictionary `value_id`, or a free-text `value` string.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum OzonResolvedValue {
    Dictionary { dictionary_value_id: u64 },
    FreeText { value: String },
}

/// A fully-resolved attribute to write: numeric `attribute_id` + resolved
/// values. No name/string matching happens past this point.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonResolvedAttribute {
    pub attribute_id: u64,
    pub values: Vec<OzonResolvedValue>,
}

/// The payload for a copy (title/description/attributes) update. `name` and
/// `description` are optional so callers can update only what changed.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonProductCopyUpdate {
    pub offer_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub attributes: Vec<OzonResolvedAttribute>,
}

/// Result of a copy update. `task_id` is the Ozon import task (the import is
/// asynchronous); `accepted` reflects a 2xx submission.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonProductCopyUpdateResult {
    pub offer_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Ozon's well-known attribute id for the product description (annotation),
/// used so a description update rides inside the same `/v3/product/import`
/// attributes array as the other attributes.
pub const OZON_DESCRIPTION_ATTRIBUTE_ID: u64 = 4191;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonPicturesImport {
    pub product_id: String,
    pub pictures: Vec<OzonPictureState>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OzonPictureState {
    pub url: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub is_primary: Option<bool>,
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
        self.product_list_page_with_visibility(credentials, limit, last_id, None)
            .await
    }

    async fn product_list_page_with_visibility(
        &self,
        credentials: &OzonCredentials,
        limit: u16,
        last_id: Option<String>,
        visibility: Option<String>,
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
                filter: ProductListFilter {
                    visibility: normalize_product_list_visibility(visibility)?,
                },
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

    async fn product_get(
        &self,
        credentials: &OzonCredentials,
        lookup: OzonProductLookup,
    ) -> Result<OzonProductDetail, OzonConnectorError> {
        let lookup = validate_lookup(lookup)?;
        let info_item = self.product_info_list(credentials, &lookup).await?;
        let mut detail = detail_from_info_item(&lookup, info_item)?;
        let attribute_lookup = OzonProductLookup {
            product_id: Some(detail.product_id.clone()),
            offer_id: None,
            sku: None,
        };

        match self
            .product_info_attributes(credentials, &attribute_lookup)
            .await
        {
            Ok(Some(supplement)) => apply_attribute_supplement(&mut detail, supplement),
            Ok(None) => detail
                .warnings
                .push("Ozon attributes endpoint returned no matching product".to_string()),
            Err(error) => detail.warnings.push(format!(
                "Ozon attributes endpoint could not enrich product facts: {error}"
            )),
        }

        // Best-effort description enrichment: never fail product_get if the
        // dedicated description endpoint is unavailable.
        let description_lookup = OzonProductLookup {
            product_id: Some(detail.product_id.clone()),
            offer_id: None,
            sku: None,
        };
        match self
            .product_info_description(credentials, &description_lookup)
            .await
        {
            Ok(Some(description)) => {
                detail.description = Some(description);
                detail
                    .source_endpoints
                    .push("/v1/product/info/description".to_string());
            }
            Ok(None) => {}
            Err(error) => detail.warnings.push(format!(
                "Ozon description endpoint could not enrich product facts: {error}"
            )),
        }

        if detail.images.is_empty() {
            detail
                .warnings
                .push("Ozon returned no product images for this product".to_string());
        }
        Ok(detail)
    }
}

impl OzonHttpClient {
    async fn product_info_list(
        &self,
        credentials: &OzonCredentials,
        lookup: &OzonProductLookup,
    ) -> Result<ProductInfoItem, OzonConnectorError> {
        let url = self
            .base_url
            .join("/v3/product/info/list")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let response = self
            .client
            .post(url)
            .header("Client-Id", &credentials.client_id)
            .header("Api-Key", credentials.api_key.expose_secret())
            .json(&ProductInfoListRequest::from_lookup(lookup)?)
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

        let body: ProductInfoListResponse = response.json().await?;
        body.items
            .into_iter()
            .next()
            .or_else(|| {
                body.result
                    .and_then(|result| result.items.into_iter().next())
            })
            .ok_or_else(|| OzonConnectorError::ProductNotFound(lookup_label(lookup)))
    }

    async fn product_info_attributes(
        &self,
        credentials: &OzonCredentials,
        lookup: &OzonProductLookup,
    ) -> Result<Option<ProductAttributesSupplement>, OzonConnectorError> {
        let Some(request) = ProductInfoAttributesRequest::from_lookup(lookup)? else {
            return Ok(None);
        };
        let url = self
            .base_url
            .join("/v4/product/info/attributes")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let response = self
            .client
            .post(url)
            .header("Client-Id", &credentials.client_id)
            .header("Api-Key", credentials.api_key.expose_secret())
            .json(&request)
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

        let body: Value = response.json().await?;
        Ok(first_attribute_item(&body).map(ProductAttributesSupplement::from_value))
    }

    async fn product_info_description(
        &self,
        credentials: &OzonCredentials,
        lookup: &OzonProductLookup,
    ) -> Result<Option<String>, OzonConnectorError> {
        let request = ProductInfoDescriptionRequest::from_lookup(lookup)?;
        let url = self
            .base_url
            .join("/v1/product/info/description")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let response = self
            .client
            .post(url)
            .header("Client-Id", &credentials.client_id)
            .header("Api-Key", credentials.api_key.expose_secret())
            .json(&request)
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

        let body: Value = response.json().await?;
        Ok(parse_product_description(&body))
    }
}

#[async_trait]
impl OzonWriteConnector for OzonHttpClient {
    async fn pictures_import(
        &self,
        credentials: &OzonCredentials,
        product_id: &str,
        images: Vec<String>,
    ) -> Result<OzonPicturesImport, OzonConnectorError> {
        let product_number: i64 = product_id.trim().parse().map_err(|_| {
            OzonConnectorError::InvalidProductLookup(format!(
                "product_id must be numeric to import pictures, got {product_id:?}"
            ))
        })?;
        if images.is_empty() {
            return Err(OzonConnectorError::InvalidProductLookup(
                "pictures_import requires at least one image URL".to_string(),
            ));
        }
        let url = self
            .base_url
            .join("/v1/product/pictures/import")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let response = self
            .client
            .post(url)
            .header("Client-Id", &credentials.client_id)
            .header("Api-Key", credentials.api_key.expose_secret())
            .json(&PicturesImportRequest {
                product_id: product_number,
                images: images.clone(),
                color_image: String::new(),
                images360: Vec::new(),
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

        let body: Value = response.json().await?;
        Ok(OzonPicturesImport {
            product_id: product_id.to_string(),
            pictures: parse_picture_states(&body, &images),
        })
    }

    async fn description_category_attributes(
        &self,
        credentials: &OzonCredentials,
        description_category_id: u64,
        type_id: u64,
    ) -> Result<Vec<OzonCategoryAttribute>, OzonConnectorError> {
        let url = self
            .base_url
            .join("/v1/description-category/attribute")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let response = self
            .client
            .post(url)
            .header("Client-Id", &credentials.client_id)
            .header("Api-Key", credentials.api_key.expose_secret())
            .json(&CategoryAttributeRequest {
                description_category_id,
                type_id,
                language: "DEFAULT",
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

        let body: Value = response.json().await?;
        Ok(parse_category_attributes(&body))
    }

    async fn description_category_attribute_values(
        &self,
        credentials: &OzonCredentials,
        description_category_id: u64,
        type_id: u64,
        attribute_id: u64,
    ) -> Result<Vec<OzonCategoryAttributeValue>, OzonConnectorError> {
        let url = self
            .base_url
            .join("/v1/description-category/attribute/values")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let mut values: Vec<OzonCategoryAttributeValue> = Vec::new();
        let mut last_value_id: u64 = 0;
        // Bounded pagination: Ozon dictionaries can be large; cap the number of
        // pages so a runaway dictionary cannot hang a write preview.
        for _ in 0..200 {
            let response = self
                .client
                .post(url.clone())
                .header("Client-Id", &credentials.client_id)
                .header("Api-Key", credentials.api_key.expose_secret())
                .json(&CategoryAttributeValuesRequest {
                    description_category_id,
                    type_id,
                    attribute_id,
                    limit: 5000,
                    last_value_id,
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

            let body: CategoryAttributeValuesResponse = response.json().await?;
            // Advance the cursor by the max RAW id (before empty-value filtering) so
            // pagination still progresses even when a whole page is filtered out.
            let raw_max = body.result.iter().map(|item| item.id).max();
            let page: Vec<OzonCategoryAttributeValue> = body
                .result
                .into_iter()
                .filter_map(|item| {
                    let value = non_empty_string(&item.value)?;
                    Some(OzonCategoryAttributeValue {
                        value_id: item.id,
                        value,
                    })
                })
                .collect();
            values.extend(page);
            if !body.has_next {
                break;
            }
            match raw_max {
                Some(max_id) => last_value_id = max_id,
                // has_next is true but the page carried no id to advance the cursor;
                // stop rather than re-fetch the same page until the page cap.
                None => break,
            }
        }
        Ok(values)
    }

    async fn product_update_copy(
        &self,
        credentials: &OzonCredentials,
        update: OzonProductCopyUpdate,
    ) -> Result<OzonProductCopyUpdateResult, OzonConnectorError> {
        let offer_id = update.offer_id.trim().to_string();
        if offer_id.is_empty() {
            return Err(OzonConnectorError::InvalidProductLookup(
                "product_update_copy requires a non-empty offer_id".to_string(),
            ));
        }

        // Build the attributes array. The description (if present) rides inside
        // the same attributes array as Ozon's annotation attribute so a single
        // /v3/product/import call updates title + description + attributes.
        let mut attributes: Vec<ImportAttribute> = update
            .attributes
            .iter()
            .map(ImportAttribute::from_resolved)
            .collect();
        if let Some(description) = update
            .description
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            attributes.push(ImportAttribute {
                id: OZON_DESCRIPTION_ATTRIBUTE_ID,
                values: vec![ImportAttributeValue {
                    dictionary_value_id: None,
                    value: Some(description.to_string()),
                }],
            });
        }

        let item = ImportItem {
            offer_id: offer_id.clone(),
            name: update
                .name
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            attributes,
        };

        let url = self
            .base_url
            .join("/v3/product/import")
            .map_err(|error| OzonConnectorError::InvalidBaseUrl(error.to_string()))?;
        let response = self
            .client
            .post(url)
            .header("Client-Id", &credentials.client_id)
            .header("Api-Key", credentials.api_key.expose_secret())
            .json(&ImportRequest { items: vec![item] })
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

        let body: Value = response.json().await?;
        let task_id = body
            .get("result")
            .and_then(|result| result.get("task_id"))
            .and_then(first_non_empty_string);
        Ok(OzonProductCopyUpdateResult {
            offer_id,
            accepted: true,
            task_id,
        })
    }
}

fn parse_category_attributes(body: &Value) -> Vec<OzonCategoryAttribute> {
    let items = body
        .get("result")
        .and_then(Value::as_array)
        .or_else(|| body.as_array());
    let Some(items) = items else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_u64)?;
            let name = first_non_empty_string(&item["name"])?;
            Some(OzonCategoryAttribute {
                id,
                name,
                is_collection: item
                    .get("is_collection")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                dictionary_id: item
                    .get("dictionary_id")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                attribute_type: item
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

/// Parse `result.pictures[]` from a pictures/import response, falling back to
/// echoing the URLs we submitted (first = primary) when the body is sparse.
fn parse_picture_states(body: &Value, submitted: &[String]) -> Vec<OzonPictureState> {
    if let Some(items) = body
        .get("result")
        .and_then(|result| result.get("pictures"))
        .and_then(|pictures| pictures.as_array())
    {
        let parsed: Vec<OzonPictureState> = items
            .iter()
            .filter_map(|item| {
                let url = item.get("url").and_then(Value::as_str)?.to_string();
                Some(OzonPictureState {
                    url,
                    state: item
                        .get("state")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    is_primary: item.get("is_primary").and_then(Value::as_bool),
                })
            })
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    submitted
        .iter()
        .enumerate()
        .map(|(index, url)| OzonPictureState {
            url: url.clone(),
            state: Some("pending".to_string()),
            is_primary: Some(index == 0),
        })
        .collect()
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
        self.product_list_page_with_visibility(_credentials, limit, _last_id, None)
            .await
    }

    async fn product_list_page_with_visibility(
        &self,
        _credentials: &OzonCredentials,
        limit: u16,
        _last_id: Option<String>,
        visibility: Option<String>,
    ) -> Result<OzonProductListPage, OzonConnectorError> {
        let visibility = normalize_product_list_visibility(visibility)?;
        let archived = visibility == "ARCHIVED";
        let products = (0..limit.min(3))
            .map(|idx| OzonProductSummary {
                product_id: format!("mock-product-{idx}"),
                offer_id: format!("SKU-MOCK-{idx}"),
                name: Some(format!("Mock Ozon product {}", idx + 1)),
                visibility: Some(visibility.to_lowercase()),
                archived: Some(archived),
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

    async fn product_get(
        &self,
        _credentials: &OzonCredentials,
        lookup: OzonProductLookup,
    ) -> Result<OzonProductDetail, OzonConnectorError> {
        let lookup = validate_lookup(lookup)?;
        let idx = lookup
            .product_id
            .as_deref()
            .and_then(|value| value.strip_prefix("mock-product-"))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let offer_id = lookup
            .offer_id
            .clone()
            .unwrap_or_else(|| format!("SKU-MOCK-{idx}"));
        let product_id = lookup
            .product_id
            .clone()
            .unwrap_or_else(|| format!("mock-product-{idx}"));
        Ok(OzonProductDetail {
            lookup,
            product_id,
            offer_id,
            sku: Some(format!("MOCK-SKU-{idx}")),
            name: Some(format!("Mock Ozon product {}", idx + 1)),
            description_category_id: Some(10_001),
            type_id: Some(20_001),
            description: Some(format!("Mock Ozon description for product {}", idx + 1)),
            barcodes: vec![format!("46000000000{idx}")],
            primary_image: Some(format!(
                "https://cdn.example.test/ozon/mock-{idx}-primary.jpg"
            )),
            images: vec![
                OzonProductImage {
                    url: format!("https://cdn.example.test/ozon/mock-{idx}-primary.jpg"),
                    role: OzonProductImageRole::Primary,
                    position: 0,
                },
                OzonProductImage {
                    url: format!("https://cdn.example.test/ozon/mock-{idx}-gallery.jpg"),
                    role: OzonProductImageRole::Gallery,
                    position: 1,
                },
            ],
            gallery_images: vec![format!(
                "https://cdn.example.test/ozon/mock-{idx}-gallery.jpg"
            )],
            images360: vec![],
            color_image: None,
            attributes: vec![
                OzonProductAttribute {
                    id: Some(1),
                    name: Some("Brand".to_string()),
                    values: vec!["Mock Brand".to_string()],
                },
                OzonProductAttribute {
                    id: Some(2),
                    name: Some("Color".to_string()),
                    values: vec!["Graphite".to_string()],
                },
            ],
            visibility: Some("visible".to_string()),
            archived: Some(false),
            autoarchived: Some(false),
            created_at: None,
            updated_at: None,
            statuses: None,
            source_endpoints: vec![
                "mock:/v3/product/info/list".to_string(),
                "mock:/v4/product/info/attributes".to_string(),
            ],
            warnings: vec![],
        })
    }
}

#[async_trait]
impl OzonWriteConnector for MockOzonConnector {
    async fn pictures_import(
        &self,
        _credentials: &OzonCredentials,
        product_id: &str,
        images: Vec<String>,
    ) -> Result<OzonPicturesImport, OzonConnectorError> {
        if images.is_empty() {
            return Err(OzonConnectorError::InvalidProductLookup(
                "pictures_import requires at least one image URL".to_string(),
            ));
        }
        let pictures = images
            .iter()
            .enumerate()
            .map(|(index, url)| OzonPictureState {
                url: url.clone(),
                state: Some("imported".to_string()),
                is_primary: Some(index == 0),
            })
            .collect();
        Ok(OzonPicturesImport {
            product_id: product_id.to_string(),
            pictures,
        })
    }

    async fn description_category_attributes(
        &self,
        _credentials: &OzonCredentials,
        _description_category_id: u64,
        _type_id: u64,
    ) -> Result<Vec<OzonCategoryAttribute>, OzonConnectorError> {
        // Mirrors the two attributes the mock product_get returns: "Brand" is a
        // free-text attribute, "Color" is a dictionary-backed attribute.
        Ok(vec![
            OzonCategoryAttribute {
                id: 85,
                name: "Brand".to_string(),
                is_collection: false,
                dictionary_id: 0,
                attribute_type: Some("String".to_string()),
            },
            OzonCategoryAttribute {
                id: 10096,
                name: "Color".to_string(),
                is_collection: false,
                dictionary_id: 901,
                attribute_type: Some("String".to_string()),
            },
        ])
    }

    async fn description_category_attribute_values(
        &self,
        _credentials: &OzonCredentials,
        _description_category_id: u64,
        _type_id: u64,
        _attribute_id: u64,
    ) -> Result<Vec<OzonCategoryAttributeValue>, OzonConnectorError> {
        Ok(vec![
            OzonCategoryAttributeValue {
                value_id: 5001,
                value: "Graphite".to_string(),
            },
            OzonCategoryAttributeValue {
                value_id: 5002,
                value: "Silver".to_string(),
            },
        ])
    }

    async fn product_update_copy(
        &self,
        _credentials: &OzonCredentials,
        update: OzonProductCopyUpdate,
    ) -> Result<OzonProductCopyUpdateResult, OzonConnectorError> {
        if update.offer_id.trim().is_empty() {
            return Err(OzonConnectorError::InvalidProductLookup(
                "product_update_copy requires a non-empty offer_id".to_string(),
            ));
        }
        Ok(OzonProductCopyUpdateResult {
            offer_id: update.offer_id,
            accepted: true,
            task_id: Some("mock-import-task".to_string()),
        })
    }
}

#[derive(Debug, Error)]
pub enum OzonConnectorError {
    #[error("invalid Ozon API base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("invalid Ozon product lookup: {0}")]
    InvalidProductLookup(String),
    #[error("Ozon product not found for {0}")]
    ProductNotFound(String),
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
struct PicturesImportRequest {
    product_id: i64,
    images: Vec<String>,
    color_image: String,
    images360: Vec<String>,
}

#[derive(Serialize)]
struct CategoryAttributeRequest {
    description_category_id: u64,
    type_id: u64,
    language: &'static str,
}

#[derive(Serialize)]
struct CategoryAttributeValuesRequest {
    description_category_id: u64,
    type_id: u64,
    attribute_id: u64,
    limit: u32,
    last_value_id: u64,
}

#[derive(Deserialize)]
struct CategoryAttributeValuesResponse {
    #[serde(default)]
    result: Vec<CategoryAttributeValueItem>,
    #[serde(default)]
    has_next: bool,
}

#[derive(Deserialize)]
struct CategoryAttributeValueItem {
    id: u64,
    #[serde(default)]
    value: String,
}

#[derive(Serialize)]
struct ImportRequest {
    items: Vec<ImportItem>,
}

#[derive(Serialize)]
struct ImportItem {
    offer_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    attributes: Vec<ImportAttribute>,
}

#[derive(Serialize)]
struct ImportAttribute {
    id: u64,
    values: Vec<ImportAttributeValue>,
}

impl ImportAttribute {
    fn from_resolved(resolved: &OzonResolvedAttribute) -> Self {
        Self {
            id: resolved.attribute_id,
            values: resolved
                .values
                .iter()
                .map(|value| match value {
                    OzonResolvedValue::Dictionary { dictionary_value_id } => ImportAttributeValue {
                        dictionary_value_id: Some(*dictionary_value_id),
                        value: None,
                    },
                    OzonResolvedValue::FreeText { value } => ImportAttributeValue {
                        dictionary_value_id: None,
                        value: Some(value.clone()),
                    },
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct ImportAttributeValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    dictionary_value_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
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

#[derive(Serialize)]
struct ProductInfoListRequest {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    product_id: Vec<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    offer_id: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sku: Vec<u64>,
}

impl ProductInfoListRequest {
    fn from_lookup(lookup: &OzonProductLookup) -> Result<Self, OzonConnectorError> {
        Ok(Self {
            product_id: parse_optional_u64_vec(&lookup.product_id, "product_id")?,
            offer_id: lookup.offer_id.iter().cloned().collect(),
            sku: parse_optional_u64_vec(&lookup.sku, "sku")?,
        })
    }
}

#[derive(Deserialize)]
struct ProductInfoListResponse {
    #[serde(default)]
    items: Vec<ProductInfoItem>,
    #[serde(default)]
    result: Option<ProductInfoListResult>,
}

#[derive(Deserialize)]
struct ProductInfoListResult {
    #[serde(default)]
    items: Vec<ProductInfoItem>,
}

#[derive(Deserialize)]
struct ProductInfoItem {
    #[serde(default, alias = "product_id")]
    id: Option<u64>,
    #[serde(default)]
    offer_id: Option<String>,
    #[serde(default)]
    sku: Option<Value>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description_category_id: Option<u64>,
    #[serde(default)]
    type_id: Option<u64>,
    #[serde(default)]
    barcodes: Value,
    #[serde(default)]
    primary_image: Value,
    #[serde(default)]
    images: Value,
    #[serde(default)]
    images360: Value,
    #[serde(default)]
    color_image: Value,
    #[serde(default, alias = "is_archived")]
    archived: Option<bool>,
    #[serde(default, alias = "is_autoarchived")]
    autoarchived: Option<bool>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    statuses: Option<Value>,
}

#[derive(Serialize)]
struct ProductInfoAttributesRequest {
    filter: ProductInfoAttributesFilter,
    limit: u16,
    last_id: String,
}

impl ProductInfoAttributesRequest {
    fn from_lookup(lookup: &OzonProductLookup) -> Result<Option<Self>, OzonConnectorError> {
        if lookup.sku.is_some() && lookup.product_id.is_none() && lookup.offer_id.is_none() {
            return Ok(None);
        }
        Ok(Some(Self {
            filter: ProductInfoAttributesFilter {
                product_id: parse_optional_u64_vec(&lookup.product_id, "product_id")?,
                offer_id: lookup.offer_id.iter().cloned().collect(),
                visibility: "ALL".to_string(),
            },
            limit: 1,
            last_id: String::new(),
        }))
    }
}

#[derive(Serialize)]
struct ProductInfoDescriptionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    product_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offer_id: Option<String>,
}

impl ProductInfoDescriptionRequest {
    fn from_lookup(lookup: &OzonProductLookup) -> Result<Self, OzonConnectorError> {
        let product_id = parse_optional_u64_vec(&lookup.product_id, "product_id")?
            .into_iter()
            .next();
        let offer_id = lookup.offer_id.clone();
        if product_id.is_none() && offer_id.is_none() {
            return Err(OzonConnectorError::InvalidProductLookup(
                "description lookup requires product_id or offer_id".to_string(),
            ));
        }
        Ok(Self {
            product_id,
            offer_id,
        })
    }
}

#[derive(Serialize)]
struct ProductInfoAttributesFilter {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    product_id: Vec<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    offer_id: Vec<String>,
    visibility: String,
}

struct ProductAttributesSupplement {
    attributes: Vec<OzonProductAttribute>,
    primary_image: Option<String>,
    gallery_images: Vec<String>,
    images360: Vec<String>,
    color_image: Option<String>,
}

impl ProductAttributesSupplement {
    fn from_value(value: &Value) -> Self {
        Self {
            attributes: extract_attributes(value),
            primary_image: strings_from_value(&value["primary_image"])
                .into_iter()
                .next(),
            gallery_images: strings_from_value(&value["images"]),
            images360: strings_from_value(&value["images360"]),
            color_image: strings_from_value(&value["color_image"]).into_iter().next(),
        }
    }
}

fn detail_from_info_item(
    lookup: &OzonProductLookup,
    item: ProductInfoItem,
) -> Result<OzonProductDetail, OzonConnectorError> {
    let product_id = item
        .id
        .map(|value| value.to_string())
        .or_else(|| lookup.product_id.clone())
        .ok_or_else(|| OzonConnectorError::UnexpectedResponse("missing product id".to_string()))?;
    let offer_id = item
        .offer_id
        .clone()
        .or_else(|| lookup.offer_id.clone())
        .ok_or_else(|| OzonConnectorError::UnexpectedResponse("missing offer id".to_string()))?;
    let primary_image = strings_from_value(&item.primary_image).into_iter().next();
    let gallery_images = strings_from_value(&item.images);
    let images360 = strings_from_value(&item.images360);
    let color_image = strings_from_value(&item.color_image).into_iter().next();
    let mut images = Vec::new();
    push_images(
        &mut images,
        primary_image.iter(),
        OzonProductImageRole::Primary,
    );
    push_images(
        &mut images,
        gallery_images.iter(),
        OzonProductImageRole::Gallery,
    );
    push_images(&mut images, color_image.iter(), OzonProductImageRole::Color);
    push_images(&mut images, images360.iter(), OzonProductImageRole::Spin360);

    Ok(OzonProductDetail {
        lookup: lookup.clone(),
        product_id,
        offer_id,
        sku: item.sku.as_ref().and_then(first_non_empty_string),
        name: item.name,
        description_category_id: item.description_category_id,
        type_id: item.type_id,
        description: None,
        barcodes: strings_from_value(&item.barcodes),
        primary_image,
        images,
        gallery_images,
        images360,
        color_image,
        attributes: Vec::new(),
        visibility: item.visibility,
        archived: item.archived,
        autoarchived: item.autoarchived,
        created_at: item.created_at,
        updated_at: item.updated_at,
        statuses: item.statuses,
        source_endpoints: vec!["/v3/product/info/list".to_string()],
        warnings: vec![],
    })
}

fn apply_attribute_supplement(
    detail: &mut OzonProductDetail,
    supplement: ProductAttributesSupplement,
) {
    detail.attributes = supplement.attributes;
    if detail.primary_image.is_none() {
        detail.primary_image = supplement.primary_image;
    }
    merge_unique(&mut detail.gallery_images, supplement.gallery_images);
    merge_unique(&mut detail.images360, supplement.images360);
    if detail.color_image.is_none() {
        detail.color_image = supplement.color_image;
    }
    detail.images.clear();
    push_images(
        &mut detail.images,
        detail.primary_image.iter(),
        OzonProductImageRole::Primary,
    );
    push_images(
        &mut detail.images,
        detail.gallery_images.iter(),
        OzonProductImageRole::Gallery,
    );
    push_images(
        &mut detail.images,
        detail.color_image.iter(),
        OzonProductImageRole::Color,
    );
    push_images(
        &mut detail.images,
        detail.images360.iter(),
        OzonProductImageRole::Spin360,
    );
    detail
        .source_endpoints
        .push("/v4/product/info/attributes".to_string());
}

fn validate_lookup(lookup: OzonProductLookup) -> Result<OzonProductLookup, OzonConnectorError> {
    let lookup = lookup.normalized();
    if lookup.selected_identifier_count() != 1 {
        return Err(OzonConnectorError::InvalidProductLookup(
            "provide exactly one of product_id, offer_id, or sku".to_string(),
        ));
    }
    Ok(lookup)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_optional_u64_vec(
    value: &Option<String>,
    field: &str,
) -> Result<Vec<u64>, OzonConnectorError> {
    value
        .as_ref()
        .map(|value| {
            value
                .parse::<u64>()
                .map(|parsed| vec![parsed])
                .map_err(|_| {
                    OzonConnectorError::InvalidProductLookup(format!(
                        "{field} must be a positive integer"
                    ))
                })
        })
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn lookup_label(lookup: &OzonProductLookup) -> String {
    lookup
        .product_id
        .as_ref()
        .map(|value| format!("product_id={value}"))
        .or_else(|| {
            lookup
                .offer_id
                .as_ref()
                .map(|value| format!("offer_id={value}"))
        })
        .or_else(|| lookup.sku.as_ref().map(|value| format!("sku={value}")))
        .unwrap_or_else(|| "empty lookup".to_string())
}

/// Pull `result.description` (Ozon nests the payload under `result`) from a
/// `/v1/product/info/description` response, tolerating a flat shape too.
fn parse_product_description(body: &Value) -> Option<String> {
    let scope = body.get("result").unwrap_or(body);
    first_non_empty_string(&scope["description"])
}

fn first_attribute_item(value: &Value) -> Option<&Value> {
    let result = value.get("result").unwrap_or(value);
    result
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .or_else(|| result.as_array().and_then(|items| items.first()))
}

fn extract_attributes(value: &Value) -> Vec<OzonProductAttribute> {
    value
        .get("attributes")
        .and_then(Value::as_array)
        .map(|attributes| {
            attributes
                .iter()
                .map(|attribute| OzonProductAttribute {
                    id: attribute.get("id").and_then(Value::as_u64),
                    name: first_non_empty_string(&attribute["name"]),
                    values: extract_attribute_values(&attribute["values"]),
                })
                .filter(|attribute| {
                    attribute.id.is_some()
                        || attribute.name.is_some()
                        || !attribute.values.is_empty()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_attribute_values(value: &Value) -> Vec<String> {
    match value {
        Value::Array(values) => ordered_unique(
            values
                .iter()
                .filter_map(|value| {
                    first_non_empty_string(&value["value"])
                        .or_else(|| first_non_empty_string(&value["name"]))
                        .or_else(|| first_non_empty_string(value))
                })
                .collect(),
        ),
        _ => strings_from_value(value),
    }
}

fn strings_from_value(value: &Value) -> Vec<String> {
    match value {
        Value::Array(values) => {
            ordered_unique(values.iter().filter_map(first_non_empty_string).collect())
        }
        _ => first_non_empty_string(value).into_iter().collect(),
    }
}

fn first_non_empty_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => non_empty_string(value),
        Value::Number(value) => Some(value.to_string()),
        Value::Object(object) => object
            .get("url")
            .and_then(first_non_empty_string)
            .or_else(|| object.get("value").and_then(first_non_empty_string))
            .or_else(|| object.get("name").and_then(first_non_empty_string)),
        _ => None,
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn push_images<'a>(
    images: &mut Vec<OzonProductImage>,
    urls: impl IntoIterator<Item = &'a String>,
    role: OzonProductImageRole,
) {
    for url in urls {
        if images.iter().any(|image| image.url == *url) {
            continue;
        }
        images.push(OzonProductImage {
            url: url.clone(),
            role,
            position: images.len() as u16,
        });
    }
}

fn merge_unique(target: &mut Vec<String>, incoming: Vec<String>) {
    for value in incoming {
        if !target.iter().any(|existing| existing == &value) {
            target.push(value);
        }
    }
}

fn ordered_unique(values: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    for value in values {
        if !output.iter().any(|existing| existing == &value) {
            output.push(value);
        }
    }
    output
}

fn sanitize_api_error_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(500).collect()
}

fn normalize_product_list_visibility(
    visibility: Option<String>,
) -> Result<String, OzonConnectorError> {
    let Some(visibility) = visibility else {
        return Ok(ProductListFilter::default().visibility);
    };
    let visibility = visibility.trim().to_ascii_uppercase();
    if visibility.is_empty() {
        return Ok(ProductListFilter::default().visibility);
    }
    if !visibility
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        return Err(OzonConnectorError::UnexpectedResponse(format!(
            "invalid product list visibility: {visibility}"
        )));
    }
    Ok(visibility)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_payload_shape_is_stable() {
        // Locks the live-store /v3/product/import wire contract: offer_id key,
        // name omitted when absent, attributes [{id, values:[{dictionary_value_id}|{value}]}],
        // and description riding as the annotation attribute (id 4191).
        let request = ImportRequest {
            items: vec![ImportItem {
                offer_id: "OF-1".to_string(),
                name: Some("Новый заголовок".to_string()),
                attributes: vec![
                    ImportAttribute::from_resolved(&OzonResolvedAttribute {
                        attribute_id: 85,
                        values: vec![OzonResolvedValue::Dictionary {
                            dictionary_value_id: 999,
                        }],
                    }),
                    ImportAttribute::from_resolved(&OzonResolvedAttribute {
                        attribute_id: OZON_DESCRIPTION_ATTRIBUTE_ID,
                        values: vec![OzonResolvedValue::FreeText {
                            value: "Описание".to_string(),
                        }],
                    }),
                ],
            }],
        };
        let actual = serde_json::to_value(&request).expect("serialize import request");
        let expected = serde_json::json!({
            "items": [{
                "offer_id": "OF-1",
                "name": "Новый заголовок",
                "attributes": [
                    { "id": 85, "values": [{ "dictionary_value_id": 999 }] },
                    { "id": 4191, "values": [{ "value": "Описание" }] }
                ]
            }]
        });
        assert_eq!(actual, expected);

        // name is omitted entirely when absent (never write an empty title).
        let bare = ImportRequest {
            items: vec![ImportItem {
                offer_id: "OF-2".to_string(),
                name: None,
                attributes: vec![],
            }],
        };
        assert_eq!(
            serde_json::to_value(&bare).expect("serialize bare import"),
            serde_json::json!({ "items": [{ "offer_id": "OF-2", "attributes": [] }] })
        );
    }

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

    #[tokio::test]
    async fn mock_product_get_returns_stable_images_and_attributes() {
        let connector = MockOzonConnector;
        let credentials = OzonCredentials {
            client_id: "mock-client-id".to_string(),
            api_key: SecretString::from("mock-api-key"),
        };

        let detail = connector
            .product_get(
                &credentials,
                OzonProductLookup {
                    product_id: Some("mock-product-1".to_string()),
                    offer_id: None,
                    sku: None,
                },
            )
            .await
            .expect("mock detail");

        assert_eq!(detail.product_id, "mock-product-1");
        assert_eq!(detail.images.len(), 2);
        assert_eq!(detail.images[0].role, OzonProductImageRole::Primary);
        assert_eq!(detail.attributes.len(), 2);
    }

    #[test]
    fn product_info_item_builds_deduped_image_order() {
        let item: ProductInfoItem = serde_json::from_value(serde_json::json!({
            "id": 42,
            "offer_id": "SKU-42",
            "name": "Example",
            "barcodes": ["4601"],
            "primary_image": ["https://cdn.example/p.jpg"],
            "images": ["https://cdn.example/p.jpg", "https://cdn.example/g.jpg"],
            "color_image": "https://cdn.example/c.jpg",
            "images360": [{"url": "https://cdn.example/s.jpg"}]
        }))
        .expect("product info item");
        let detail = detail_from_info_item(
            &OzonProductLookup {
                product_id: Some("42".to_string()),
                offer_id: None,
                sku: None,
            },
            item,
        )
        .expect("detail");

        assert_eq!(
            detail.primary_image.as_deref(),
            Some("https://cdn.example/p.jpg")
        );
        assert_eq!(detail.images.len(), 4);
        assert_eq!(detail.images[0].role, OzonProductImageRole::Primary);
        assert_eq!(detail.images[1].role, OzonProductImageRole::Gallery);
        assert_eq!(detail.images[2].role, OzonProductImageRole::Color);
        assert_eq!(detail.images[3].role, OzonProductImageRole::Spin360);
    }

    #[test]
    fn product_description_parses_nested_and_flat_shapes() {
        let nested = serde_json::json!({
            "result": { "id": 42, "offer_id": "SKU", "description": "Nested desc" }
        });
        assert_eq!(
            parse_product_description(&nested).as_deref(),
            Some("Nested desc")
        );

        let flat = serde_json::json!({ "description": "Flat desc" });
        assert_eq!(parse_product_description(&flat).as_deref(), Some("Flat desc"));

        let empty = serde_json::json!({ "result": { "description": "  " } });
        assert_eq!(parse_product_description(&empty), None);
    }

    #[test]
    fn product_lookup_requires_one_identifier() {
        let error = validate_lookup(OzonProductLookup {
            product_id: Some("1".to_string()),
            offer_id: Some("SKU".to_string()),
            sku: None,
        })
        .expect_err("ambiguous lookup");

        assert!(matches!(error, OzonConnectorError::InvalidProductLookup(_)));
    }

    #[test]
    fn api_error_body_is_compacted_and_bounded() {
        let body = format!("{}\n{}", "x".repeat(600), "secret-looking-noise");
        let sanitized = sanitize_api_error_body(&body);

        assert_eq!(sanitized.chars().count(), 500);
        assert!(!sanitized.contains('\n'));
    }
}
