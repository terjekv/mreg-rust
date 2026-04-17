//! Operator validation tests (field type rejection, unknown fields, numeric, cidr,
//! datetime, and ancillary endpoint operator tests).

use std::sync::Arc;

use actix_web::{App, http::StatusCode, test, web};
use mreg_rust::{
    AppState, BuildInfo,
    authn::AuthnClient,
    authz::AuthorizerClient,
    config::{Config, StorageBackendSetting},
    events::EventSinkClient,
    services::Services,
    storage::ReadableStorage,
    storage::build_storage,
};
use rstest::rstest;
use serde_json::Value;

fn app_state() -> AppState {
    let config = Config {
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: true,
        ..Config::default()
    };
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

// ═══════════════════════════════════════════════════════
// OPERATOR VALIDATION: field type restrictions
// ═══════════════════════════════════════════════════════

/// Operators that should be REJECTED for datetime fields (only equals/gt/gte/lt/lte/is_null allowed).
#[rstest]
#[case::contains("contains")]
#[case::icontains("icontains")]
#[case::startswith("startswith")]
#[case::endswith("endswith")]
#[case::iequals("iequals")]
#[case::in_op("in")]
#[actix_web::test]
async fn datetime_field_rejects_invalid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/hosts?created_at__{op}=2024-01-01"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "created_at__{op} should be rejected"
    );
}

/// Operators that should be ACCEPTED for datetime fields.
#[rstest]
#[case::equals("equals")]
#[case::gt("gt")]
#[case::gte("gte")]
#[case::lt("lt")]
#[case::lte("lte")]
#[case::is_null("is_null")]
#[case::not_equals("not_equals")]
#[case::not_gt("not_gt")]
#[actix_web::test]
async fn datetime_field_accepts_valid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/inventory/hosts?created_at__{op}=2024-01-01T00:00:00Z"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "created_at__{op} should be accepted"
    );
}

/// Enum fields (family, owner_kind) reject string-only operators.
#[rstest]
#[case::contains("contains")]
#[case::icontains("icontains")]
#[case::startswith("startswith")]
#[case::endswith("endswith")]
#[case::gt("gt")]
#[case::lt("lt")]
#[actix_web::test]
async fn enum_field_rejects_invalid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/networks?family__{op}=4"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "family__{op} should be rejected"
    );
}

/// Enum fields accept equals and in.
#[rstest]
#[case::equals("equals")]
#[case::in_op("in")]
#[case::is_null("is_null")]
#[case::not_equals("not_equals")]
#[actix_web::test]
async fn enum_field_accepts_valid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/networks?family__{op}=4"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "family__{op} should be accepted"
    );
}

// ═══════════════════════════════════════════════════════
// SYSTEMATIC VALIDATION: every endpoint rejects unknown fields
// ═══════════════════════════════════════════════════════

#[rstest]
#[case::hosts("/inventory/hosts")]
#[case::networks("/inventory/networks")]
#[case::records("/dns/records")]
#[case::host_contacts("/inventory/host-contacts")]
#[case::host_groups("/inventory/host-groups")]
#[case::bacnet_ids("/inventory/bacnet-ids")]
#[case::ptr_overrides("/dns/ptr-overrides")]
#[case::network_policies("/policy/network/policies")]
#[case::communities("/policy/network/communities")]
#[case::host_community_assignments("/policy/network/host-community-assignments")]
#[actix_web::test]
async fn every_filterable_endpoint_rejects_unknown_field(#[case] endpoint: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("{endpoint}?totally_bogus__equals=foo"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "{endpoint} should reject unknown field"
    );
}

#[rstest]
#[case::hosts("/inventory/hosts")]
#[case::networks("/inventory/networks")]
#[case::records("/dns/records")]
#[case::host_contacts("/inventory/host-contacts")]
#[case::host_groups("/inventory/host-groups")]
#[case::bacnet_ids("/inventory/bacnet-ids")]
#[case::ptr_overrides("/dns/ptr-overrides")]
#[case::network_policies("/policy/network/policies")]
#[case::communities("/policy/network/communities")]
#[case::host_community_assignments("/policy/network/host-community-assignments")]
#[actix_web::test]
async fn every_filterable_endpoint_rejects_unknown_operator(#[case] endpoint: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // Use first valid field per endpoint, with garbage operator
    let field = match endpoint {
        "/inventory/hosts" => "name",
        "/inventory/networks" => "description",
        "/dns/records" => "type_name",
        "/inventory/host-contacts" => "email",
        "/inventory/host-groups" => "name",
        "/inventory/bacnet-ids" => "host",
        "/dns/ptr-overrides" => "host",
        "/policy/network/policies" => "name",
        "/policy/network/communities" => "name",
        "/policy/network/host-community-assignments" => "host",
        _ => unreachable!(),
    };
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("{endpoint}?{field}__garbage_op=foo"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "{endpoint} should reject unknown operator on {field}"
    );
}

