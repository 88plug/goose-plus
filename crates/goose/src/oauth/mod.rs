mod persist;

pub use persist::GooseCredentialStore;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use minijinja::render;
use oauth2::TokenResponse;
use rmcp::transport::auth::{
    AuthorizationMetadata, AuthorizationSession, CredentialStore, OAuthState, StoredCredentials,
};
use rmcp::transport::AuthorizationManager;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tracing::warn;
use url::Url;

const CALLBACK_TEMPLATE: &str = include_str!("oauth_callback.html");
const CLIENT_METADATA_URL: &str = "https://goose-docs.ai/oauth/client-metadata.json";
const DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS: u64 = 300;
const OAUTH_CALLBACK_TIMEOUT_ENV: &str = "GOOSE_OAUTH_CALLBACK_TIMEOUT_SECONDS";

#[derive(Clone)]
struct AppState {
    code_receiver: Arc<Mutex<Option<oneshot::Sender<CallbackParams>>>>,
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

fn resolve_oauth_callback_timeout(value: Option<&str>) -> Duration {
    value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS))
}

fn oauth_callback_timeout() -> Duration {
    let timeout = std::env::var(OAUTH_CALLBACK_TIMEOUT_ENV).ok();
    resolve_oauth_callback_timeout(timeout.as_deref())
}

fn announce_authorization_url(name: &str, authorization_url: &str) {
    warn!(
        "[OAuth:{}] If the browser did not open, authorize manually at: {}",
        name, authorization_url
    );
    eprintln!(
        "If the browser did not open, authorize {} at:\n  {}",
        name, authorization_url
    );
}

async fn wait_for_callback(
    code_receiver: oneshot::Receiver<CallbackParams>,
    timeout_duration: Duration,
    name: &str,
    authorization_url: &str,
) -> Result<CallbackParams, anyhow::Error> {
    match tokio::time::timeout(timeout_duration, code_receiver).await {
        Ok(Ok(params)) => Ok(params),
        Ok(Err(e)) => Err(anyhow::anyhow!(
            "OAuth authorization for {} ended before the callback was received: {}",
            name,
            e
        )),
        Err(_) => {
            let message = format!(
                "OAuth authorization for {} timed out waiting for the local callback. \
                 Start the OAuth flow again and open this URL manually if the browser does not open: {}",
                name, authorization_url
            );
            warn!("[OAuth:{}] {}", name, message);
            Err(anyhow::anyhow!(message))
        }
    }
}

