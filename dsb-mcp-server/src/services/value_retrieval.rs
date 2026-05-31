// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Value Retrieval Service for knowledge and entity search via MCP tools.
//!
//! This service provides MCP tools for searching knowledge bases and entity
//! names using vector embeddings, Milvus vector search, and reranking.
//! It orchestrates the full pipeline: embed query -> search Milvus -> rerank results.
//!
//! The service mirrors the Python implementation at `value_retrieval/retrieval.py`,
//! providing `search_for_knowledge` and `search_for_partner_name` tools.

use crate::dsb_client::DSBClient;
use crate::session::SessionManager;
use crate::settings::{KnowledgeRetrievalSettings, Settings, ValueRetrievalSettings};
use reqwest::Client;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParam, CallToolResult, ErrorCode, ErrorData, Implementation,
        InitializeRequestParam, InitializeResult, ListToolsResult, PaginatedRequestParam,
        ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info};

// ========== Service Definition ==========

/// Value retrieval service providing knowledge and entity search MCP tools.
///
/// Implements a multi-stage retrieval pipeline:
/// 1. Embed the query using a configured embedding API
/// 2. Search Milvus for candidate passages via vector similarity
/// 3. Rerank results using a configured reranking API
/// 4. Filter by relevance threshold and return top results
#[derive(Debug, Clone)]
pub struct ValueRetrievalService {
    settings: Arc<Settings>,
    http_client: Client,
    tool_router: ToolRouter<ValueRetrievalService>,
}

impl ValueRetrievalService {
    /// Create a new value retrieval service.
    pub fn new(
        _dsb_client: Arc<DSBClient>,
        _session_manager: Arc<SessionManager>,
        settings: Arc<Settings>,
    ) -> Self {
        let http_client = Client::new();
        Self {
            settings,
            http_client,
            tool_router: Self::tool_router(),
        }
    }
}

// ========== Internal Types ==========

/// Response from an embedding API call.
#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

/// Single embedding result containing the vector.
#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f64>,
}

/// Response from a Milvus vector search.
#[derive(Debug, Deserialize)]
struct MilvusSearchResponse {
    data: Vec<MilvusSearchResult>,
}

/// Single result from a Milvus search.
#[derive(Debug, Deserialize, Serialize)]
struct MilvusSearchResult {
    #[serde(flatten)]
    fields: serde_json::Map<String, serde_json::Value>,
}

/// Response from a reranking API call.
#[derive(Debug, Deserialize)]
struct RerankResponse {
    results: Vec<RerankResult>,
}

/// Single result from reranking with index and score.
#[derive(Debug, Deserialize)]
struct RerankResult {
    index: usize,
    relevance_score: f64,
}

/// Owned snapshot of retrieval configuration values.
///
/// Captures all needed settings into owned `String`s and `Vec<String>` so the
/// snapshot is `Send + Sync` and can be held across `.await` points in async
/// tool handlers without triggering `dyn Trait` send-safety issues.
#[derive(Debug, Clone)]
struct RetrievalSnapshot {
    milvus_url: String,
    collection_name: String,
    search_limit: u32,
    rerank_threshold: f64,
    embedding_model: String,
    embedding_api_url: String,
    embedding_api_key: Option<String>,
    rerank_model: String,
    rerank_api_url: String,
    rerank_api_key: Option<String>,
    search_instruction: String,
    output_fields: Vec<String>,
    doc_field: String,
}

impl RetrievalSnapshot {
    /// Capture a snapshot from knowledge retrieval settings.
    fn from_knowledge(cfg: &KnowledgeRetrievalSettings) -> Self {
        Self {
            milvus_url: cfg.milvus_url.clone(),
            collection_name: cfg.collection_name.clone(),
            search_limit: cfg.search_limit,
            rerank_threshold: cfg.rerank_threshold,
            embedding_model: cfg.embedding_model.clone(),
            embedding_api_url: cfg.embedding_api_url.clone(),
            embedding_api_key: cfg.embedding_api_key.clone(),
            rerank_model: cfg.rerank_model.clone(),
            rerank_api_url: cfg.rerank_api_url.clone(),
            rerank_api_key: cfg.rerank_api_key.clone(),
            search_instruction: cfg.search_instruction.clone(),
            output_fields: cfg.output_fields.clone(),
            doc_field: cfg.doc.clone(),
        }
    }

