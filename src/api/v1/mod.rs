//! Compatibility surface for the original Django mreg API.
//!
//! The old API and the native Rust API do not share resource identifiers or
//! representations.  Where the underlying operation has an unambiguous v2
//! equivalent, this module rewrites the legacy path internally and lets the v2
//! handler perform authentication, authorization, validation, and persistence.
//! Routes which cannot be represented honestly return 501 with an explanation.

use std::{
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

use actix_web::{
    HttpRequest, HttpResponse, Result,
    body::MessageBody,
    dev::{HttpServiceFactory, ServiceResponse},
    http::StatusCode,
    middleware::{ErrorHandlerResponse, ErrorHandlers},
    web,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::{AppState, authn, errors::AppError};

mod reads;
mod resources;

const PREFIX: &str = "/api/v1";
const UNAVAILABLE: &str = "/_legacy-unavailable";

pub(super) fn legacy_id(id: Uuid) -> u32 {
    (u32::from_be_bytes(id.as_bytes()[..4].try_into().expect("four UUID bytes")) & 0x7fff_ffff)
        .saturating_add(1)
}

pub(super) fn legacy_name_id(name: &str) -> u32 {
    let hash = name.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    });
    (hash & 0x7fff_ffff).saturating_add(1)
}

/// Build the legacy `/api/v1/` scope.
pub fn scope(_trust_proxy_headers: bool) -> impl HttpServiceFactory {
    web::scope(PREFIX)
        .configure(reads::configure)
        .configure(resources::configure)
        .service(web::resource("/{tail:.*}").route(web::to(dispatch_legacy)))
        .wrap(ErrorHandlers::new().handler(StatusCode::NOT_FOUND, legacy_not_found))
}

fn legacy_not_found<B: MessageBody + 'static>(
    response: ServiceResponse<B>,
) -> Result<ErrorHandlerResponse<B>> {
    let (request, _) = response.into_parts();
    let response = HttpResponse::NotFound().json(serde_json::json!({
        "type": "client_error",
        "errors": [{
            "code": "not_found",
            "detail": "Not found.",
            "attr": serde_json::Value::Null,
        }],
    }));
    Ok(ErrorHandlerResponse::Response(
        ServiceResponse::new(request, response).map_into_right_body(),
    ))
}

/// Configure the historical, unversioned endpoints from `mreg/api/urls.py`.
pub fn configure_unversioned(cfg: &mut web::ServiceConfig) {
    // Register absolute resources instead of a broad `/api` scope. Actix
    // scopes are prefix matches, so such a scope would swallow `/api/v1` and
    // `/api/v2` before their versioned routers get a chance to match.
    cfg.route("/api/token-auth/", web::post().to(token_auth))
        .route("/api/token-logout/", web::post().to(token_logout))
        .route("/api/token-is-valid/", web::get().to(token_is_valid))
        .route("/api/meta/user", web::get().to(user_info))
        .route("/api/meta/version", web::get().to(version))
        .route("/api/meta/libraries", web::get().to(libraries))
        .route("/api/meta/metrics", web::get().to(unavailable))
        .route("/api/meta/health/heartbeat", web::get().to(heartbeat))
        .route("/api/meta/health/ldap", web::get().to(unavailable));
}

#[derive(Deserialize)]
struct LegacyLoginRequest {
    username: String,
    password: String,
    service_name: Option<String>,
    otp_code: Option<String>,
}

async fn token_auth(
    state: web::Data<AppState>,
    body: web::Form<LegacyLoginRequest>,
) -> Result<HttpResponse, AppError> {
    if !state.authn.requires_bearer_auth() {
        // Header-trust mode has no token issuer. Returning a compatibility
        // token lets legacy clients operate in local/CI deployments; the
        // authentication middleware intentionally ignores it in this mode.
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "token": "mreg-rust-header-trust"
        })));
    }
    let username = if body.username.contains(':') {
        body.username.clone()
    } else {
        format!("local:{}", body.username)
    };
    let session = state
        .authn
        .login(authn::LoginRequest {
            username,
            password: body.password.clone(),
            service_name: body.service_name.clone(),
            otp_code: body.otp_code.clone(),
        })
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": session.access_token })))
}

