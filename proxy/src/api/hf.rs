use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;

// ---------------------------------------------------------------------------
// Download state — shared across handlers and background tasks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DownloadState {
    pub id: String,
    pub hf_repo: String,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub status: DownloadStatus,
    pub error: Option<String>,
    pub category_id: Option<String>,
    pub backend_type: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Downloading,
    Complete,
    Failed,
    Cancelled,
}

pub type Downloads = Arc<RwLock<HashMap<String, DownloadState>>>;

// ---------------------------------------------------------------------------
// Shared state wrapper — holds Downloads + a handle to AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct HfState {
    pub app: Arc<AppState>,
    pub downloads: Downloads,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn routes(state: Arc<AppState>) -> Router {
    let hf_state = HfState {
        app: state,
        downloads: Arc::new(RwLock::new(HashMap::new())),
    };

    Router::new()
        .route("/search", get(search_models))
        .route("/files", get(list_repo_files))
        .route("/download", post(start_download))
        .route("/downloads", get(list_downloads))
        .route("/downloads/{id}", delete(cancel_download))
        .with_state(hf_state)
}

// ---------------------------------------------------------------------------
// GET /search?q=<query>&task=<task>
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
    task: Option<String>,
    tags: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HfModelResult {
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    id: Option<String>,
    downloads: Option<u64>,
    likes: Option<u64>,
    #[serde(rename = "pipeline_tag")]
    pipeline_tag: Option<String>,
    tags: Option<Vec<String>>,
}

async fn search_models(
    State(_state): State<HfState>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    let mut query = params.q.unwrap_or_default();
    let task = params.task.unwrap_or_else(|| "text-generation".to_string());
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(20).min(100);

    // When GGUF filter is active, add "GGUF" to the search so HF ranks GGUF repos first
    if params.tags.as_deref() == Some("gguf") && !query.to_lowercase().contains("gguf") {
        query = format!("{} GGUF", query);
    }

    // Request extra from HF to compensate for client-side GGUF name filtering
    let hf_limit = if params.tags.as_deref() == Some("gguf") {
        (limit + offset) * 3
    } else {
        limit + offset
    };

    let mut url = format!(
        "https://huggingface.co/api/models?search={}&pipeline_tag={}&sort=downloads&direction=-1&limit={}",
        urlencoded(&query),
        urlencoded(&task),
        hf_limit,
    );

    if let Some(ref tags) = params.tags {
        for tag in tags.split(',') {
            url.push_str(&format!("&tags={}", urlencoded(tag.trim())));
        }
    }

    let client = match reqwest::Client::builder()
        .user_agent("sovereign-engine/0.1")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return super::error::internal_error("hf:build_http_client", e);
        }
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(
                    serde_json::json!({ "error": format!("HuggingFace API request failed: {e}") }),
                ),
            )
                .into_response();
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": format!("HuggingFace API returned {status}: {body}")
            })),
        )
            .into_response();
    }

    let hf_models: Vec<HfModelResult> = match resp.json().await {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to parse HuggingFace response: {e}") })),
            )
                .into_response();
        }
    };

    let require_gguf_name = params.tags.as_deref() == Some("gguf");

    let all_models: Vec<serde_json::Value> = hf_models
        .into_iter()
        .filter_map(|m| {
            let id = m.model_id.or(m.id).unwrap_or_default();
            // When GGUF filter is active, only show repos with GGUF in the name
            // (HF's tag filter is too loose — includes repos that merely contain GGUF files)
            if require_gguf_name && !id.to_lowercase().contains("gguf") {
                return None;
            }
            Some(serde_json::json!({
                "id": id,
                "downloads": m.downloads.unwrap_or(0),
                "likes": m.likes.unwrap_or(0),
                "pipeline_tag": m.pipeline_tag,
                "tags": m.tags.unwrap_or_default(),
            }))
        })
        .collect();

    let has_more = all_models.len() > offset + limit;
    let models: Vec<serde_json::Value> = all_models.into_iter().skip(offset).take(limit).collect();

    Json(serde_json::json!({ "models": models, "has_more": has_more })).into_response()
}

