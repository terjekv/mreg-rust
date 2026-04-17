//! Operator tests (contains, icontains, not_equals, startswith, endswith, in, etc.)

use std::sync::Arc;

use actix_web::{App, http::StatusCode, test, web};
use mreg_rust::{
    AppState, BuildInfo,
    authn::AuthnClient,
    authz::AuthorizerClient,
    config::{Config, StorageBackendSetting},
    events::EventSinkClient,
    storage::build_storage,
};
use serde_json::{Value, json};

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
    let authz = AuthorizerClient::from_config(&config);
    AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        storage,
        authn,
        authz,
        events: EventSinkClient::noop(),
    }
}

fn item_names(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap().to_string())
        .collect()
}

macro_rules! seed_zoned_hosts {
    ($app:expr) => {{
        for (uri, body) in [
            ("/dns/nameservers", json!({"name": "ns1.ftest.org"})),
            ("/dns/forward-zones", json!({
                "name": "ftest.org",
                "primary_ns": "ns1.ftest.org",
                "nameservers": ["ns1.ftest.org"],
                "email": "admin@ftest.org"
            })),
            ("/inventory/hosts", json!({"name": "echo.ftest.org", "zone": "ftest.org", "comment": "echo server"})),
            ("/inventory/hosts", json!({"name": "delta.ftest.org", "zone": "ftest.org", "comment": "delta worker"})),
            ("/inventory/hosts", json!({"name": "charlie.ftest.org", "zone": "ftest.org", "comment": "charlie db"})),
            ("/inventory/hosts", json!({"name": "bravo.ftest.org", "zone": "ftest.org", "comment": "bravo cache"})),
            ("/inventory/hosts", json!({"name": "alpha.ftest.org", "zone": "ftest.org", "comment": "alpha gateway"})),
            ("/inventory/hosts", json!({"name": "standalone.other.org", "comment": "no zone"})),
        ] {
            let resp = test::call_service(
                &$app,
                test::TestRequest::post().uri(uri).set_json(body).to_request(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
    }};
}

#[actix_web::test]
async fn operator_contains_filters_by_substring() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__contains=alpha")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "alpha.ftest.org");
}

#[actix_web::test]
async fn operator_icontains_is_case_insensitive() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__icontains=ALPHA")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "alpha.ftest.org");
}

#[actix_web::test]
async fn operator_not_equals_excludes_match() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    for name in ["keep.example.org", "drop.example.org"] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({"name": name, "comment": "test"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__not_equals=drop.example.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "keep.example.org");
}

#[actix_web::test]
async fn operator_startswith_matches_prefix() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__startswith=echo")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "echo.ftest.org");
}

#[actix_web::test]
async fn operator_endswith_matches_suffix() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__endswith=other.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "standalone.other.org");
}

#[actix_web::test]
async fn operator_in_matches_set() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__in=alpha.ftest.org,echo.ftest.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 2);
    let names = item_names(&body);
    assert!(names.contains(&"alpha.ftest.org".to_string()));
    assert!(names.contains(&"echo.ftest.org".to_string()));
}

#[actix_web::test]
async fn operator_multiple_conditions_on_same_field() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    // name must contain "ftest" AND NOT contain "alpha"
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__contains=ftest&name__not_contains=alpha")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 4); // bravo, charlie, delta, echo (not alpha, not standalone)
    assert!(!item_names(&body).contains(&"alpha.ftest.org".to_string()));
}

#[actix_web::test]
async fn operator_comment_icontains() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?comment__icontains=GATEWAY")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "alpha.ftest.org");
}

#[actix_web::test]
async fn operator_unknown_field_returns_400() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?nonexistent__equals=foo")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn operator_invalid_operator_returns_400() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__badop=foo")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn operator_backwards_compatible_bare_equals() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    // Old syntax: ?name=value (no operator) should still work as equals
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name=delta.ftest.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "delta.ftest.org");
}

#[actix_web::test]
async fn operator_with_sort_and_pagination() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    // Filter + sort + paginate: contains "ftest", sort desc, limit 2
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__contains=ftest&sort_by=name&sort_dir=desc&limit=2")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 5);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    let names = item_names(&body);
    assert_eq!(names[0], "echo.ftest.org");
    assert_eq!(names[1], "delta.ftest.org");
    assert!(body["next_cursor"].is_string());
}