    /// Capture a snapshot from value retrieval settings.
    fn from_value(cfg: &ValueRetrievalSettings) -> Self {
        Self {
            milvus_url: cfg.milvus_url.clone(),
            collection_name: cfg.collection_name.clone(),
            search_limit: cfg.search_limit,
            rerank_threshold: cfg.rerank_threshold,
            embedding_model: cfg.embedding_model.clone(),
            embedding_api_url: cfg.embedding_api_url.clone(),
            embedding_api_key: cfg.embedding_api_key.clone(),
            rerank_model: cfg.rerank_model.clone(),
            rerank_api_url: cfg.rerank_api_url.clone(),
            rerank_api_key: cfg.rerank_api_key.clone(),
            search_instruction: cfg.search_instruction.clone(),
            output_fields: cfg.output_fields.clone(),
            doc_field: cfg.doc.clone(),
        }
    }
}

// ========== Tool Argument Schemas ==========

/// Arguments for searching knowledge passages.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchForKnowledgeArgs {
    /// The search query to find relevant knowledge passages.
    pub query: String,
    /// Maximum number of results to return. If not provided, uses the configured default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Arguments for searching partner/entity names.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchForPartnerNameArgs {
    /// The search query to find relevant entity names.
    pub query: String,
    /// Maximum number of results to return. If not provided, uses the configured default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

// ========== Tool Router ==========