async fn token_logout(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let context = current_principal(&req, &state)?;
    state.authn.logout(&context).await?;
    // Django mreg returns 200 with an empty body, rather than v2's 204.
    Ok(HttpResponse::Ok().finish())
}

async fn token_is_valid() -> HttpResponse {
    // The authentication middleware has already validated the bearer token.
    HttpResponse::Ok().finish()
}

async fn user_info(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let context = current_principal(&req, &state)?;
    let groups = context
        .principal
        .groups
        .iter()
        .map(|group| group.key())
        .collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "username": context.username,
        "last_login": serde_json::Value::Null,
        "token": {
            "is_valid": true,
            "created": context.issued_at,
            "expire": context.expires_at,
            "last_used": serde_json::Value::Null,
            "lifespan": (context.expires_at - Utc::now()).to_string(),
        },
        "django_status": {
            "superuser": false,
            "staff": false,
            "active": true,
        },
        "mreg_status": {
            "superuser": false,
            "admin": false,
            "group_admin": false,
            "network_admin": false,
            "hostpolicy_admin": false,
            "dns_wildcard_admin": false,
            "underscore_admin": false,
        },
        "groups": groups,
        "permissions": [],
    })))
}

async fn version(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "version": state.build_info.version }))
}

async fn libraries() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "implementation": "rust",
        "mreg-rust": env!("CARGO_PKG_VERSION"),
        "actix-web": "4",
        "utoipa": "5",
    }))
}

async fn heartbeat() -> HttpResponse {
    static START_TIME: OnceLock<u64> = OnceLock::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let start_time = *START_TIME.get_or_init(|| now);
    HttpResponse::Ok().json(serde_json::json!({
        "start_time": start_time,
        "uptime": now.saturating_sub(start_time),
    }))
}

async fn unavailable(req: HttpRequest) -> HttpResponse {
    HttpResponse::NotImplemented().json(serde_json::json!({
        "detail": "The legacy route has no safe mreg-rust v2 equivalent",
        "method": req.method().as_str(),
        "path": req.path(),
    }))
}

fn current_principal(
    req: &HttpRequest,
    state: &AppState,
) -> Result<authn::PrincipalContext, AppError> {
    if let Some(context) = authn::principal_context(req) {
        return Ok(context);
    }
    if state.config.trusts_identity_headers() || !state.authn.requires_bearer_auth() {
        return Ok(authn::PrincipalContext::headers(
            authn::header_principal(req),
            Utc::now(),
        ));
    }
    Err(AppError::unauthorized("authentication required"))
}

async fn dispatch_legacy(req: HttpRequest) -> HttpResponse {
    let legacy_path = req.path().strip_prefix(PREFIX).unwrap_or(req.path());
    let Some(target) = rewrite_legacy_path(legacy_path) else {
        return HttpResponse::NotFound().finish();
    };
    HttpResponse::NotImplemented().json(serde_json::json!({
        "detail": "This legacy endpoint requires a dedicated v1 contract adapter",
        "method": req.method().as_str(),
        "path": req.path(),
        "closest_v2_path": (target != UNAVAILABLE).then(|| format!("/api/v2{target}")),
    }))
}

