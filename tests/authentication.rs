use std::sync::Arc;

use actix_web::{App, body::to_bytes, http::StatusCode, test, web};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use mreg_rust::{
    AppState, BuildInfo,
    authn::AuthnClient,
    authz::AuthorizerClient,
    config::{
        AuthMode, AuthScopeBackendConfig, AuthScopeConfig, Config, LocalUserConfig,
        StorageBackendSetting,
    },
    events::EventSinkClient,
    middleware,
    services::Services,
    storage::ReadableStorage,
    storage::build_storage,
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

fn base_config() -> Config {
    Config {
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: true,
        ..Config::default()
    }
}

fn build_state(config: Config) -> AppState {
    let storage = build_storage(&config).expect("memory storage");
    let authn = AuthnClient::from_config(&config, storage.clone()).expect("authn config");
    let authz = AuthorizerClient::from_config(&config).expect("authz config");
    AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        reader: ReadableStorage::new(storage.clone()),
        services: Services::new(storage, EventSinkClient::noop()),
        authn,
        authz,
    }
}

async fn call_json(request: actix_http::Request, state: AppState) -> (StatusCode, Value) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .wrap(middleware::Authn)
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    let response = test::call_service(&app, request).await;
    let status = response.status();
    let bytes = to_bytes(response.into_body()).await.expect("body bytes");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, body)
}

#[derive(Clone, Serialize)]
struct RemoteClaims {
    sub: String,
    groups: Vec<String>,
    iss: String,
    exp: i64,
}

fn remote_token(secret: &str, issuer: &str, sub: &str, groups: &[&str]) -> String {
    encode(
        &Header::new(Algorithm::HS256),
        &RemoteClaims {
            sub: sub.to_string(),
            groups: groups.iter().map(|group| group.to_string()).collect(),
            iss: issuer.to_string(),
            exp: (Utc::now() + Duration::minutes(5)).timestamp(),
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("remote token")
}

fn local_password_hash(password: &str) -> String {
    let salt = SaltString::encode_b64(b"static-local-salt").expect("salt");
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("password hash")
        .to_string()
}

fn local_scope(name: &str) -> AuthScopeConfig {
    AuthScopeConfig {
        name: name.to_string(),
        backend: AuthScopeBackendConfig::Local {
            users: vec![LocalUserConfig {
                username: "admin".to_string(),
                password_hash: local_password_hash("secret"),
                groups: vec!["admins".to_string(), "ops".to_string()],
            }],
        },
    }
}

fn remote_scope(name: &str, login_url: String, issuer: &str, secret: &str) -> AuthScopeConfig {
    AuthScopeConfig {
        name: name.to_string(),
        backend: AuthScopeBackendConfig::Remote {
            login_url,
            timeout_ms: 5000,
            default_service_name: None,
            jwt_issuer: issuer.to_string(),
            jwt_audience: None,
            jwks_url: None,
            jwt_public_key_pem: None,
            jwt_hmac_secret: Some(secret.to_string()),
            username_claim: "sub".to_string(),
            groups_claim: "groups".to_string(),
        },
    }
}

struct MockHttpServer {
    url: String,
    handle: JoinHandle<()>,
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn spawn_mock_server(status: u16, body: String) -> MockHttpServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let response_body = body.clone();
            tokio::spawn(async move {
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer).await;
                let reason = match status {
                    200 => "OK",
                    401 => "Unauthorized",
                    500 => "Internal Server Error",
                    _ => "OK",
                };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    MockHttpServer {
        url: format!("http://{addr}/login"),
        handle,
    }
}

#[actix_web::test]
async fn none_mode_keeps_header_based_identity_and_disables_login() {
    let state = build_state(base_config());

    let (status, body) = call_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("X-Mreg-User", "alice"))
            .insert_header(("X-Mreg-Groups", "ops,net"))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["principal"]["id"], "alice");
    assert_eq!(body["principal"]["username"], "alice");
    assert_eq!(body["principal"]["groups"], json!(["ops", "net"]));
    assert!(body["auth_scope"].is_null());
    assert!(body["auth_provider_kind"].is_null());

    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"alice","password":"secret"}))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "service_unavailable");
}

#[actix_web::test]
async fn local_scope_login_issues_prefixed_identity() {
    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![local_scope("local")];

    let state = build_state(config);
    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"local:admin","password":"secret"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"].as_str().expect("token").to_string();
    assert_eq!(body["principal"]["id"], "local:admin");
    assert_eq!(body["principal"]["username"], "admin");
    assert_eq!(
        body["principal"]["groups"],
        json!(["local:admins", "local:ops"])
    );
    assert_eq!(body["auth_scope"], "local");
    assert_eq!(body["auth_provider_kind"], "local");

    let (status, body) = call_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["principal"]["id"], "local:admin");
    assert_eq!(body["principal"]["username"], "admin");
    assert_eq!(body["auth_scope"], "local");
    assert_eq!(body["auth_provider_kind"], "local");
}