#[tool_router]
impl ValueRetrievalService {
    #[tool(
        description = "Search for relevant knowledge passages using vector similarity and reranking. Embeds the query, searches a Milvus collection, reranks results, and returns the most relevant passages."
    )]
    async fn search_for_knowledge(
        &self,
        Parameters(SearchForKnowledgeArgs { query, limit }): Parameters<SearchForKnowledgeArgs>,
    ) -> Result<String, ErrorData> {
        info!(query = %query, "Searching for knowledge");

        let cfg = RetrievalSnapshot::from_knowledge(&self.settings.knowledge_retrieval);
        let limit = limit.unwrap_or(cfg.search_limit);

        let results = self.hybrid_retrieve(&query, limit, &cfg).await?;

        let result = serde_json::json!({
            "query": query,
            "results": results,
            "total": results.len(),
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "search_for_knowledge",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Search for partner or entity names using vector similarity and reranking. Embeds the query, searches a Milvus collection, reranks results, and returns the most relevant entity names."
    )]
    async fn search_for_partner_name(
        &self,
        Parameters(SearchForPartnerNameArgs { query, limit }): Parameters<SearchForPartnerNameArgs>,
    ) -> Result<String, ErrorData> {
        info!(query = %query, "Searching for partner name");

        let cfg = RetrievalSnapshot::from_value(&self.settings.value_retrieval);
        let limit = limit.unwrap_or(cfg.search_limit);

        let results = self.hybrid_retrieve(&query, limit, &cfg).await?;

        let result = serde_json::json!({
            "query": query,
            "results": results,
            "total": results.len(),
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "search_for_partner_name",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }
}

// ========== Private Helpers ==========

impl ValueRetrievalService {
    /// Execute the full retrieval pipeline: embed -> search Milvus -> rerank -> filter.
    ///
    /// This is the internal helper that both MCP tools delegate to, parameterized
    /// by the appropriate retrieval configuration snapshot.
    async fn hybrid_retrieve(
        &self,
        query: &str,
        limit: u32,
        cfg: &RetrievalSnapshot,
    ) -> Result<Vec<serde_json::Value>, ErrorData> {
        // Step 1: Embed the query
        debug!(model = %cfg.embedding_model, "Embedding query");
        let embedding = self.embed_query(query, cfg).await?;

        // Step 2: Search Milvus
        debug!(
            collection = %cfg.collection_name,
            limit = limit,
            "Searching Milvus"
        );
        let search_results = self.search_milvus(&embedding, limit, cfg).await?;

        if search_results.is_empty() {
            info!("No results found in Milvus");
            return Ok(vec![]);
        }

        // Step 3: Extract documents for reranking
        let doc_field = &cfg.doc_field;
        let documents: Vec<String> = search_results
            .iter()
            .filter_map(|r| {
                r.fields
                    .get(doc_field.as_str())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        if documents.is_empty() {
            info!("No documents to rerank");
            return Ok(search_results
                .into_iter()
                .map(|r| serde_json::Value::Object(r.fields))
                .collect());
        }

        // Step 4: Rerank
        debug!(model = %cfg.rerank_model, "Reranking results");
        let reranked = self.rerank(query, &documents, cfg).await?;

        // Step 5: Filter by threshold and build results
        let threshold = cfg.rerank_threshold;
        let output_fields = &cfg.output_fields;

        let results: Vec<serde_json::Value> = reranked
            .into_iter()
            .filter(|r| r.relevance_score >= threshold)
            .filter_map(|r| {
                search_results.get(r.index).map(|sr| {
                    let mut obj = serde_json::Map::new();
                    for field in output_fields {
                        if let Some(val) = sr.fields.get(field) {
                            obj.insert(field.clone(), val.clone());
                        }
                    }
                    obj.insert(
                        "relevance_score".to_string(),
                        serde_json::json!(r.relevance_score),
                    );
                    serde_json::Value::Object(obj)
                })
            })
            .collect();

        info!(
            total = search_results.len(),
            returned = results.len(),
            "Retrieval complete"
        );

        Ok(results)
    }

    /// Call the embedding API to get a vector representation of the query.
    async fn embed_query(
        &self,
        query: &str,
        cfg: &RetrievalSnapshot,
    ) -> Result<Vec<f64>, ErrorData> {
        let mut request_body = serde_json::json!({
            "model": cfg.embedding_model,
            "input": query,
        });

        // Use instruction-prefixed query if configured
        if !cfg.search_instruction.is_empty() {
            request_body["input"] =
                serde_json::json!(format!("{}: {}", cfg.search_instruction, query));
        }

        let mut builder = self
            .http_client
            .post(&cfg.embedding_api_url)
            .json(&request_body);

        if let Some(api_key) = &cfg.embedding_api_key {
            builder = builder.bearer_auth(api_key);
        }

        let response = builder.send().await.map_err(|e| {
            error!(error = %e, "Failed to call embedding API");
            ErrorData::internal_error(
                "embed_query",
                Some(serde_json::json!(format!(
                    "Embedding API request failed: {}",
                    e
                ))),
            )
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Embedding API error");
            return Err(ErrorData::internal_error(
                "embed_query",
                Some(serde_json::json!(format!(
                    "Embedding API returned status {}: {}",
                    status, body
                ))),
            ));
        }

        let embedding_response: EmbeddingResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse embedding response");
            ErrorData::internal_error(
                "embed_query",
                Some(serde_json::json!(format!(
                    "Failed to parse embedding response: {}",
                    e
                ))),
            )
        })?;

        embedding_response
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| {
                ErrorData::internal_error(
                    "embed_query",
                    Some(serde_json::json!("No embedding returned from API")),
                )
            })
    }

    /// Search Milvus for vector-similar documents.
    async fn search_milvus(
        &self,
        embedding: &[f64],
        limit: u32,
        cfg: &RetrievalSnapshot,
    ) -> Result<Vec<MilvusSearchResult>, ErrorData> {
        let search_url = format!("{}/v2/vectordb/entities/search", cfg.milvus_url);

        let request_body = serde_json::json!({
            "collectionName": cfg.collection_name,
            "data": [embedding],
            "limit": limit,
            "outputFields": cfg.output_fields,
        });

        let response = self
            .http_client
            .post(&search_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to call Milvus search");
                ErrorData::internal_error(
                    "search_milvus",
                    Some(serde_json::json!(format!(
                        "Milvus search request failed: {}",
                        e
                    ))),
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Milvus search error");
            return Err(ErrorData::internal_error(
                "search_milvus",
                Some(serde_json::json!(format!(
                    "Milvus search returned status {}: {}",
                    status, body
                ))),
            ));
        }

        let search_response: MilvusSearchResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse Milvus response");
            ErrorData::internal_error(
                "search_milvus",
                Some(serde_json::json!(format!(
                    "Failed to parse Milvus response: {}",
                    e
                ))),
            )
        })?;

        Ok(search_response.data)
    }

    /// Rerank documents against a query using the reranking API.
    async fn rerank(
        &self,
        query: &str,
        documents: &[String],
        cfg: &RetrievalSnapshot,
    ) -> Result<Vec<RerankResult>, ErrorData> {
        let request_body = serde_json::json!({
            "model": cfg.rerank_model,
            "query": query,
            "documents": documents,
        });

        let mut builder = self
            .http_client
            .post(&cfg.rerank_api_url)
            .json(&request_body);

        if let Some(api_key) = &cfg.rerank_api_key {
            builder = builder.bearer_auth(api_key);
        }

        let response = builder.send().await.map_err(|e| {
            error!(error = %e, "Failed to call reranking API");
            ErrorData::internal_error(
                "rerank",
                Some(serde_json::json!(format!(
                    "Reranking API request failed: {}",
                    e
                ))),
            )
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Reranking API error");
            return Err(ErrorData::internal_error(
                "rerank",
                Some(serde_json::json!(format!(
                    "Reranking API returned status {}: {}",
                    status, body
                ))),
            ));
        }

        let rerank_response: RerankResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse reranking response");
            ErrorData::internal_error(
                "rerank",
                Some(serde_json::json!(format!(
                    "Failed to parse reranking response: {}",
                    e
                ))),
            )
        })?;

        Ok(rerank_response.results)
    }
}

