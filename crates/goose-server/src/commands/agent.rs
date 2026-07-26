use crate::configuration;
use crate::state;
use anyhow::Result;
use axum::middleware;
use axum_server::Handle;
use futures::StreamExt;
use goose::acp::server_factory::{AcpServer, AcpServerFactoryConfig};
use goose::acp::transport::create_acp_router;
use goose::agents::types::SessionConfig;
use goose::agents::{AgentEvent, GoosePlatform};
use goose::config::paths::Paths;
use goose::conversation::message::Message;
use goose_server::auth::{check_acp_token, check_token};
#[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
use goose_server::tls::setup_tls;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

fn boot_marker(message: &str) {
    eprintln!("GOOSED_BOOT: {message}");
}

/// Read an opt-in flag tolerantly from the environment or config.yaml. The
/// shared `get_param` coerces a raw `"1"` into a number in both sources, so a
/// plain `get_param::<bool>` silently disables the feature; `get_flag` accepts
/// the conventional truthy spellings from either.
fn env_flag_enabled(key: &str) -> bool {
    goose::config::Config::global().get_flag(key)
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

pub async fn run() -> Result<()> {
    // Install the rustls crypto provider early, before any spawned tasks (tunnel,
    // gateways, etc.) try to open TLS connections. Both `ring` and `aws-lc-rs`
    // features are enabled on rustls (via different transitive deps), so rustls
    // cannot auto-detect a provider — we must pick one explicitly.
    #[cfg(feature = "rustls-tls")]
    let _ = rustls::crypto::ring::default_provider().install_default();

    boot_marker("main entered");
    crate::logging::setup_logging(Some("goosed"))?;

    let settings = configuration::Settings::new()?;

    let secret_key = std::env::var("GOOSE_SERVER__SECRET_KEY")
        .unwrap_or_else(|_| hex::encode(rand::random::<[u8; 32]>()));

    boot_marker("appstate init start");
    let app_state = state::AppState::new(settings.tls).await?;

    // Share the server secret with the tunnel manager so it uses the same
    // key for forwarded requests, without mutating the process environment.
    app_state
        .tunnel_manager
        .set_server_secret(secret_key.clone())
        .await;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // TODO(acp-migration): When ui/desktop launches `goose serve` directly,
    // move any goosed-only ACP setup into the goose serve path before deleting
    // this bridge. In particular, verify everything ACP currently gets from
    // goosed startup/AppState initialization, including builtin extension
    // registration and the desktop platform identity.
    let acp_server = Arc::new(AcpServer::new(AcpServerFactoryConfig {
        builtins: vec!["developer".to_string()],
        data_dir: Paths::data_dir(),
        config_dir: Paths::config_dir(),
        goose_platform: GoosePlatform::GooseDesktop,
        additional_source_roots: Vec::new(),
        scheduler: Some(app_state.scheduler()),
    }));

    let rest_router = crate::routes::configure(app_state.clone(), secret_key.clone()).layer(
        middleware::from_fn_with_state(secret_key.clone(), check_token),
    );
    let acp_router = create_acp_router(acp_server).layer(middleware::from_fn_with_state(
        secret_key.clone(),
        check_acp_token,
    ));

    // A2A (Agent2Agent) server is opt-in via GOOSE_A2A_ENABLE and mounted
    // without the x-secret-key layer (remote agents authenticate per the Agent
    // Card scheme, not goose's internal secret).
    let a2a_enabled = env_flag_enabled("GOOSE_A2A_ENABLE");
    let app = if a2a_enabled {
        let origin = goose::config::Config::global()
            .get_param::<String>("GOOSE_A2A_URL")
            .unwrap_or_else(|_| format!("http://{}", settings.socket_addr()));
        info!("A2A server enabled, Agent Card origin {}", origin);
        let a2a_router = crate::routes::a2a::router(app_state.clone(), origin);
        rest_router.merge(acp_router).merge(a2a_router).layer(cors)
    } else {
        rest_router.merge(acp_router).layer(cors)
    };

    let addr = settings.socket_addr();

    let tunnel_manager = app_state.tunnel_manager.clone();
    tokio::spawn(async move {
        tunnel_manager.check_auto_start().await;
    });

    let gateway_manager = app_state.gateway_manager.clone();
    tokio::spawn(async move {
        gateway_manager.check_auto_start().await;
    });

    spawn_nats_drive_loop(app_state.clone());

    if settings.tls {
        #[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
        {
            boot_marker("tls setup start");
            let tls_setup = setup_tls(
                settings.tls_cert_path.as_deref(),
                settings.tls_key_path.as_deref(),
            )
            .await?;

            let handle = Handle::new();
            let shutdown_handle = handle.clone();
            tokio::spawn(async move {
                shutdown_signal().await;
                shutdown_handle.graceful_shutdown(None);
            });

            info!("listening on https://{}", addr);
            boot_marker("listening");

            #[cfg(feature = "rustls-tls")]
            axum_server::bind_rustls(addr, tls_setup.config)
                .handle(handle)
                .serve(app.into_make_service())
                .await?;

            #[cfg(feature = "native-tls")]
            axum_server::bind_openssl(addr, tls_setup.config)
                .handle(handle)
                .serve(app.into_make_service())
                .await?;
        }

        #[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
        {
            anyhow::bail!(
                "TLS was requested but no TLS backend is enabled. \
                 Enable the `rustls-tls` or `native-tls` feature."
            );
        }
    } else {
        boot_marker("tcp bind start");
        let listener = tokio::net::TcpListener::bind(addr).await?;

        info!("listening on http://{}", addr);
        boot_marker("listening");

        axum::serve(listener, app)
            .with_graceful_shutdown(async { shutdown_signal().await })
            .await?;
    }

    #[cfg(feature = "otel")]
    if goose::otel::otlp::is_otlp_initialized() {
        goose::otel::otlp::shutdown_otlp();
    }

    info!("server shutdown complete");
    Ok(())
}

/// Opt-in: when `GOOSE_NATS_DRIVE` is enabled and `GOOSE_NATS_URL` is set, let
/// NATS drive the agent. Mirrors the A2A executor: each inbound command runs one
/// `agent.reply` to completion and the concatenated assistant text is returned.
fn spawn_nats_drive_loop(app_state: Arc<state::AppState>) {
    let cfg = goose::config::Config::global();
    let drive_enabled = env_flag_enabled("GOOSE_NATS_DRIVE");
    let nats_configured = cfg
        .get_param::<String>("GOOSE_NATS_URL")
        .map(|u| !u.trim().is_empty())
        .unwrap_or(false);
    if !(drive_enabled && nats_configured) {
        return;
    }

    info!("NATS drive loop enabled");
    tokio::spawn(async move {
        goose::nats::run_drive_loop(move |session_id, prompt| {
            let app = app_state.clone();
            async move {
                let agent = app.get_agent(session_id.clone()).await?;

                let working_dir =
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                app.session_manager()
                    .ensure_session(&session_id, working_dir)
                    .await?;

                if agent.provider().await.is_err() {
                    let cfg = goose::config::Config::global();
                    match (cfg.get_goose_provider(), cfg.get_goose_model()) {
                        (Ok(provider_name), Ok(model)) => {
                            let model_config = goose::model_config::model_config_from_user_config(
                                &provider_name,
                                &model,
                            )?;
                            let extensions = goose::session::EnabledExtensionsState::for_session(
                                app.session_manager(),
                                &session_id,
                                cfg,
                            )
                            .await;
                            let provider =
                                goose::providers::create(&provider_name, model_config, extensions)
                                    .await?;
                            agent.update_provider(provider, &session_id).await?;
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "no provider configured (set GOOSE_PROVIDER and GOOSE_MODEL)"
                            ));
                        }
                    }
                }

                let session_config = SessionConfig {
                    id: session_id.clone(),
                    schedule_id: None,
                    max_turns: Some(50),
                    retry_config: None,
                };

                let user_message = Message::user().with_text(prompt);
                let mut stream = agent.reply(user_message, session_config, None).await?;

                let mut agent_text = String::new();
                while let Some(event) = stream.next().await {
                    if let Ok(AgentEvent::Message(m)) = event {
                        if m.role == rmcp::model::Role::Assistant {
                            agent_text.push_str(&m.as_concat_text());
                        }
                    }
                }
                Ok(agent_text)
            }
        })
        .await;
    });
}

#[cfg(test)]
mod tests {
    use goose::config::Config;

    #[test]
    fn truthy_accepts_conventional_spellings() {
        for v in ["1", "true", "TRUE", " yes ", "on", "On"] {
            assert!(Config::flag_is_truthy(v), "{v:?} should be truthy");
        }
    }

    #[test]
    fn truthy_rejects_falsey_and_garbage() {
        for v in ["0", "false", "no", "off", "", "2", "enable"] {
            assert!(!Config::flag_is_truthy(v), "{v:?} should not be truthy");
        }
    }
}