/// Map original mreg paths to the closest native v2 operation.
///
/// A `None` result means the path is not a known legacy route. Known routes
/// without a sound v2 equivalent map to the explicit 501 handler.
fn rewrite_legacy_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };

    let simple = [
        ("/bacnet/ids", "/inventory/bacnet-ids"),
        ("/hosts", "/inventory/hosts"),
        ("/hostgroups", "/inventory/host-groups"),
        ("/ipaddresses", "/inventory/ip-addresses"),
        ("/labels", "/inventory/labels"),
        ("/nameservers", "/dns/nameservers"),
        ("/ptroverrides", "/dns/ptr-overrides"),
        ("/networks", "/inventory/networks"),
        ("/zones/forward", "/dns/forward-zones"),
        ("/zones/reverse", "/dns/reverse-zones"),
        ("/networkpolicies", "/policy/network/policies"),
        ("/hostpolicy/atoms", "/policy/host/atoms"),
        ("/hostpolicy/roles", "/policy/host/roles"),
        ("/history", "/system/history"),
    ];
    if let Some((_, target)) = simple.iter().find(|(source, _)| path == *source) {
        return Some((*target).to_string());
    }

    if let Some(name) = path.strip_prefix("/labels/name/") {
        return Some(format!("/inventory/labels/{name}"));
    }
    if let Some(rest) = map_detail(path, "/bacnet/ids/", "/inventory/bacnet-ids/") {
        return Some(rest);
    }
    if let Some(rest) = map_detail(path, "/hosts/", "/inventory/hosts/") {
        if rest.ends_with("/contacts") {
            return Some(UNAVAILABLE.to_string());
        }
        return Some(rest);
    }
    if let Some(rest) = map_detail(path, "/hostgroups/", "/inventory/host-groups/") {
        if ["/groups", "/hosts", "/owners"]
            .iter()
            .any(|part| rest.contains(part))
        {
            return Some(UNAVAILABLE.to_string());
        }
        return Some(rest);
    }
    if let Some(rest) = map_detail(path, "/ipaddresses/", "/inventory/ip-addresses/") {
        return Some(rest);
    }
    if let Some(rest) = map_detail(path, "/labels/", "/inventory/labels/") {
        return Some(rest);
    }
    if let Some(rest) = map_detail(path, "/nameservers/", "/dns/nameservers/") {
        return Some(rest);
    }
    if let Some(rest) = map_detail(path, "/ptroverrides/", "/dns/ptr-overrides/") {
        return Some(rest);
    }

    if let Some(mapped) = rewrite_record_path(path) {
        return Some(mapped);
    }
    if let Some(mapped) = rewrite_network_path(path) {
        return Some(mapped);
    }
    if let Some(mapped) = rewrite_zone_path(path, "forward") {
        return Some(mapped);
    }
    if let Some(mapped) = rewrite_zone_path(path, "reverse") {
        return Some(mapped);
    }

    if let Some(rest) = map_detail(path, "/networkpolicies/", "/policy/network/policies/") {
        return Some(rest);
    }
    if path.starts_with("/networkpolicyattributes") {
        return Some(UNAVAILABLE.to_string());
    }
    if path.starts_with("/history/") {
        return Some(UNAVAILABLE.to_string());
    }
    if path.starts_with("/permissions/netgroupregex") || path.starts_with("/dhcphosts") {
        return Some(UNAVAILABLE.to_string());
    }
    if path.starts_with("/zonefiles/") || path.starts_with("/zones/forward/hostname/") {
        return Some(UNAVAILABLE.to_string());
    }
    if let Some(mapped) = rewrite_host_policy_path(path) {
        return Some(mapped);
    }

    None
}

fn map_detail(path: &str, source: &str, target: &str) -> Option<String> {
    path.strip_prefix(source)
        .map(|value| format!("{target}{value}"))
}

fn rewrite_record_path(path: &str) -> Option<String> {
    const TYPES: [(&str, &str); 8] = [
        ("cnames", "CNAME"),
        ("hinfos", "HINFO"),
        ("locs", "LOC"),
        ("mxs", "MX"),
        ("naptrs", "NAPTR"),
        ("sshfps", "SSHFP"),
        ("srvs", "SRV"),
        ("txts", "TXT"),
    ];
    for (legacy, record_type) in TYPES {
        let prefix = format!("/{legacy}");
        if path == prefix {
            return Some(format!("/dns/records?type_name={record_type}"));
        }
        if let Some(id) = path.strip_prefix(&format!("{prefix}/")) {
            // The old API uses integer primary keys while v2 uses UUIDs. Keeping
            // the route wired gives UUID-aware migrations a useful bridge; old
            // integer identifiers fail validation rather than targeting bad data.
            return Some(format!("/dns/records/{id}"));
        }
    }
    None
}

