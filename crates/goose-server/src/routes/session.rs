use crate::routes::errors::ErrorResponse;
use crate::routes::recipe_utils::{apply_recipe_to_agent, build_recipe_with_parameter_values};
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{
    extract::Path,
    http::{header::HeaderName, HeaderValue, StatusCode},
    routing::{get, put},
    Json, Router,
};
use goose::agents::ExtensionConfig;
use goose::conversation::Conversation;
use goose::recipe::Recipe;
#[cfg(feature = "nostr")]
use goose::session::nostr_share;
#[cfg(feature = "nostr")]
use goose::session::session_manager::SessionType;
use goose::session::{EnabledExtensionsState, Session};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionNameRequest {
    /// Updated name for the session (max 200 characters)
    name: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionUserRecipeValuesRequest {
    /// Recipe parameter values entered by the user
    user_recipe_values: HashMap<String, String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateSessionUserRecipeValuesResponse {
    recipe: Recipe,
}

#[cfg_attr(not(feature = "nostr"), allow(dead_code))]
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareSessionNostrRequest {
    #[serde(default)]
    relays: Vec<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareSessionNostrResponse {
    deeplink: String,
    nevent: String,
    event_id: String,
    relays: Vec<String>,
}

#[cfg_attr(not(feature = "nostr"), allow(dead_code))]
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionNostrRequest {
    deeplink: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ForkRequest {
    timestamp: Option<i64>,
    truncate: bool,
    copy: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ForkResponse {
    session_id: String,
}

const MAX_NAME_LENGTH: usize = 200;

/// Default page size when only an offset is supplied. Large enough that the
/// endpoint keeps returning the full conversation when no pagination params
/// are given, preserving backwards-compatible behavior.
const DEFAULT_MESSAGE_LIMIT: usize = usize::MAX;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionQuery {
    /// Maximum number of messages to return. Omit to return all messages.
    limit: Option<usize>,
    /// Number of messages to skip from the start of the conversation.
    offset: Option<usize>,
}

fn paginate_conversation(
    conversation: Conversation,
    offset: usize,
    limit: usize,
) -> (Conversation, usize) {
    let messages = conversation.messages();
    let total = messages.len();
    if offset == 0 && limit == DEFAULT_MESSAGE_LIMIT {
        return (conversation, total);
    }
    let page = messages
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    (Conversation::new_unvalidated(page), total)
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session"),
        ("limit" = Option<usize>, Query, description = "Maximum number of messages to return (default: all)"),
        ("offset" = Option<usize>, Query, description = "Number of messages to skip from the start (default: 0)")
    ),
    responses(
        (status = 200, description = "Session history retrieved successfully. Pagination metadata is returned in the x-total-messages, x-limit and x-offset response headers.", body = Session),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<GetSessionQuery>,
) -> Result<Response, StatusCode> {
    let mut session = state
        .session_manager()
        .get_session(&session_id, true)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(DEFAULT_MESSAGE_LIMIT);

    let total = if let Some(conversation) = session.conversation.take() {
        let (page, total) = paginate_conversation(conversation, offset, limit);
        session.conversation = Some(page);
        total
    } else {
        0
    };

    let mut response = Json(session).into_response();
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-total-messages"),
        HeaderValue::from(total),
    );
    headers.insert(
        HeaderName::from_static("x-offset"),
        HeaderValue::from(offset),
    );
    if limit != DEFAULT_MESSAGE_LIMIT {
        headers.insert(HeaderName::from_static("x-limit"), HeaderValue::from(limit));
    }

    Ok(response)
}

#[utoipa::path(
    put,
    path = "/sessions/{session_id}/name",
    request_body = UpdateSessionNameRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session name updated successfully"),
        (status = 400, description = "Bad request - Name too long (max 200 characters)"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn update_session_name(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionNameRequest>,
) -> Result<StatusCode, StatusCode> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if name.len() > MAX_NAME_LENGTH {
        return Err(StatusCode::BAD_REQUEST);
    }

    state
        .session_manager()
        .update(&session_id)
        .user_provided_name(name.to_string())
        .apply()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    put,
    path = "/sessions/{session_id}/user_recipe_values",
    request_body = UpdateSessionUserRecipeValuesRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session user recipe values updated successfully", body = UpdateSessionUserRecipeValuesResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
// Update session user recipe parameter values
async fn update_session_user_recipe_values(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionUserRecipeValuesRequest>,
) -> Result<Json<UpdateSessionUserRecipeValuesResponse>, ErrorResponse> {
    state
        .session_manager()
        .update(&session_id)
        .user_recipe_values(Some(request.user_recipe_values))
        .apply()
        .await
        .map_err(|err| ErrorResponse {
            message: err.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    let session = state
        .session_manager()
        .get_session(&session_id, false)
        .await
        .map_err(|err| ErrorResponse {
            message: err.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;
    let recipe = session.recipe.ok_or_else(|| ErrorResponse {
        message: "Recipe not found".to_string(),
        status: StatusCode::NOT_FOUND,
    })?;

    let user_recipe_values = session.user_recipe_values.unwrap_or_default();
    match build_recipe_with_parameter_values(&recipe, user_recipe_values).await {
        Ok(Some(recipe)) => {
            let agent = state
                .get_agent_for_route(session_id.clone())
                .await
                .map_err(|status| ErrorResponse {
                    message: format!("Failed to get agent: {}", status),
                    status,
                })?;
            if let Some(prompt) = apply_recipe_to_agent(&agent, &recipe, false).await {
                agent
                    .extend_system_prompt("recipe".to_string(), prompt)
                    .await;
            }
            Ok(Json(UpdateSessionUserRecipeValuesResponse { recipe }))
        }
        Ok(None) => Err(ErrorResponse {
            message: "Missing required parameters".to_string(),
            status: StatusCode::BAD_REQUEST,
        }),
        Err(e) => Err(ErrorResponse {
            message: e.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }),
    }
}

#[cfg_attr(not(feature = "nostr"), allow(unused_variables))]
#[utoipa::path(
    post,
    path = "/sessions/{session_id}/share/nostr",
    request_body = ShareSessionNostrRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session shared to Nostr successfully", body = ShareSessionNostrResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn share_session_nostr(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<ShareSessionNostrRequest>,
) -> Result<Json<ShareSessionNostrResponse>, StatusCode> {
    #[cfg(feature = "nostr")]
    {
        let exported = state
            .session_manager()
            .export_session(&session_id)
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        let relays = nostr_share::resolve_relays(request.relays, goose::config::Config::global());
        let share = nostr_share::publish_session_json(&exported, relays)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(ShareSessionNostrResponse {
            deeplink: share.deeplink,
            nevent: share.nevent,
            event_id: share.event_id,
            relays: share.relays,
        }))
    }

    #[cfg(not(feature = "nostr"))]
    Err(StatusCode::NOT_FOUND)
}

#[cfg_attr(not(feature = "nostr"), allow(unused_variables))]
#[utoipa::path(
    post,
    path = "/sessions/import/nostr",
    request_body = ImportSessionNostrRequest,
    responses(
        (status = 200, description = "Nostr shared session imported successfully", body = Session),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 400, description = "Bad request - Invalid Nostr share link"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn import_session_nostr(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ImportSessionNostrRequest>,
) -> Result<Json<Session>, StatusCode> {
    #[cfg(feature = "nostr")]
    {
        let json = nostr_share::import_session_json_from_deeplink(&request.deeplink)
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        let session = state
            .session_manager()
            .import_session(&json, Some(SessionType::User))
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        Ok(Json(session))
    }

    #[cfg(not(feature = "nostr"))]
    Err(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/fork",
    request_body = ForkRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session forked successfully", body = ForkResponse),
        (status = 400, description = "Bad request - truncate=true requires timestamp"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn fork_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<ForkRequest>,
) -> Result<Json<ForkResponse>, ErrorResponse> {
    if request.truncate && request.timestamp.is_none() {
        return Err(ErrorResponse {
            message: "truncate=true requires a timestamp".to_string(),
            status: StatusCode::BAD_REQUEST,
        });
    }

    let session_manager = state.session_manager();

    let target_session_id = if request.copy {
        let original = session_manager
            .get_session(&session_id, false)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get session: {}", e);
                #[cfg(feature = "telemetry")]
                goose::posthog::emit_error("session_get_failed", &e.to_string());
                ErrorResponse {
                    message: if e.to_string().contains("not found") {
                        format!("Session {} not found", session_id)
                    } else {
                        format!("Failed to get session: {}", e)
                    },
                    status: if e.to_string().contains("not found") {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    },
                }
            })?;

        let copied = session_manager
            .copy_session(&session_id, original.name)
            .await
            .map_err(|e| {
                tracing::error!("Failed to copy session: {}", e);
                #[cfg(feature = "telemetry")]
                goose::posthog::emit_error("session_copy_failed", &e.to_string());
                ErrorResponse {
                    message: format!("Failed to copy session: {}", e),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;

        copied.id
    } else {
        session_id.clone()
    };

    if request.truncate {
        session_manager
            .truncate_conversation(&target_session_id, request.timestamp.unwrap_or(0))
            .await
            .map_err(|e| {
                tracing::error!("Failed to truncate conversation: {}", e);
                #[cfg(feature = "telemetry")]
                goose::posthog::emit_error("session_truncate_failed", &e.to_string());
                ErrorResponse {
                    message: format!("Failed to truncate conversation: {}", e),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;
    }

    Ok(Json(ForkResponse {
        session_id: target_session_id,
    }))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionExtensionsResponse {
    extensions: Vec<ExtensionConfig>,
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/extensions",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session extensions retrieved successfully", body = SessionExtensionsResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session_extensions(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionExtensionsResponse>, StatusCode> {
    let session = state
        .session_manager()
        .get_session(&session_id, false)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let extensions = EnabledExtensionsState::extensions_or_default(
        Some(&session.extension_data),
        goose::config::Config::global(),
    );

    Ok(Json(SessionExtensionsResponse { extensions }))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sessions/{session_id}", get(get_session))
        .route(
            "/sessions/{session_id}/share/nostr",
            post(share_session_nostr).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route(
            "/sessions/import/nostr",
            post(import_session_nostr).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/sessions/{session_id}/name", put(update_session_name))
        .route(
            "/sessions/{session_id}/user_recipe_values",
            put(update_session_user_recipe_values),
        )
        .route("/sessions/{session_id}/fork", post(fork_session))
        .route(
            "/sessions/{session_id}/extensions",
            get(get_session_extensions),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::conversation::message::Message;

    fn conversation_with(n: usize) -> Conversation {
        Conversation::new_unvalidated((0..n).map(|i| Message::user().with_id(format!("m{i}"))))
    }

    fn ids(conversation: &Conversation) -> Vec<String> {
        conversation
            .messages()
            .iter()
            .map(|m| m.id.clone().unwrap_or_default())
            .collect()
    }

    #[test]
    fn no_params_returns_full_conversation() {
        let (page, total) = paginate_conversation(conversation_with(5), 0, DEFAULT_MESSAGE_LIMIT);
        assert_eq!(total, 5);
        assert_eq!(page.len(), 5);
        assert_eq!(ids(&page), vec!["m0", "m1", "m2", "m3", "m4"]);
    }

    #[test]
    fn limit_caps_returned_messages() {
        let (page, total) = paginate_conversation(conversation_with(5), 0, 2);
        assert_eq!(total, 5);
        assert_eq!(ids(&page), vec!["m0", "m1"]);
    }

    #[test]
    fn offset_skips_leading_messages() {
        let (page, total) = paginate_conversation(conversation_with(5), 2, 2);
        assert_eq!(total, 5);
        assert_eq!(ids(&page), vec!["m2", "m3"]);
    }

    #[test]
    fn offset_past_end_returns_empty_page_with_full_total() {
        let (page, total) = paginate_conversation(conversation_with(3), 10, 5);
        assert_eq!(total, 3);
        assert!(page.messages().is_empty());
    }

    #[test]
    fn limit_larger_than_remaining_returns_tail() {
        let (page, total) = paginate_conversation(conversation_with(4), 3, 100);
        assert_eq!(total, 4);
        assert_eq!(ids(&page), vec!["m3"]);
    }
}