// ---------------------------------------------------------------------------
// GET /files?repo=<repo>
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FilesQuery {
    repo: String,
}

async fn list_repo_files(
    State(_state): State<HfState>,
    Query(params): Query<FilesQuery>,
) -> impl IntoResponse {
    let client = match reqwest::Client::builder()
        .user_agent("sovereign-engine/0.1")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return super::error::internal_error("hf:build_http_client", e);
        }
    };

    if let Some(r) = super::error::validate_len("repo", &params.repo, super::error::MAX_NAME) {
        return r;
    }
    if let Some(r) = super::error::validate_hf_repo(&params.repo) {
        return r;
    }

    let tree_url = format!(
        "https://huggingface.co/api/models/{}/tree/main",
        params.repo
    );

    let resp = match client.get(&tree_url).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(
                    serde_json::json!({ "error": format!("HuggingFace API request failed: {e}") }),
                ),
            )
                .into_response();
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("HuggingFace API returned {status}: {body}") })),
        )
            .into_response();
    }

    let files: Vec<HfFileEntry> = match resp.json().await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to parse file listing: {e}") })),
            )
                .into_response();
        }
    };

    let skip_files = [".gitattributes", ".gitignore", ".git"];
    let file_list: Vec<serde_json::Value> = files
        .iter()
        .filter(|f| f.file_type == "file" && !skip_files.iter().any(|s| f.path.starts_with(s)))
        .map(|f| {
            serde_json::json!({
                "path": f.path,
                "size": f.size.unwrap_or(0),
            })
        })
        .collect();

    Json(serde_json::json!({ "files": file_list })).into_response()
}

// ---------------------------------------------------------------------------
// POST /download
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DownloadRequest {
    hf_repo: String,
    /// Optional list of specific files to download (e.g. a single GGUF file)
    files: Option<Vec<String>>,
    category_id: Option<String>,
    backend_type: Option<String>,
}