pub async fn oauth_flow(
    mcp_server_url: &String,
    name: &String,
) -> Result<AuthorizationManager, anyhow::Error> {
    let credential_store = GooseCredentialStore::new(name.clone());
    let mut auth_manager = AuthorizationManager::new(mcp_server_url).await?;
    auth_manager.set_credential_store(credential_store.clone());

    if auth_manager.initialize_from_store().await? {
        match auth_manager.refresh_token().await {
            Ok(_) => {
                return Ok(auth_manager);
            }
            Err(e) => {
                warn!(
                    "[OAuth:{}] Token refresh failed: {} - clearing stored credentials and falling back to browser auth",
                    name, e
                );
            }
        }

        if let Err(e) = credential_store.clear().await {
            warn!("[OAuth:{}] error clearing bad credentials: {}", name, e);
        }
    }

    // No existing credentials or they were invalid - need to do the full oauth flow
    let (code_sender, code_receiver) = oneshot::channel::<CallbackParams>();
    let app_state = AppState {
        code_receiver: Arc::new(Mutex::new(Some(code_sender))),
    };

    let rendered = render!(CALLBACK_TEMPLATE, name => name);
    let handler = move |Query(params): Query<CallbackParams>, State(state): State<AppState>| {
        let rendered = rendered.clone();
        async move {
            if let Some(sender) = state.code_receiver.lock().await.take() {
                let _ = sender.send(params);
            }
            Html(rendered)
        }
    };
    let app = Router::new()
        .route("/oauth_callback", get(handler))
        .with_state(app_state);

    let port: u16 = std::env::var("GOOSE_OAUTH_CALLBACK_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let used_addr = listener.local_addr()?;
    let server_handle = tokio::spawn(async move {
        let result = axum::serve(listener, app).await;
        if let Err(e) = result {
            eprintln!("Callback server error: {}", e);
        }
    });

    let mut oauth_state = OAuthState::new(mcp_server_url, None).await?;

    let redirect_uri = format!("http://127.0.0.1:{}/oauth_callback", used_addr.port());
    if let Err(e) = oauth_state
        .start_authorization_with_metadata_url(
            &[],
            redirect_uri.as_str(),
            Some("goose"),
            Some(CLIENT_METADATA_URL),
        )
        .await
    {
        // Workaround for an rmcp bug where resource metadata discovery fails fatally
        // when the MCP server returns non-JSON (e.g. HTML) at its base URL with HTTP 200,
        // preventing fallback to .well-known/oauth-authorization-server discovery.
        // See: https://github.com/modelcontextprotocol/rust-sdk/pull/810
        warn!(
            "OAuth authorization failed, retrying with direct metadata discovery: {}",
            e
        );
        oauth_state = start_authorization_with_wellknown_fallback(mcp_server_url, &redirect_uri)
            .await
            .map_err(|fallback_err| {
                anyhow::anyhow!(
                    "OAuth authorization failed: {e}; fallback also failed: {fallback_err}"
                )
            })?;
    }

    let authorization_url = oauth_state.get_authorization_url().await?;
    announce_authorization_url(name, authorization_url.as_str());
    if let Err(e) = webbrowser::open(authorization_url.as_str()) {
        warn!(
            "[OAuth:{}] Failed to open browser automatically: {}",
            name, e
        );
    }

    let callback_params = wait_for_callback(
        code_receiver,
        oauth_callback_timeout(),
        name,
        authorization_url.as_str(),
    )
    .await;
    server_handle.abort();
    let CallbackParams {
        code: auth_code,
        state: csrf_token,
    } = callback_params?;
    oauth_state.handle_callback(&auth_code, &csrf_token).await?;

    let (client_id, token_response) = oauth_state.get_credentials().await?;

    let mut auth_manager = oauth_state
        .into_authorization_manager()
        .ok_or_else(|| anyhow::anyhow!("Failed to get authorization manager"))?;

    let granted_scopes: Vec<String> = token_response
        .as_ref()
        .and_then(|tr| tr.scopes())
        .map(|scopes| scopes.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    credential_store
        .save(StoredCredentials::new(
            client_id,
            token_response,
            granted_scopes,
            Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0),
            ),
        ))
        .await?;

    auth_manager.set_credential_store(credential_store);

    Ok(auth_manager)
}

/// Minimal subset of RFC 9728 Protected Resource Metadata, used only in the fallback
/// discovery path. The full type in rmcp is not public.
#[derive(Deserialize)]
struct ProtectedResourceMetadata {
    authorization_servers: Option<Vec<String>>,
}