// ═══════════════════════════════════════════════════════
// NUMERIC FIELD VALIDATION (bacnet_id)
// ═══════════════════════════════════════════════════════

/// Numeric fields reject string-only operators.
#[rstest]
#[case::contains("contains")]
#[case::icontains("icontains")]
#[case::startswith("startswith")]
#[case::endswith("endswith")]
#[case::iequals("iequals")]
#[actix_web::test]
async fn numeric_field_rejects_invalid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/bacnet-ids?bacnet_id__{op}=100"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "bacnet_id__{op} should be rejected"
    );
}

/// Numeric fields accept comparison and equality operators.
#[rstest]
#[case::equals("equals")]
#[case::gt("gt")]
#[case::gte("gte")]
#[case::lt("lt")]
#[case::lte("lte")]
#[case::in_op("in")]
#[case::is_null("is_null")]
#[case::not_equals("not_equals")]
#[actix_web::test]
async fn numeric_field_accepts_valid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/bacnet-ids?bacnet_id__{op}=100"))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "bacnet_id__{op} should be accepted"
    );
}

// ═══════════════════════════════════════════════════════
// CIDR FIELD VALIDATION (network on communities)
// ═══════════════════════════════════════════════════════

/// Cidr fields reject comparison and full-text operators.
#[rstest]
#[case::gt("gt")]
#[case::lt("lt")]
#[case::gte("gte")]
#[case::lte("lte")]
#[case::icontains("icontains")]
#[case::endswith("endswith")]
#[case::iequals("iequals")]
#[actix_web::test]
async fn cidr_field_rejects_invalid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/policy/network/communities?network__{op}=10.0.0.0/24"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "network__{op} should be rejected"
    );
}

/// Cidr fields accept equals, contains, startswith, in.
#[rstest]
#[case::equals("equals")]
#[case::contains("contains")]
#[case::startswith("startswith")]
#[case::in_op("in")]
#[case::is_null("is_null")]
#[case::not_equals("not_equals")]
#[actix_web::test]
async fn cidr_field_accepts_valid_operator(#[case] op: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/policy/network/communities?network__{op}=10.0.0.0/24"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "network__{op} should be accepted"
    );
}

// ═══════════════════════════════════════════════════════
// UPDATED_AT FILTERING
// ═══════════════════════════════════════════════════════

#[actix_web::test]
async fn updated_at_gte_matches_recently_created_host() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/hosts")
            .set_json(serde_json::json!({"name": "ts.example.org", "comment": "timestamp test"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let created_ts = body["updated_at"].as_str().unwrap().to_string();

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/hosts?updated_at__gte={created_ts}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert!(body["total"].as_u64().unwrap() >= 1);
}

#[actix_web::test]
async fn updated_at_gt_future_date_matches_nothing() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/hosts")
            .set_json(serde_json::json!({"name": "ts2.example.org", "comment": "timestamp test"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?updated_at__gt=2099-01-01T00:00:00Z")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 0);
}

#[actix_web::test]
async fn updated_at_rejects_string_operators() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?updated_at__contains=2024")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ═══════════════════════════════════════════════════════
// ANCILLARY ENDPOINT OPERATOR TESTS
// ═══════════════════════════════════════════════════════

macro_rules! seed_contacts {
    ($app:expr) => {{
        let resp = test::call_service(
            &$app,
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({"name": "c.example.org", "comment": "contact test"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        for email in ["alice@example.org", "bob@example.org", "charlie@other.org"] {
            let resp = test::call_service(
                &$app,
                test::TestRequest::post()
                    .uri("/inventory/host-contacts")
                    .set_json(serde_json::json!({"email": email, "hosts": ["c.example.org"]}))
                    .to_request(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
    }};
}

#[actix_web::test]
async fn host_contact_email_icontains_matches() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_contacts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/host-contacts?email__icontains=EXAMPLE")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 2); // alice and bob
}

#[actix_web::test]
async fn host_contact_email_endswith_matches() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_contacts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/host-contacts?email__endswith=other.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
}

macro_rules! seed_policies {
    ($app:expr) => {{
        for (name, desc) in [
            ("campus-core", "Main campus policy"),
            ("campus-edge", "Edge network policy"),
            ("datacenter", "DC infrastructure"),
        ] {
            let resp = test::call_service(
                &$app,
                test::TestRequest::post()
                    .uri("/policy/network/policies")
                    .set_json(serde_json::json!({"name": name, "description": desc}))
                    .to_request(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
    }};
}

#[actix_web::test]
async fn network_policy_name_startswith_matches() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_policies!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/policy/network/policies?name__startswith=campus")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 2);
}

#[actix_web::test]
async fn network_policy_description_not_icontains_excludes() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_policies!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/policy/network/policies?description__not_icontains=campus")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 2); // campus-edge ("Edge network policy") and datacenter
}