// ========== ServerHandler Implementation ==========

impl ServerHandler for ValueRetrievalService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "dsb-value-retrieval-service".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "DSB Value Retrieval Service - Search for knowledge passages and entity names using vector similarity and reranking.".to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self.tool_router.list_all();
        tracing::info!(
            "list_tools: returning {} value retrieval tools",
            tools.len()
        );
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: Default::default(),
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = &request.name;
        let tool_route = self.tool_router.map.get(tool_name).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Tool not found: {}", tool_name),
                None,
            )
        })?;

        let tool_ctx = ToolCallContext::new(self, request.clone(), ctx);
        (tool_route.call)(tool_ctx).await
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_service() -> ValueRetrievalService {
        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(settings);
        ValueRetrievalService::new(dsb_client, session_manager, settings)
    }

    #[test]
    fn test_search_for_knowledge_args_deserialization() {
        let json = r#"{
            "query": "what is machine learning",
            "limit": 5
        }"#;
        let args: SearchForKnowledgeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.query, "what is machine learning");
        assert_eq!(args.limit, Some(5));
    }

    #[test]
    fn test_search_for_knowledge_args_minimal() {
        let json = r#"{"query": "hello world"}"#;
        let args: SearchForKnowledgeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.query, "hello world");
        assert!(args.limit.is_none());
    }

    #[test]
    fn test_search_for_knowledge_args_missing_query_fails() {
        let json = r#"{"limit": 5}"#;
        let result = serde_json::from_str::<SearchForKnowledgeArgs>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_for_partner_name_args_deserialization() {
        let json = r#"{
            "query": "Acme Corp",
            "limit": 10
        }"#;
        let args: SearchForPartnerNameArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.query, "Acme Corp");
        assert_eq!(args.limit, Some(10));
    }

    #[test]
    fn test_search_for_partner_name_args_minimal() {
        let json = r#"{"query": "test entity"}"#;
        let args: SearchForPartnerNameArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.query, "test entity");
        assert!(args.limit.is_none());
    }

    #[test]
    fn test_search_for_partner_name_args_missing_query_fails() {
        let json = r#"{}"#;
        let result = serde_json::from_str::<SearchForPartnerNameArgs>(json);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_2_tools_registered() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert_eq!(tools.len(), 2, "Should have exactly 2 tools registered");

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(tool_names.contains(&"search_for_knowledge".to_string()));
        assert!(tool_names.contains(&"search_for_partner_name".to_string()));
    }

    #[tokio::test]
    async fn test_server_handler_info() {
        let service = create_test_service();
        let info = service.get_info();
        assert_eq!(info.server_info.name, "dsb-value-retrieval-service");
    }

    #[test]
    fn test_retrieval_snapshot_from_knowledge() {
        let settings = Settings::load_for_tests().unwrap();
        let cfg = RetrievalSnapshot::from_knowledge(&settings.knowledge_retrieval);
        assert_eq!(cfg.collection_name, "deep_research_knowledge");
        assert_eq!(cfg.embedding_model, "qwen-embedding-0.6");
        assert_eq!(cfg.rerank_model, "qwen-reranker-0.6");
        assert_eq!(cfg.search_limit, 10);
        assert!(!cfg.output_fields.is_empty());
    }

    #[test]
    fn test_retrieval_snapshot_from_value() {
        let settings = Settings::load_for_tests().unwrap();
        let cfg = RetrievalSnapshot::from_value(&settings.value_retrieval);
        assert_eq!(cfg.collection_name, "nl2sql_value_retrieval_ft");
        assert_eq!(cfg.embedding_model, "qwen-embedding-0.6");
        assert_eq!(cfg.rerank_model, "qwen-reranker-0.6");
        assert!(!cfg.output_fields.is_empty());
    }
}