fn rewrite_network_path(path: &str) -> Option<String> {
    if let Some(ip) = path.strip_prefix("/networks/ip/") {
        return Some(format!("/inventory/networks?contains_ip={ip}"));
    }
    let rest = path.strip_prefix("/networks/")?;
    let (cidr, suffix) = split_cidr_suffix(rest)?;
    let target = match suffix {
        "" => format!("/inventory/networks/{cidr}"),
        "/excluded_ranges" => format!("/inventory/networks/{cidr}/excluded-ranges"),
        value if value.starts_with("/excluded_ranges/") => UNAVAILABLE.to_string(),
        "/used_list" | "/used_host_list" => format!("/inventory/networks/{cidr}/used_addresses"),
        "/unused_list" => format!("/inventory/networks/{cidr}/unused_addresses"),
        "/first_unused" => format!("/inventory/networks/{cidr}/unused_addresses?limit=1"),
        "/random_unused"
        | "/reserved_list"
        | "/used_count"
        | "/unused_count"
        | "/ptroverride_list"
        | "/ptroverride_host_list" => UNAVAILABLE.to_string(),
        value if value.starts_with("/communities") => rewrite_network_community(cidr, value),
        _ => return None,
    };
    Some(target)
}

fn split_cidr_suffix(value: &str) -> Option<(&str, &str)> {
    let slash = value.find('/')?;
    let after_prefix = &value[slash + 1..];
    let suffix_start = after_prefix.find('/').map(|index| slash + 1 + index);
    match suffix_start {
        Some(index) => Some((&value[..index], &value[index..])),
        None => Some((value, "")),
    }
}

fn rewrite_network_community(cidr: &str, suffix: &str) -> String {
    if suffix == "/communities" {
        format!("/policy/network/communities?network={cidr}")
    } else if suffix.contains("/hosts") {
        UNAVAILABLE.to_string()
    } else if let Some(id) = suffix.strip_prefix("/communities/") {
        format!("/policy/network/communities/{id}")
    } else {
        UNAVAILABLE.to_string()
    }
}

fn rewrite_zone_path(path: &str, kind: &str) -> Option<String> {
    let prefix = format!("/zones/{kind}/");
    let rest = path.strip_prefix(&prefix)?;
    if rest.is_empty() {
        return Some(format!("/dns/{kind}-zones"));
    }
    if let Some((zone, delegation)) = rest.split_once("/delegations/") {
        if delegation.is_empty() {
            return Some(format!("/dns/{kind}-zones/{zone}/delegations"));
        }
        return Some(UNAVAILABLE.to_string());
    }
    if let Some(zone) = rest.strip_suffix("/delegations") {
        return Some(format!("/dns/{kind}-zones/{zone}/delegations"));
    }
    if rest.ends_with("/nameservers") {
        return Some(UNAVAILABLE.to_string());
    }
    Some(format!("/dns/{kind}-zones/{rest}"))
}