async fn start_download(
    State(state): State<HfState>,
    Json(req): Json<DownloadRequest>,
) -> impl IntoResponse {
    if let Some(r) = super::error::validate_len("hf_repo", &req.hf_repo, super::error::MAX_NAME) {
        return r;
    }
    if let Some(r) = super::error::validate_hf_repo(&req.hf_repo) {
        return r;
    }
    // Check disk space before starting
    let model_path = &state.app.config.model_path;
    match get_disk_usage(model_path) {
        Ok(disk) => {
            let usage_pct = if disk.total_bytes > 0 {
                (disk.used_bytes as f64 / disk.total_bytes as f64) * 100.0
            } else {
                0.0
            };
            if usage_pct >= 95.0 {
                return (
                    StatusCode::INSUFFICIENT_STORAGE,
                    Json(serde_json::json!({
                        "error": format!(
                            "Disk usage at {:.1}% — downloads blocked above 95%",
                            usage_pct
                        )
                    })),
                )
                    .into_response();
            }
            if usage_pct >= 90.0 {
                warn!(
                    usage_pct = format!("{:.1}%", usage_pct),
                    "Disk usage above 90% warning threshold"
                );
            }
        }
        Err(e) => {
            warn!("Could not check disk usage: {e}");
            // Continue anyway — don't block downloads if df fails
        }
    }

    // Check if we're already downloading this repo
    {
        let downloads = state.downloads.read().await;
        for dl in downloads.values() {
            if dl.hf_repo == req.hf_repo && dl.status == DownloadStatus::Downloading {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": format!("Download already in progress for {}", req.hf_repo),
                        "download_id": dl.id,
                    })),
                )
                    .into_response();
            }
        }
    }

    let download_id = Uuid::new_v4().to_string();

    // Create initial download state
    let dl_state = DownloadState {
        id: download_id.clone(),
        hf_repo: req.hf_repo.clone(),
        progress_bytes: 0,
        total_bytes: 0,
        status: DownloadStatus::Downloading,
        error: None,
        category_id: req.category_id.clone(),
        backend_type: req
            .backend_type
            .clone()
            .unwrap_or_else(|| "llamacpp".to_string()),
    };

    {
        let mut downloads = state.downloads.write().await;
        downloads.insert(download_id.clone(), dl_state);
    }

    // Spawn background download task
    let downloads = state.downloads.clone();
    let app_state = state.app.clone();
    let hf_repo = req.hf_repo.clone();
    let file_filter = req.files.clone();
    let category_id = req.category_id.clone();
    let backend_type = req.backend_type.clone();
    let dl_id = download_id.clone();

    tokio::spawn(async move {
        run_download(
            app_state,
            downloads,
            dl_id,
            hf_repo,
            file_filter,
            category_id,
            backend_type,
        )
        .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "download_id": download_id,
            "status": "started",
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /downloads
// ---------------------------------------------------------------------------

async fn list_downloads(State(state): State<HfState>) -> impl IntoResponse {
    let downloads = state.downloads.read().await;
    let data: Vec<serde_json::Value> = downloads
        .values()
        .map(|dl| {
            serde_json::json!({
                "id": dl.id,
                "hf_repo": dl.hf_repo,
                "progress_bytes": dl.progress_bytes,
                "total_bytes": dl.total_bytes,
                "status": dl.status,
                "error": dl.error,
            })
        })
        .collect();

    Json(serde_json::json!({ "downloads": data }))
}

// ---------------------------------------------------------------------------
// DELETE /downloads/:id
// ---------------------------------------------------------------------------

async fn cancel_download(
    State(state): State<HfState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut downloads = state.downloads.write().await;

    match downloads.get_mut(&id) {
        Some(dl) => {
            if dl.status == DownloadStatus::Downloading {
                dl.status = DownloadStatus::Cancelled;
                info!(download_id = %id, "Download cancelled");
                Json(serde_json::json!({ "status": "cancelled" })).into_response()
            } else {
                (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": format!("Download is not active (status: {:?})", dl.status)
                    })),
                )
                    .into_response()
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Download not found" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Background download task
// ---------------------------------------------------------------------------

async fn run_download(
    app_state: Arc<AppState>,
    downloads: Downloads,
    download_id: String,
    hf_repo: String,
    file_filter: Option<Vec<String>>,
    category_id: Option<String>,
    backend_type: Option<String>,
) {
    info!(hf_repo = %hf_repo, download_id = %download_id, "Starting model download");

    let hf_token = std::env::var("HF_TOKEN").ok();

    let mut client_builder = reqwest::Client::builder().user_agent("sovereign-engine/0.1");

    if let Some(ref token) = hf_token {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
            headers.insert(reqwest::header::AUTHORIZATION, val);
        }
        client_builder = client_builder.default_headers(headers);
    }

    let client = match client_builder.build() {
        Ok(c) => c,
        Err(e) => {
            set_download_error(&downloads, &download_id, &format!("HTTP client error: {e}")).await;
            return;
        }
    };

    // Step 1: List files in the repo
    let tree_url = format!("https://huggingface.co/api/models/{}/tree/main", hf_repo);
    let tree_resp = match client.get(&tree_url).send().await {
        Ok(r) => r,
        Err(e) => {
            set_download_error(
                &downloads,
                &download_id,
                &format!("Failed to list repo files: {e}"),
            )
            .await;
            return;
        }
    };

    if !tree_resp.status().is_success() {
        let status = tree_resp.status();
        let body = tree_resp.text().await.unwrap_or_default();
        set_download_error(
            &downloads,
            &download_id,
            &format!("HuggingFace tree API returned {status}: {body}"),
        )
        .await;
        return;
    }

    let files: Vec<HfFileEntry> = match tree_resp.json().await {
        Ok(f) => f,
        Err(e) => {
            set_download_error(
                &downloads,
                &download_id,
                &format!("Failed to parse file listing: {e}"),
            )
            .await;
            return;
        }
    };

    // Filter to downloadable files (skip directories and git metadata)
    let skip_files = [".gitattributes", ".gitignore", ".git"];
    let downloadable: Vec<&HfFileEntry> = files
        .iter()
        .filter(|f| {
            if f.file_type != "file" {
                return false;
            }
            if skip_files.iter().any(|s| f.path.starts_with(s)) {
                return false;
            }
            // If specific files were requested, only include those
            if let Some(ref filter) = file_filter {
                return filter.iter().any(|p| p == &f.path);
            }
            true
        })
        .collect();

    if downloadable.is_empty() {
        set_download_error(&downloads, &download_id, "No files found in repository").await;
        return;
    }

    // Calculate total size
    let total_bytes: u64 = downloadable.iter().map(|f| f.size.unwrap_or(0)).sum();

    {
        let mut dls = downloads.write().await;
        if let Some(dl) = dls.get_mut(&download_id) {
            dl.total_bytes = total_bytes;
        }
    }

    // Check that this download (plus other in-flight downloads) will fit on disk
    if let Ok(disk) = get_disk_usage(&app_state.config.model_path) {
        // Sum remaining bytes for all other active downloads
        let other_inflight: u64 = {
            let dls = downloads.read().await;
            dls.values()
                .filter(|dl| dl.id != download_id && dl.status == DownloadStatus::Downloading)
                .map(|dl| dl.total_bytes.saturating_sub(dl.progress_bytes))
                .sum()
        };

        let required = total_bytes + other_inflight;
        let projected_used = disk.used_bytes + required;
        let projected_pct = if disk.total_bytes > 0 {
            (projected_used as f64 / disk.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        if required > disk.free_bytes {
            set_download_error(
                &downloads,
                &download_id,
                &format!(
                    "Not enough disk space: need {} but only {} free",
                    format_bytes(required),
                    format_bytes(disk.free_bytes),
                ),
            )
            .await;
            return;
        }

        if projected_pct >= 95.0 {
            set_download_error(
                &downloads,
                &download_id,
                &format!(
                    "Download would push disk to {:.1}% (threshold 95%): need {}, {} free",
                    projected_pct,
                    format_bytes(required),
                    format_bytes(disk.free_bytes),
                ),
            )
            .await;
            return;
        }

        if projected_pct >= 90.0 {
            warn!(
                projected_pct = format!("{:.1}%", projected_pct),
                download_bytes = total_bytes,
                "Download will push disk above 90% warning threshold"
            );
        }
    }

    // Create destination directory: MODEL_PATH/<repo_name>
    // Replace '/' in repo name with '--' for filesystem safety
    let safe_repo = hf_repo.replace('/', "--");
    let dest_dir = format!("{}/{}", app_state.config.model_path, safe_repo);

    if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
        set_download_error(
            &downloads,
            &download_id,
            &format!("Failed to create directory {dest_dir}: {e}"),
        )
        .await;
        return;
    }

    // Step 2: Download each file
    let mut total_downloaded: u64 = 0;

    for file in &downloadable {
        // Check for cancellation
        {
            let dls = downloads.read().await;
            if let Some(dl) = dls.get(&download_id) {
                if dl.status == DownloadStatus::Cancelled {
                    info!(download_id = %download_id, "Download was cancelled, stopping");
                    return;
                }
            }
        }

        // Reject path components that could escape the destination directory
        if file.path.contains("..") || file.path.starts_with('/') {
            set_download_error(
                &downloads,
                &download_id,
                &format!("Refusing file with unsafe path: {}", file.path),
            )
            .await;
            return;
        }

        let file_url = format!(
            "https://huggingface.co/{}/resolve/main/{}",
            hf_repo, file.path
        );
        let file_dest = format!("{}/{}", dest_dir, file.path);

        // Create parent directory for nested files
        if let Some(parent) = std::path::Path::new(&file_dest).parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                set_download_error(
                    &downloads,
                    &download_id,
                    &format!("Failed to create directory for {}: {e}", file.path),
                )
                .await;
                return;
            }
        }

        info!(file = %file.path, url = %file_url, "Downloading file");

        let resp = match client.get(&file_url).send().await {
            Ok(r) => r,
            Err(e) => {
                set_download_error(
                    &downloads,
                    &download_id,
                    &format!("Failed to download {}: {e}", file.path),
                )
                .await;
                return;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let hint = if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                if hf_token.is_some() {
                    " — HF_TOKEN may lack access to this gated model"
                } else {
                    " — this may be a gated model; set HF_TOKEN env var to authenticate"
                }
            } else {
                ""
            };
            set_download_error(
                &downloads,
                &download_id,
                &format!("Download of {} returned HTTP {status}{hint}", file.path),
            )
            .await;
            return;
        }

        // Stream file to disk with progress tracking
        let mut out_file = match tokio::fs::File::create(&file_dest).await {
            Ok(f) => f,
            Err(e) => {
                set_download_error(
                    &downloads,
                    &download_id,
                    &format!("Failed to create file {}: {e}", file_dest),
                )
                .await;
                return;
            }
        };

        let mut stream = resp.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            // Check for cancellation periodically
            {
                let dls = downloads.read().await;
                if let Some(dl) = dls.get(&download_id) {
                    if dl.status == DownloadStatus::Cancelled {
                        info!(download_id = %download_id, "Download cancelled during transfer");
                        // Clean up partial file
                        let _ = tokio::fs::remove_file(&file_dest).await;
                        return;
                    }
                }
            }

            match chunk_result {
                Ok(chunk) => {
                    use tokio::io::AsyncWriteExt;
                    if let Err(e) = out_file.write_all(&chunk).await {
                        set_download_error(
                            &downloads,
                            &download_id,
                            &format!("Write error for {}: {e}", file.path),
                        )
                        .await;
                        return;
                    }

                    total_downloaded += chunk.len() as u64;

                    // Update progress
                    let mut dls = downloads.write().await;
                    if let Some(dl) = dls.get_mut(&download_id) {
                        dl.progress_bytes = total_downloaded;
                    }
                }
                Err(e) => {
                    set_download_error(
                        &downloads,
                        &download_id,
                        &format!("Stream error for {}: {e}", file.path),
                    )
                    .await;
                    return;
                }
            }
        }
    }

    // Step 3: Capture tokenizer metadata for future use (chat template detection, etc.)
    let model_metadata = fetch_tokenizer_config(&dest_dir, &hf_repo, &client).await;

    // Step 3b: Extract architecture metadata from GGUF file
    let gguf_meta = {
        let mut meta: Option<GgufMetadata> = None;
        for file in &downloadable {
            if file.path.ends_with(".gguf") {
                let gguf_path = format!("{}/{}", dest_dir, file.path);
                match read_gguf_metadata(&gguf_path).await {
                    Ok(m) => {
                        info!(
                            file = %file.path,
                            context_length = ?m.context_length,
                            n_layers = ?m.block_count,
                            n_heads = ?m.head_count,
                            n_kv_heads = ?m.head_count_kv,
                            embedding_length = ?m.embedding_length,
                            "Extracted GGUF metadata"
                        );
                        meta = Some(m);
                        break;
                    }
                    Err(e) => {
                        warn!(file = %file.path, error = %e, "Failed to read GGUF metadata");
                    }
                }
            }
        }
        meta.unwrap_or_default()
    };

    // Step 4: Register model in DB on completion
    let model_id = Uuid::new_v4().to_string();
    let size_bytes = total_downloaded as i64;

    // Detect the primary model file: prefer the largest .gguf, then .safetensors, then any file
    let primary_filename = {
        let mut best_gguf: Option<(&str, u64)> = None;
        let mut best_safetensors: Option<(&str, u64)> = None;
        for file in &downloadable {
            let sz = file.size.unwrap_or(0);
            if file.path.ends_with(".gguf") && best_gguf.is_none_or(|(_, prev)| sz > prev) {
                best_gguf = Some((&file.path, sz));
            } else if file.path.ends_with(".safetensors")
                && best_safetensors.is_none_or(|(_, prev)| sz > prev)
            {
                best_safetensors = Some((&file.path, sz));
            }
        }
        best_gguf
            .or(best_safetensors)
            .map(|(path, _)| path.to_string())
    };

    let bt = backend_type.as_deref().unwrap_or("llamacpp");
    match sqlx::query(
        "INSERT INTO models (id, hf_repo, filename, size_bytes, category_id, backend_type, model_metadata, context_length, n_layers, n_heads, n_kv_heads, embedding_length) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&model_id)
    .bind(&hf_repo)
    .bind(&primary_filename)
    .bind(size_bytes)
    .bind(&category_id)
    .bind(bt)
    .bind(&model_metadata)
    .bind(gguf_meta.context_length.map(|v| v as i64))
    .bind(gguf_meta.block_count.map(|v| v as i64))
    .bind(gguf_meta.head_count.map(|v| v as i64))
    .bind(gguf_meta.head_count_kv.map(|v| v as i64))
    .bind(gguf_meta.embedding_length.map(|v| v as i64))
    .execute(&app_state.db.pool)
    .await
    {
        Ok(_) => {
            info!(
                hf_repo = %hf_repo,
                model_id = %model_id,
                size_bytes = size_bytes,
                "Model downloaded and registered"
            );
        }
        Err(e) => {
            error!(hf_repo = %hf_repo, "Failed to register model in DB: {e}");
            set_download_error(
                &downloads,
                &download_id,
                &format!("Download complete but DB registration failed: {e}"),
            )
            .await;
            return;
        }
    }

    // Mark as complete
    let mut dls = downloads.write().await;
    if let Some(dl) = dls.get_mut(&download_id) {
        dl.status = DownloadStatus::Complete;
        dl.progress_bytes = total_downloaded;
    }
}

// ---------------------------------------------------------------------------
// Tokenizer metadata capture
// ---------------------------------------------------------------------------

/// Attempt to fetch tokenizer_config.json for a downloaded model.
///
/// Strategy:
/// 1. Read from local download directory (already downloaded)
/// 2. Fetch from HuggingFace repo directly
/// 3. Look up base_model in HF API and fetch from there
///
/// Returns the JSON string if found, None otherwise.
async fn fetch_tokenizer_config(
    dest_dir: &str,
    hf_repo: &str,
    client: &reqwest::Client,
) -> Option<String> {
    // 1. Try local file (may already be in the download)
    let local_path = format!("{}/tokenizer_config.json", dest_dir);
    if let Ok(contents) = tokio::fs::read_to_string(&local_path).await {
        // Validate it's actually JSON
        if serde_json::from_str::<serde_json::Value>(&contents).is_ok() {
            info!(hf_repo = %hf_repo, source = "local", "Captured tokenizer_config.json");
            return Some(contents);
        }
    }

    // 2. Fetch directly from the repo
    let url = format!(
        "https://huggingface.co/{}/raw/main/tokenizer_config.json",
        hf_repo
    );
    if let Ok(resp) = client.get(&url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if serde_json::from_str::<serde_json::Value>(&text).is_ok() {
                    info!(hf_repo = %hf_repo, source = "repo", "Captured tokenizer_config.json");
                    return Some(text);
                }
            }
        }
    }

    // 3. Try to find base_model via HF API and fetch from there
    let api_url = format!("https://huggingface.co/api/models/{}", hf_repo);
    if let Ok(resp) = client.get(&api_url).send().await {
        if resp.status().is_success() {
            if let Ok(model_info) = resp.json::<serde_json::Value>().await {
                // cardData.base_model can be a string or array of strings
                let base_model = model_info
                    .get("cardData")
                    .and_then(|cd| cd.get("base_model"))
                    .and_then(|bm| {
                        bm.as_str().map(String::from).or_else(|| {
                            bm.as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_str().map(String::from))
                        })
                    });

                if let Some(base) = base_model {
                    let base_url = format!(
                        "https://huggingface.co/{}/raw/main/tokenizer_config.json",
                        base
                    );
                    if let Ok(resp) = client.get(&base_url).send().await {
                        if resp.status().is_success() {
                            if let Ok(text) = resp.text().await {
                                if serde_json::from_str::<serde_json::Value>(&text).is_ok() {
                                    info!(
                                        hf_repo = %hf_repo,
                                        base_model = %base,
                                        source = "base_model",
                                        "Captured tokenizer_config.json from base model"
                                    );
                                    return Some(text);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    info!(hf_repo = %hf_repo, "No tokenizer_config.json found");
    None
}

#[derive(Debug, Deserialize)]
struct HfFileEntry {
    #[serde(rename = "type")]
    file_type: String,
    #[serde(rename = "rfilename", alias = "path")]
    path: String,
    size: Option<u64>,
}

async fn set_download_error(downloads: &Downloads, download_id: &str, error_msg: &str) {
    error!(download_id = %download_id, error = %error_msg, "Download failed");
    let mut dls = downloads.write().await;
    if let Some(dl) = dls.get_mut(download_id) {
        dl.status = DownloadStatus::Failed;
        dl.error = Some(error_msg.to_string());
    }
}

// ---------------------------------------------------------------------------
// Disk usage monitoring
// ---------------------------------------------------------------------------

pub struct DiskUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
}

/// Get disk usage for the filesystem containing the given path.
/// Uses `df` command to avoid additional crate dependencies.
pub fn get_disk_usage(path: &str) -> Result<DiskUsage, String> {
    let output = std::process::Command::new("df")
        .args(["-B1", "--output=size,used,avail", path])
        .output()
        .map_err(|e| format!("Failed to run df: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("df command failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output format:
    //      1B-blocks          Used         Avail
    //  1000204886016  537715044352  411439906816

    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err("Unexpected df output format".to_string());
    }

    let data_line = lines[1].trim();
    let parts: Vec<&str> = data_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(format!("Unexpected df output columns: {data_line}"));
    }

    let total_bytes: u64 = parts[0]
        .parse()
        .map_err(|e| format!("Failed to parse total bytes: {e}"))?;
    let used_bytes: u64 = parts[1]
        .parse()
        .map_err(|e| format!("Failed to parse used bytes: {e}"))?;
    let free_bytes: u64 = parts[2]
        .parse()
        .map_err(|e| format!("Failed to parse free bytes: {e}"))?;

    Ok(DiskUsage {
        total_bytes,
        used_bytes,
        free_bytes,
    })
}

// ---------------------------------------------------------------------------
// Human-readable byte formatting
// ---------------------------------------------------------------------------

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ---------------------------------------------------------------------------
// Simple URL encoding for query parameters
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GGUF metadata reader — extracts architecture metadata from file header
// ---------------------------------------------------------------------------

/// Architecture metadata extracted from a GGUF file header.
#[derive(Debug, Clone, Default)]
pub struct GgufMetadata {
    pub context_length: Option<u32>,
    pub block_count: Option<u32>, // n_layers
    pub embedding_length: Option<u32>,
    pub head_count: Option<u32>,    // attention.head_count
    pub head_count_kv: Option<u32>, // attention.head_count_kv
}

/// Read architecture metadata from a GGUF file's header.
///
/// Extracts: context_length, block_count, embedding_length,
/// attention.head_count, attention.head_count_kv.
///
/// GGUF format: magic (4B) + version (u32) + n_tensors (u64) + n_kv (u64)
/// then n_kv key-value pairs, each: string key + type tag (u32) + value.
pub async fn read_gguf_metadata(path: &str) -> Result<GgufMetadata, String> {
    use tokio::io::AsyncReadExt;

    let mut f = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("open: {e}"))?;

    // Read and validate magic
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)
        .await
        .map_err(|e| format!("read magic: {e}"))?;
    if &magic != b"GGUF" {
        return Err("not a GGUF file".to_string());
    }

    // Version (u32 LE)
    let mut buf4 = [0u8; 4];
    f.read_exact(&mut buf4)
        .await
        .map_err(|e| format!("read version: {e}"))?;
    let _version = u32::from_le_bytes(buf4);

    // n_tensors (u64 LE)
    let mut buf8 = [0u8; 8];
    f.read_exact(&mut buf8)
        .await
        .map_err(|e| format!("read n_tensors: {e}"))?;

    // n_kv (u64 LE)
    f.read_exact(&mut buf8)
        .await
        .map_err(|e| format!("read n_kv: {e}"))?;
    let n_kv = u64::from_le_bytes(buf8);
    if n_kv > 10_000 {
        return Err(format!("GGUF n_kv too large: {n_kv} (max 10000)"));
    }

    // Helper: read a GGUF string (u64 length + bytes)
    async fn read_string(f: &mut tokio::fs::File) -> Result<String, String> {
        let mut buf = [0u8; 8];
        f.read_exact(&mut buf)
            .await
            .map_err(|e| format!("read string len: {e}"))?;
        let len = u64::from_le_bytes(buf) as usize;
        if len > 1_000_000 {
            return Err(format!("string too long: {len}"));
        }
        let mut data = vec![0u8; len];
        f.read_exact(&mut data)
            .await
            .map_err(|e| format!("read string data: {e}"))?;
        String::from_utf8(data).map_err(|e| format!("invalid utf8: {e}"))
    }

    // Helper: read an integer value from common GGUF types
    async fn read_int_value(f: &mut tokio::fs::File, vtype: u32) -> Result<Option<u32>, String> {
        match vtype {
            4 => {
                let mut b = [0u8; 4];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
                Ok(Some(u32::from_le_bytes(b)))
            }
            5 => {
                let mut b = [0u8; 4];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
                Ok(Some(i32::from_le_bytes(b) as u32))
            }
            10 => {
                let mut b = [0u8; 8];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
                Ok(Some(u64::from_le_bytes(b) as u32))
            }
            11 => {
                let mut b = [0u8; 8];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
                Ok(Some(i64::from_le_bytes(b) as u32))
            }
            _ => Ok(None),
        }
    }

    // Helper: skip a GGUF value by type tag
    async fn skip_value(f: &mut tokio::fs::File, vtype: u32) -> Result<(), String> {
        match vtype {
            0 | 1 | 7 => {
                let mut b = [0u8; 1];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            }
            2 | 3 => {
                let mut b = [0u8; 2];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            }
            4..=6 => {
                let mut b = [0u8; 4];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            }
            8 => {
                read_string(f).await?;
            }
            9 => {
                // array: type (u32) + count (u64) + elements
                let mut tb = [0u8; 4];
                f.read_exact(&mut tb).await.map_err(|e| e.to_string())?;
                let atype = u32::from_le_bytes(tb);
                let mut cb = [0u8; 8];
                f.read_exact(&mut cb).await.map_err(|e| e.to_string())?;
                let count = u64::from_le_bytes(cb);
                for _ in 0..count {
                    Box::pin(skip_value(f, atype)).await?;
                }
            }
            10..=12 => {
                let mut b = [0u8; 8];
                f.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            }
            _ => return Err(format!("unknown GGUF type: {vtype}")),
        }
        Ok(())
    }

    let mut meta = GgufMetadata::default();

    // Keys we're looking for (all suffixed with arch prefix, e.g. "llama.context_length")
    const CONTEXT_LENGTH: &str = ".context_length";
    const BLOCK_COUNT: &str = ".block_count";
    const EMBEDDING_LENGTH: &str = ".embedding_length";
    const HEAD_COUNT: &str = ".attention.head_count";
    const HEAD_COUNT_KV: &str = ".attention.head_count_kv";

    for _ in 0..n_kv {
        let key = read_string(&mut f).await?;
        let mut tb = [0u8; 4];
        f.read_exact(&mut tb)
            .await
            .map_err(|e| format!("read type: {e}"))?;
        let vtype = u32::from_le_bytes(tb);

        let target = if key.ends_with(CONTEXT_LENGTH) {
            Some(&mut meta.context_length)
        } else if key.ends_with(BLOCK_COUNT) {
            Some(&mut meta.block_count)
        } else if key.ends_with(EMBEDDING_LENGTH) {
            Some(&mut meta.embedding_length)
        } else if key.ends_with(HEAD_COUNT) && !key.ends_with(HEAD_COUNT_KV) {
            Some(&mut meta.head_count)
        } else if key.ends_with(HEAD_COUNT_KV) {
            Some(&mut meta.head_count_kv)
        } else {
            None
        };

        if let Some(field) = target {
            if let Some(val) = read_int_value(&mut f, vtype).await? {
                *field = Some(val);
            } else {
                skip_value(&mut f, vtype).await?;
            }
        } else {
            skip_value(&mut f, vtype).await?;
        }
    }

    Ok(meta)
}

fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}
