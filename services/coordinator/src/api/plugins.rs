use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::AppState;

#[derive(Serialize)]
pub struct PluginListResponse {
    pub plugins: Vec<PluginSummary>,
}

#[derive(Serialize)]
pub struct PluginSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub game_type: String,
    pub path: String,
    pub loaded_at: SystemTime,
    pub memory_usage_bytes: u64,
    pub active_tables: Vec<u32>,
}

#[derive(Serialize)]
pub struct PluginDetailResponse {
    pub name: String,
    pub version: String,
    pub description: String,
    pub game_type: String,
    pub author: Option<String>,
    pub path: String,
    pub loaded_at: SystemTime,
    pub memory_usage_bytes: u64,
    pub active_tables: Vec<u32>,
}

#[derive(Deserialize)]
pub struct LoadPluginRequest {
    pub path: Option<String>,
    pub wasm_bytes: Option<String>,
}

#[derive(Serialize)]
pub struct LoadPluginResponse {
    pub name: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct UnloadPluginResponse {
    pub name: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct PluginCallResponse {
    pub plugin: String,
    pub function: String,
    pub result: serde_json::Value,
}

#[derive(Deserialize)]
pub struct PluginCallRequest {
    pub function: String,
    pub args: serde_json::Value,
}

#[derive(Serialize)]
pub struct PluginHealthResponse {
    pub total_plugins: usize,
    pub plugins: Vec<PluginSummary>,
    pub status: String,
}

/// GET /api/plugins
pub async fn list_plugins(State(state): State<AppState>) -> Json<PluginListResponse> {
    let loader = state.plugin_loader.read().await;
    let plugins = loader.plugins().read().await;
    let summaries = plugins
        .values()
        .map(|p| PluginSummary {
            name: p.info.manifest.name.clone(),
            version: p.info.manifest.version.clone(),
            description: p.info.manifest.description.clone(),
            game_type: p.info.manifest.game_type.clone(),
            path: p.info.path.clone(),
            loaded_at: p.info.loaded_at,
            memory_usage_bytes: p.info.memory_usage_bytes,
            active_tables: p.info.active_tables.clone(),
        })
        .collect();

    Json(PluginListResponse { plugins: summaries })
}

/// GET /api/plugins/:name
pub async fn get_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PluginDetailResponse>, StatusCode> {
    let loader = state.plugin_loader.read().await;
    let plugins = loader.plugins().read().await;
    let plugin = plugins.get(&name).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(PluginDetailResponse {
        name: plugin.info.manifest.name.clone(),
        version: plugin.info.manifest.version.clone(),
        description: plugin.info.manifest.description.clone(),
        game_type: plugin.info.manifest.game_type.clone(),
        author: plugin.info.manifest.author.clone(),
        path: plugin.info.path.clone(),
        loaded_at: plugin.info.loaded_at,
        memory_usage_bytes: plugin.info.memory_usage_bytes,
        active_tables: plugin.info.active_tables.clone(),
    }))
}

/// POST /api/plugins/load
pub async fn load_plugin(
    State(state): State<AppState>,
    Json(req): Json<LoadPluginRequest>,
) -> Result<Json<LoadPluginResponse>, StatusCode> {
    let loader = state.plugin_loader.read().await;

    let name = if let Some(path_str) = &req.path {
        let path = std::path::Path::new(path_str);
        if !path.exists() {
            return Err(StatusCode::BAD_REQUEST);
        }
        loader.load_plugin_from_path(path).await.map_err(|e| {
            tracing::error!("Failed to load plugin from path: {}", e);
            StatusCode::BAD_REQUEST
        })?
    } else if let Some(b64_bytes) = &req.wasm_bytes {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64_bytes)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        let path = std::path::Path::new("memory://plugin.wasm");
        loader
            .load_plugin_from_bytes(&bytes, path)
            .await
            .map_err(|e| {
                tracing::error!("Failed to load plugin from bytes: {}", e);
                StatusCode::BAD_REQUEST
            })?
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    Ok(Json(LoadPluginResponse {
        name,
        status: "loaded".to_string(),
    }))
}

/// POST /api/plugins/:name/unload
pub async fn unload_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<UnloadPluginResponse>, StatusCode> {
    let loader = state.plugin_loader.read().await;
    loader.unload_plugin(&name).await.map_err(|e| {
        tracing::error!("Failed to unload plugin: {}", e);
        StatusCode::NOT_FOUND
    })?;

    Ok(Json(UnloadPluginResponse {
        name,
        status: "unloaded".to_string(),
    }))
}

/// POST /api/plugins/rescan
pub async fn rescan_plugins(State(state): State<AppState>) -> Json<PluginListResponse> {
    let loader = state.plugin_loader.read().await;
    let _ = loader.scan_plugin_directory(None).await;

    let plugins = loader.plugins().read().await;
    let summaries = plugins
        .values()
        .map(|p| PluginSummary {
            name: p.info.manifest.name.clone(),
            version: p.info.manifest.version.clone(),
            description: p.info.manifest.description.clone(),
            game_type: p.info.manifest.game_type.clone(),
            path: p.info.path.clone(),
            loaded_at: p.info.loaded_at,
            memory_usage_bytes: p.info.memory_usage_bytes,
            active_tables: p.info.active_tables.clone(),
        })
        .collect();

    Json(PluginListResponse { plugins: summaries })
}

/// GET /api/plugins/health
pub async fn plugin_health(State(state): State<AppState>) -> Json<PluginHealthResponse> {
    let loader = state.plugin_loader.read().await;
    let plugins = loader.plugins().read().await;
    let total = plugins.len();
    let summaries = plugins
        .values()
        .map(|p| PluginSummary {
            name: p.info.manifest.name.clone(),
            version: p.info.manifest.version.clone(),
            description: p.info.manifest.description.clone(),
            game_type: p.info.manifest.game_type.clone(),
            path: p.info.path.clone(),
            loaded_at: p.info.loaded_at,
            memory_usage_bytes: p.info.memory_usage_bytes,
            active_tables: p.info.active_tables.clone(),
        })
        .collect();

    Json(PluginHealthResponse {
        total_plugins: total,
        plugins: summaries,
        status: if total > 0 {
            "plugins_active".to_string()
        } else {
            "no_plugins_loaded".to_string()
        },
    })
}

/// POST /api/plugins/:name/call
pub async fn call_plugin_function(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<PluginCallRequest>,
) -> Result<Json<PluginCallResponse>, StatusCode> {
    let loader = state.plugin_loader.read().await;

    let result = match req.function.as_str() {
        "evaluate_hand" => {
            let cards: [u32; 7] = serde_json::from_value(req.args)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let rank = loader
                .call_evaluate_hand(&name, &cards)
                .await
                .map_err(|e| {
                    tracing::error!("Plugin call failed: {}", e);
                    StatusCode::BAD_GATEWAY
                })?;
            serde_json::json!({ "rank": rank })
        }
        "get_betting_rounds" => {
            let rounds = loader.call_get_betting_rounds(&name).await.map_err(|e| {
                tracing::error!("Plugin call failed: {}", e);
                StatusCode::BAD_GATEWAY
            })?;
            serde_json::json!({ "rounds": rounds })
        }
        "can_raise" => {
            let round: u32 =
                serde_json::from_value(req.args).map_err(|_| StatusCode::BAD_REQUEST)?;
            let can = loader.call_can_raise(&name, round).await.map_err(|e| {
                tracing::error!("Plugin call failed: {}", e);
                StatusCode::BAD_GATEWAY
            })?;
            serde_json::json!({ "can_raise": can })
        }
        "get_blind_structure" => {
            let args: serde_json::Value = req.args;
            let small_blind = args.get("small_blind").and_then(|v| v.as_i64()).unwrap_or(10) as i128;
            let big_blind = args.get("big_blind").and_then(|v| v.as_i64()).unwrap_or(20) as i128;
            let level = args.get("level").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let structure = loader
                .call_get_blind_structure(&name, small_blind, big_blind, level)
                .await
                .map_err(|e| {
                    tracing::error!("Plugin call failed: {}", e);
                    StatusCode::BAD_GATEWAY
                })?;
            serde_json::json!({
                "small_blind": structure.small_blind,
                "big_blind": structure.big_blind,
                "ante": structure.ante,
            })
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    Ok(Json(PluginCallResponse {
        plugin: name,
        function: req.function,
        result,
    }))
}