fn rewrite_host_policy_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/hostpolicy/")?;
    let target = format!("/policy/host/{rest}");
    if (target.ends_with("/atoms") || target.ends_with("/hosts"))
        && target.matches('/').count() >= 5
    {
        return Some(UNAVAILABLE.to_string());
    }
    Some(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{
        App,
        http::{StatusCode, header},
        test, web,
    };
    use crate::middleware::Authn;

    #[actix_web::test]
    async fn rewrites_core_legacy_routes() {
        assert_eq!(
            rewrite_legacy_path("/hosts/"),
            Some("/inventory/hosts".into())
        );
        assert_eq!(
            rewrite_legacy_path("/cnames/"),
            Some("/dns/records?type_name=CNAME".into())
        );
        assert_eq!(
            rewrite_legacy_path("/networks/10.0.0.0/24/unused_list"),
            Some("/inventory/networks/10.0.0.0/24/unused_addresses".into())
        );
        assert_eq!(
            rewrite_legacy_path("/zones/forward/example.org/delegations/"),
            Some("/dns/forward-zones/example.org/delegations".into())
        );
    }

    #[actix_web::test]
    async fn marks_unrepresentable_routes_explicitly() {
        assert_eq!(
            rewrite_legacy_path("/zonefiles/example.org"),
            Some(UNAVAILABLE.into())
        );
    }

    #[actix_web::test]
    async fn legacy_host_list_is_never_redirected_to_a_different_contract() {
        let state = super::super::v2::tests::test_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(|cfg| super::super::configure(cfg, false)),
        )
        .await;
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/hosts/")
                .insert_header((header::AUTHORIZATION, "Token test"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get(header::LOCATION).is_none());
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["count"], 0);
        assert_eq!(body["results"], serde_json::json!([]));
    }

    #[actix_web::test]
    async fn legacy_dhcp_export_is_built_from_stored_assignments() {
        let state = super::super::v2::tests::test_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(|cfg| super::super::configure(cfg, false)),
        )
        .await;
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/dhcphosts/ipv4/")
                .insert_header((header::AUTHORIZATION, "Token test"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body, serde_json::json!([]));
    }

    #[actix_web::test]
    async fn legacy_network_counts_are_computed_directly() {
        let state = super::super::v2::tests::test_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(|cfg| super::super::configure(cfg, false)),
        )
        .await;
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v2/inventory/networks")
                .set_json(serde_json::json!({
                    "cidr": "10.40.0.0/24",
                    "description": "legacy-read-test"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/networks/10.40.0.0/24/used_count")
                .insert_header((header::AUTHORIZATION, "Token test"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: usize = test::read_body_json(response).await;
        assert_eq!(body, 0);
    }

    #[actix_web::test]
    async fn legacy_zone_and_network_mutations_establish_cli_state() {
        let state = super::super::v2::tests::test_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(|cfg| super::super::configure(cfg, false)),
        )
        .await;
        let auth = (header::AUTHORIZATION, "Token test");

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/zones/forward/")
                .insert_header(auth.clone())
                .set_json(serde_json::json!({
                    "name": "example.org",
                    "email": "hostmaster@example.org",
                    "primary_ns": ["ns1.example.org"]
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri("/api/v1/zones/forward/example.org/nameservers")
                .insert_header(auth.clone())
                .set_json(serde_json::json!({"primary_ns": ["ns2.example.org"]}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/networks/")
                .insert_header(auth.clone())
                .set_json(serde_json::json!({
                    "network": "10.0.2.0/28", "description": "TinyNet", "frozen": false
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri("/api/v1/networks/10.0.2.0/28")
                .insert_header(auth)
                .set_json(serde_json::json!({"reserved": 8}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[actix_web::test]
    async fn openapi_document_only_advertises_native_v2_paths() {
        use utoipa::OpenApi;

        let document = serde_json::to_value(super::super::V2ApiDoc::openapi()).unwrap();
        let paths = document["paths"].as_object().unwrap();
        assert!(!paths.is_empty());
        assert!(paths.keys().all(|path| path.starts_with("/api/v2/")));
        assert_eq!(document["info"]["version"], "2.0.0");
    }

    #[actix_web::test]
    async fn pr_compatible_schema_location_serves_v2_openapi() {
        let state = super::super::v2::tests::test_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(|cfg| super::super::configure(cfg, false)),
        )
        .await;
        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/docs/schema").to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["info"]["version"], "2.0.0");

        let response =
            test::call_service(&app, test::TestRequest::get().uri("/docs").to_request()).await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/docs/");
    }

    #[actix_web::test]
    async fn audit_history_uses_authenticated_request_actor() {
        let state = super::super::v2::tests::test_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .wrap(Authn)
                .configure(|cfg| super::super::configure(cfg, false)),
        )
        .await;

        let create = test::TestRequest::post()
            .uri("/api/v2/inventory/labels")
            .insert_header(("X-Mreg-User", "alice"))
            .set_json(serde_json::json!({"name": "actor-test", "description": "test"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, create).await.status(),
            StatusCode::CREATED
        );

        let history = test::TestRequest::get()
            .uri("/api/v2/system/history")
            .insert_header(("X-Mreg-User", "alice"))
            .to_request();
        let response = test::call_service(&app, history).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["items"][0]["actor"], "alice");
    }
}