#[actix_web::test]
async fn remote_scope_login_returns_mreg_token_and_me_uses_bearer() {
    let issuer = "auth.example";
    let secret = "remote-secret";
    let upstream_token = remote_token(secret, issuer, "alice", &["ops", "net"]);
    let server = spawn_mock_server(
        200,
        json!({ "access_token": upstream_token.clone() }).to_string(),
    )
    .await;

    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![remote_scope(
        "remote-sso",
        server.url.clone(),
        issuer,
        secret,
    )];

    let state = build_state(config);
    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"remote-sso:alice","password":"secret"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"].as_str().expect("token").to_string();
    assert_ne!(access_token, upstream_token);
    assert_eq!(body["principal"]["id"], "remote-sso:alice");
    assert_eq!(body["principal"]["username"], "alice");
    assert_eq!(
        body["principal"]["groups"],
        json!(["remote-sso:ops", "remote-sso:net"])
    );
    assert_eq!(body["auth_scope"], "remote-sso");
    assert_eq!(body["auth_provider_kind"], "remote");

    let (status, body) = call_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["principal"]["id"], "remote-sso:alice");
    assert_eq!(body["principal"]["username"], "alice");
    assert_eq!(body["auth_scope"], "remote-sso");
    assert_eq!(body["auth_provider_kind"], "remote");
}

#[actix_web::test]
async fn scoped_mode_requires_bearer_and_ignores_identity_headers() {
    let issuer = "auth.example";
    let secret = "remote-secret";
    let upstream_token = remote_token(secret, issuer, "alice", &["ops"]);
    let server =
        spawn_mock_server(200, json!({ "access_token": upstream_token }).to_string()).await;

    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![remote_scope(
        "remote-sso",
        server.url.clone(),
        issuer,
        secret,
    )];

    let state = build_state(config.clone());
    let (status, _) = call_json(
        test::TestRequest::get()
            .uri("/system/status")
            .insert_header(("X-Mreg-User", "forged"))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, login_body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"remote-sso:alice","password":"secret"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = login_body["access_token"].as_str().unwrap().to_string();

    let (status, body) = call_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .insert_header(("X-Mreg-User", "forged"))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["principal"]["id"], "remote-sso:alice");
}

#[actix_web::test]
async fn scoped_mode_rejects_missing_or_unknown_scope() {
    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![local_scope("local")];

    let state = build_state(config.clone());
    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"admin","password":"secret"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "validation_error");

    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"unknown:admin","password":"secret"}))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "validation_error");
}

#[actix_web::test]
async fn remote_scope_propagates_invalid_credentials() {
    let server = spawn_mock_server(401, "{}".to_string()).await;

    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![remote_scope(
        "remote-sso",
        server.url.clone(),
        "auth.example",
        "remote-secret",
    )];

    let state = build_state(config);
    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"remote-sso:alice","password":"bad"}))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
}

#[actix_web::test]
async fn scoped_mode_health_and_version_stay_unauthenticated() {
    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![local_scope("local")];

    let state = build_state(config);
    let (status, _) = call_json(
        test::TestRequest::get().uri("/system/health").to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call_json(
        test::TestRequest::get().uri("/system/version").to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[actix_web::test]
async fn scoped_mode_logout_revokes_current_token() {
    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![local_scope("local")];

    let state = build_state(config);
    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"local:admin","password":"secret"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"].as_str().unwrap().to_string();

    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/logout")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    let (status, body) = call_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
}

#[actix_web::test]
async fn logout_all_revokes_existing_tokens_for_the_principal() {
    let mut config = base_config();
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![local_scope("local")];

    let state = build_state(config);
    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"local:admin","password":"secret"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"].as_str().unwrap().to_string();

    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/logout-all")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .set_json(json!({"principal_id":"local:admin"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    let (status, body) = call_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
}

#[actix_web::test]
async fn logout_all_requires_authorization() {
    let mut config = base_config();
    config.allow_dev_authz_bypass = false;
    config.auth_mode = AuthMode::Scoped;
    config.auth_jwt_signing_key = Some("jwt-signing-secret".to_string());
    config.auth_scopes = vec![local_scope("local")];

    let state = build_state(config);
    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":"local:admin","password":"secret"}))
            .to_request(),
        state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"].as_str().unwrap().to_string();

    let (status, body) = call_json(
        test::TestRequest::post()
            .uri("/auth/logout-all")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .set_json(json!({"principal_id":"local:admin"}))
            .to_request(),
        state,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden");
}
