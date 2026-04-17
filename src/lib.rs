//! mreg-rust: DNS and network inventory management API.
//!
//! This crate provides a REST API for managing DNS zones, hosts, records,
//! networks, and related infrastructure. It supports pluggable storage
//! backends (in-memory for testing, PostgreSQL for production) with
//! atomic cascading side-effects and RFC-aware DNS record validation.

pub mod api;
pub mod audit;
pub mod authn;
pub mod authz;
pub mod config;
pub mod db;
pub mod domain;
pub mod errors;
pub mod events;
pub mod exports;
pub mod imports;
pub mod middleware;
pub mod services;
pub mod storage;
pub mod tasks;
pub mod workers;

use std::{io, sync::Arc};

use actix_web::{App, HttpServer, web};
use tracing::info;
use tracing_actix_web::TracingLogger;

use crate::middleware::MregRootSpan;

use crate::{
    authn::AuthnClient,
    authz::AuthorizerClient,
    config::Config,
    errors::AppError,
    events::EventSinkClient,
    services::Services,
    storage::{ReadableStorage, build_storage},
};

/// Compile-time build metadata embedded in API health responses.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BuildInfo {
    pub package_name: &'static str,
    pub version: &'static str,
    pub git_sha: Option<&'static str>,
}

impl BuildInfo {
    /// Capture build information from Cargo environment variables.
    pub fn current() -> Self {
        Self {
            package_name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            git_sha: option_env!("MREG_GIT_SHA"),
        }
    }
}

/// Shared application state injected into every HTTP handler via `web::Data`.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub build_info: BuildInfo,
    pub reader: ReadableStorage,
    pub services: Services,
    pub authn: AuthnClient,
    pub authz: AuthorizerClient,
}

/// Bootstrap configuration, storage, authorization, and start the HTTP server.
pub async fn run() -> io::Result<()> {
    let config = Config::from_env().map_err(to_io_error)?;
    init_tracing(&config).map_err(to_io_error)?;

    let storage = build_storage(&config).map_err(to_io_error)?;
    let storage_backend = storage.backend_kind();
    let authn = AuthnClient::from_config(&config, storage.clone()).map_err(to_io_error)?;
    let authz = AuthorizerClient::from_config(&config).map_err(to_io_error)?;
    let events = EventSinkClient::from_config(&config);
    let reader = ReadableStorage::new(storage.clone());

    // Background task: prune expired revoked_tokens rows hourly.
    let prune_storage = storage.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = prune_storage.auth_sessions().prune_expired_tokens().await {
                tracing::warn!(error = %e, "failed to prune expired revoked tokens");
            }
        }
    });

    let services = Services::new(storage, events.clone());
    let state = AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        reader,
        services,
        authn,
        authz,
    };

    let bind_addr = state.config.bind_addr();
    let worker_count = state.config.workers.unwrap_or_else(num_cpus_hint);
    let app_state = state.clone();

    let build = BuildInfo::current();
    let authn_mode = match state.config.auth_mode {
        crate::config::AuthMode::None => "none",
        crate::config::AuthMode::Scoped => "scoped",
    };
    let authz_mode = if state.config.treetop_url.is_some() {
        "treetop"
    } else if state.config.allow_dev_authz_bypass {
        "bypass (all allowed)"
    } else {
        "deny (all denied)"
    };

    let mut event_sinks = Vec::new();
    if state.config.event_webhook_url.is_some() {
        event_sinks.push("webhook");
    }
    #[cfg(feature = "amqp")]
    if state.config.event_amqp_url.is_some() {
        event_sinks.push("amqp");
    }
    #[cfg(feature = "redis")]
    if state.config.event_redis_url.is_some() {
        event_sinks.push("redis");
    }
    let event_sinks_str = if event_sinks.is_empty() {
        "none".to_string()
    } else {
        event_sinks.join(", ")
    };

    info!(
        version = build.version,
        git_sha = build.git_sha.unwrap_or("unknown"),
        address = %bind_addr,
        workers = worker_count,
        storage_backend = ?storage_backend,
        database_configured = state.config.database_url.is_some(),
        authn_mode = authn_mode,
        authz_mode = authz_mode,
        event_sinks = %event_sinks_str,
        json_logs = state.config.json_logs,
        run_migrations = state.config.run_migrations,
        "starting mreg-rust"
    );

    let trust_proxy = state.config.auth_login_trust_proxy_headers;
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .app_data(api::json_config(app_state.config.json_payload_limit_bytes))
            .wrap(TracingLogger::<MregRootSpan>::new())
            .wrap(middleware::Authn)
            .wrap(middleware::RequestId)
            .configure(move |cfg| api::configure(cfg, trust_proxy))
    })
    .workers(worker_count)
    .bind(bind_addr)?
    .run()
    .await
}

fn init_tracing(config: &Config) -> Result<(), AppError> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,actix_web=info"));
    let builder = tracing_subscriber::fmt().with_env_filter(env_filter);

    if config.json_logs {
        builder.json().try_init().map_err(AppError::internal)?;
    } else {
        builder.try_init().map_err(AppError::internal)?;
    }

    Ok(())
}

fn num_cpus_hint() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

fn to_io_error(error: AppError) -> io::Error {
    io::Error::other(error.to_string())
}
