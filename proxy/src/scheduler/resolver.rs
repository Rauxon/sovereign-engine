use anyhow::{bail, Result};
use sqlx::FromRow;

use crate::db::Database;

#[derive(Debug, Clone, FromRow)]
pub struct ResolvedModel {
    pub id: String,
    pub hf_repo: String,
    pub backend_port: Option<i32>,
    pub loaded: bool,
    pub category_id: Option<String>,
    pub backend_type: String,
}

/// Resolve a model for an inference request.
///
/// Resolution order:
/// 1. If `specific_model_id` is provided, use that model directly (must exist).
/// 2. If `category_id` is provided, look up the category's `preferred_model_id`.
/// 3. If the preferred model isn't loaded, find any loaded model in that category.
/// 4. If nothing works, try treating `model_name` as a direct model ID/hf_repo.
/// 5. If still nothing, try treating `model_name` as a category name.
pub async fn resolve_model(
    db: &Database,
    model_name: &str,
    category_id: Option<&str>,
    specific_model_id: Option<&str>,
) -> Result<ResolvedModel> {
    // 1. Specific model override from the token
    if let Some(specific_id) = specific_model_id {
        return resolve_specific_model(db, specific_id).await;
    }

    // 2. Token has a category_id — resolve via category.
    //    If the token is scoped to a category, we MUST NOT fall through to
    //    unrestricted resolution — that would bypass the category constraint.
    if let Some(cat_id) = category_id {
        if let Some(model) = resolve_from_category_id(db, cat_id).await? {
            return Ok(model);
        }
        bail!(
            "No models available in category '{}'. The token is scoped to this category \
             and cannot access models outside it.",
            cat_id
        );
    }

    // 3. Try the model field from the request as a direct model ID or hf_repo
    if let Some(model) = resolve_by_id_or_repo(db, model_name).await? {
        return Ok(model);
    }

    // 4. Try the model field as a category name
    if let Some(model) = resolve_from_category_name(db, model_name).await? {
        return Ok(model);
    }

    bail!(
        "Model '{}' not found. Provide a valid model ID, HuggingFace repo name, \
         or category name. Use GET /v1/models to list available models.",
        model_name
    )
}

/// Resolve a specific model by ID. Fails if the model doesn't exist.
async fn resolve_specific_model(db: &Database, model_id: &str) -> Result<ResolvedModel> {
    let model = sqlx::query_as::<_, ResolvedModel>(
        "SELECT id, hf_repo, backend_port, loaded, category_id, backend_type FROM models WHERE id = ?",
    )
    .bind(model_id)
    .fetch_optional(&db.pool)
    .await?;

    match model {
        Some(m) => Ok(m),
        None => bail!(
            "Specific model '{}' not found. It may have been removed.",
            model_id
        ),
    }
}

/// Look up a model by direct ID or hf_repo match.
async fn resolve_by_id_or_repo(db: &Database, model_name: &str) -> Result<Option<ResolvedModel>> {
    let model = sqlx::query_as::<_, ResolvedModel>(
        "SELECT id, hf_repo, backend_port, loaded, category_id, backend_type FROM models WHERE id = ? OR hf_repo = ?",
    )
    .bind(model_name)
    .bind(model_name)
    .fetch_optional(&db.pool)
    .await?;

    Ok(model)
}

/// Resolve from a category ID: try preferred model first, then any loaded model.
async fn resolve_from_category_id(
    db: &Database,
    category_id: &str,
) -> Result<Option<ResolvedModel>> {
    // Try the category's preferred model
    let preferred = sqlx::query_as::<_, ResolvedModel>(
        r#"
        SELECT m.id, m.hf_repo, m.backend_port, m.loaded, m.category_id, m.backend_type
        FROM model_categories c
        JOIN models m ON m.id = c.preferred_model_id
        WHERE c.id = ?
        "#,
    )
    .bind(category_id)
    .fetch_optional(&db.pool)
    .await?;

    if let Some(ref m) = preferred {
        if m.loaded {
            return Ok(preferred);
        }
    }

    // Preferred model not loaded — try any loaded model in this category
    let fallback = sqlx::query_as::<_, ResolvedModel>(
        r#"
        SELECT id, hf_repo, backend_port, loaded, category_id, backend_type
        FROM models
        WHERE category_id = ? AND loaded = 1
        ORDER BY last_used_at DESC
        LIMIT 1
        "#,
    )
    .bind(category_id)
    .fetch_optional(&db.pool)
    .await?;

    if fallback.is_some() {
        return Ok(fallback);
    }

    // Return the preferred model even if not loaded (caller will check .loaded)
    Ok(preferred)
}

/// Resolve from a category name: try preferred model first, then any loaded model.
async fn resolve_from_category_name(
    db: &Database,
    category_name: &str,
) -> Result<Option<ResolvedModel>> {
    // Look up category by name to get its ID
    let category: Option<(String,)> =
        sqlx::query_as("SELECT id FROM model_categories WHERE name = ?")
            .bind(category_name)
            .fetch_optional(&db.pool)
            .await?;

    match category {
        Some((cat_id,)) => resolve_from_category_id(db, &cat_id).await,
        None => Ok(None),
    }
}