// ═══════════════════════════════════════════════════════
// ADDITIONAL OPERATORS: is_null, negated comparisons
// ═══════════════════════════════════════════════════════

#[actix_web::test]
async fn operator_is_null_matches_missing_zone() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    // standalone.other.org has no zone
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?zone__is_null=true")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "standalone.other.org");
}

#[actix_web::test]
async fn operator_not_is_null_matches_hosts_with_zone() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?zone__not_is_null=true")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 5); // all hosts with zones
}

#[actix_web::test]
async fn operator_iequals_exact_case_insensitive() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__iequals=DELTA.FTEST.ORG")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "delta.ftest.org");
}

#[actix_web::test]
async fn operator_not_in_excludes_set() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    // Exclude alpha and echo, expect bravo, charlie, delta, standalone
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__not_in=alpha.ftest.org,echo.ftest.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 4);
    let names = item_names(&body);
    assert!(!names.contains(&"alpha.ftest.org".to_string()));
    assert!(!names.contains(&"echo.ftest.org".to_string()));
}

#[actix_web::test]
async fn operator_iendswith_case_insensitive_suffix() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__iendswith=.FTEST.ORG")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 5); // all ftest.org hosts
}

#[actix_web::test]
async fn operator_not_startswith_excludes_prefix() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name__not_startswith=standalone")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 5); // everything except standalone
}

// ═══════════════════════════════════════════════════════
// CROSS-ENTITY: network family, record owner_kind
// ═══════════════════════════════════════════════════════

macro_rules! seed_family_networks {
    ($app:expr) => {{
        for (cidr, desc) in [("10.0.0.0/24", "v4 net"), ("fd00::/64", "v6 net")] {
            let resp = test::call_service(
                &$app,
                test::TestRequest::post()
                    .uri("/inventory/networks")
                    .set_json(serde_json::json!({"cidr": cidr, "description": desc}))
                    .to_request(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
    }};
}

use rstest::rstest;

#[rstest]
#[case::family_equals("family=4", 1)]
#[case::family_not_equals("family__not_equals=4", 1)]
#[actix_web::test]
async fn filter_network_by_family(#[case] query: &str, #[case] expected: u64) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_family_networks!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/networks?{query}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], expected);
}

macro_rules! seed_networks {
    ($app:expr) => {{
        for (cidr, desc) in [
            ("10.0.0.0/24", "Production LAN"),
            ("10.1.0.0/24", "Development VLAN"),
            ("10.2.0.0/24", "Staging Environment"),
            ("fd00::/64", "IPv6 net"),
        ] {
            let resp = test::call_service(
                &$app,
                test::TestRequest::post()
                    .uri("/inventory/networks")
                    .set_json(serde_json::json!({"cidr": cidr, "description": desc}))
                    .to_request(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
    }};
}

#[actix_web::test]
async fn network_description_istartswith_matches_prefix() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_networks!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/networks?description__istartswith=prod")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
}

#[actix_web::test]
async fn network_description_not_icontains_excludes() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_networks!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/networks?description__not_icontains=lan")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 2); // Staging + IPv6
}

macro_rules! seed_records_with_owner_kinds {
    ($app:expr) => {{
        for (uri, body) in [
            ("/dns/nameservers", serde_json::json!({"name": "ns1.rk.org"})),
            ("/dns/forward-zones", serde_json::json!({
                "name": "rk.org", "primary_ns": "ns1.rk.org",
                "nameservers": ["ns1.rk.org"], "email": "admin@rk.org"
            })),
            ("/inventory/hosts", serde_json::json!({"name": "web.rk.org", "zone": "rk.org", "comment": "web"})),
        ] {
            let resp = test::call_service(
                &$app, test::TestRequest::post().uri(uri).set_json(body).to_request(),
            ).await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }

        let resp = test::call_service(
            &$app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "TXT", "owner_kind": "host", "owner_name": "web.rk.org",
                    "data": {"value": "test"}
                }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }};
}

#[actix_web::test]
async fn filter_records_by_owner_kind_enum() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;
    seed_records_with_owner_kinds!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/dns/records?owner_kind=forward_zone")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert!(body["total"].as_u64().unwrap() >= 1);
    for item in body["items"].as_array().unwrap() {
        assert_eq!(item["owner_kind"], "forward_zone");
    }
}

#[actix_web::test]
async fn owner_kind_enum_rejects_contains_operator() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/dns/records?owner_kind__contains=zone")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