/// Fallback for when OAuthState::start_authorization_with_metadata_url fails due to
/// the resource metadata discovery bug. Follows the MCP spec discovery flow:
/// 1. Fetch Protected Resource Metadata from .well-known/oauth-protected-resource
/// 2. Extract the authorization server URL
/// 3. Fetch Authorization Server Metadata from that URL
/// 4. Create the session manually using rmcp's public APIs
///
/// This correctly handles cases where the authorization server is on a different
/// host from the MCP server, and path-based deployments.
async fn start_authorization_with_wellknown_fallback(
    mcp_server_url: &str,
    redirect_uri: &str,
) -> Result<OAuthState, anyhow::Error> {
    let base_url = Url::parse(mcp_server_url)?;
    let origin = base_url.origin().ascii_serialization();
    let path = base_url.path().trim_matches('/');

    // Step 1: Discover Protected Resource Metadata (RFC 9728)
    // Try path-specific endpoint first, then root, per the MCP spec.
    let resource_metadata = {
        let mut candidates = Vec::new();
        if !path.is_empty() {
            candidates.push(format!(
                "{origin}/.well-known/oauth-protected-resource/{path}"
            ));
        }
        candidates.push(format!("{origin}/.well-known/oauth-protected-resource"));

        let mut result = None;
        for url in &candidates {
            if let Ok(resp) = reqwest::get(url).await {
                if let Ok(meta) = resp.json::<ProtectedResourceMetadata>().await {
                    result = Some(meta);
                    break;
                }
            }
        }
        result.ok_or_else(|| anyhow::anyhow!("no protected resource metadata found"))?
    };

    // Step 2: Extract authorization server URL
    let auth_server_url = resource_metadata
        .authorization_servers
        .as_ref()
        .and_then(|servers| servers.first())
        .ok_or_else(|| anyhow::anyhow!("no authorization_servers in resource metadata"))?;

    // Step 3: Discover Authorization Server Metadata (RFC 8414)
    // Try path-qualified and root well-known URLs per the MCP spec.
    let auth_server = Url::parse(auth_server_url)?;
    let as_origin = auth_server.origin().ascii_serialization();
    let as_path = auth_server.path().trim_matches('/');

    let as_metadata = {
        let mut candidates = Vec::new();
        if !as_path.is_empty() {
            candidates.push(format!(
                "{as_origin}/.well-known/oauth-authorization-server/{as_path}"
            ));
            candidates.push(format!(
                "{as_origin}/.well-known/openid-configuration/{as_path}"
            ));
        }
        candidates.push(format!(
            "{as_origin}/.well-known/oauth-authorization-server"
        ));
        candidates.push(format!("{as_origin}/.well-known/openid-configuration"));

        let mut result = None;
        for url in &candidates {
            if let Ok(resp) = reqwest::get(url).await {
                if let Ok(meta) = resp.json::<AuthorizationMetadata>().await {
                    result = Some(meta);
                    break;
                }
            }
        }
        result.ok_or_else(|| anyhow::anyhow!("no authorization server metadata found"))?
    };

    // Step 4: Create session using rmcp public APIs
    let mut manager = AuthorizationManager::new(mcp_server_url).await?;
    manager.set_metadata(as_metadata);

    let session = AuthorizationSession::new(
        manager,
        &[],
        redirect_uri,
        Some("goose"),
        Some(CLIENT_METADATA_URL),
    )
    .await?;

    Ok(OAuthState::Session(session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_oauth_callback_timeout_uses_default_for_missing_or_invalid_values() {
        assert_eq!(
            resolve_oauth_callback_timeout(None),
            Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS)
        );
        assert_eq!(
            resolve_oauth_callback_timeout(Some("not-a-number")),
            Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS)
        );
        assert_eq!(
            resolve_oauth_callback_timeout(Some("0")),
            Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS)
        );
    }

    #[test]
    fn resolve_oauth_callback_timeout_uses_positive_values() {
        assert_eq!(
            resolve_oauth_callback_timeout(Some("42")),
            Duration::from_secs(42)
        );
    }

    #[tokio::test]
    async fn wait_for_callback_returns_received_callback_params() {
        let (sender, receiver) = oneshot::channel();
        sender
            .send(CallbackParams {
                code: "auth-code".to_string(),
                state: "csrf-state".to_string(),
            })
            .unwrap();

        let params = wait_for_callback(
            receiver,
            Duration::from_secs(1),
            "test-server",
            "https://auth.example/authorize",
        )
        .await
        .unwrap();

        assert_eq!(params.code, "auth-code");
        assert_eq!(params.state, "csrf-state");
    }

    #[tokio::test]
    async fn wait_for_callback_times_out_with_authorization_url() {
        let (_sender, receiver) = oneshot::channel();

        let error = wait_for_callback(
            receiver,
            Duration::from_millis(1),
            "test-server",
            "https://auth.example/authorize",
        )
        .await
        .unwrap_err();
        let message = error.to_string();

        assert!(message.contains("test-server"));
        assert!(message.contains("timed out"));
        assert!(message.contains("https://auth.example/authorize"));
    }

    #[test]
    fn protected_resource_metadata_deserializes_authorization_servers() {
        let json = r#"{"resource": "https://mcp.example/api", "authorization_servers": ["https://auth.example"]}"#;
        let meta: ProtectedResourceMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(
            meta.authorization_servers,
            Some(vec!["https://auth.example".to_string()])
        );
    }

    #[test]
    fn protected_resource_metadata_tolerates_missing_authorization_servers() {
        let json = r#"{"resource": "https://mcp.example/api"}"#;
        let meta: ProtectedResourceMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.authorization_servers, None);
    }

    async fn serve_fixed_response(body: &'static str, content_type: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    content_type,
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn wellknown_fallback_fails_cleanly_when_protected_resource_metadata_missing() {
        // No listener at all behind this URL -> the fetch itself fails, exercising the
        // "no protected resource metadata found" error path without a real MCP server.
        let result = start_authorization_with_wellknown_fallback(
            "http://127.0.0.1:1/mcp",
            "http://127.0.0.1:0/oauth_callback",
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn wellknown_fallback_surfaces_missing_authorization_servers() {
        let base_url = serve_fixed_response(r#"{"resource": "test"}"#, "application/json").await;
        let result = start_authorization_with_wellknown_fallback(
            &base_url,
            "http://127.0.0.1:0/oauth_callback",
        )
        .await;
        match result {
            Ok(_) => panic!("expected an error when authorization_servers is missing"),
            Err(err) => assert!(
                err.to_string().contains("no authorization_servers"),
                "unexpected error: {err}"
            ),
        }
    }
}
