//! Basic filter tests (zone, name, search, networks, records) and combined filter+sort+pagination.

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
async fn filter_hosts_by_zone() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?zone=ftest.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 5);
    assert!(!item_names(&body).contains(&"standalone.other.org".to_string()));
}

#[actix_web::test]
async fn filter_hosts_by_exact_name() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_zoned_hosts!(app);

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

#[rstest]
#[case::by_comment("cache", "bravo.ftest.org")]
#[case::by_name("alpha", "alpha.ftest.org")]
#[case::by_partial("stand", "standalone.other.org")]
#[actix_web::test]
async fn filter_hosts_by_search(#[case] query: &str, #[case] expected: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_zoned_hosts!(app);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/hosts?search={query}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], expected);
}

#[actix_web::test]
async fn filter_returns_empty_when_no_match() {
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
            .set_json(json!({"name": "only.x.org", "comment": "c"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?name=missing.x.org")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 0);
}

#[rstest]
#[case::ipv4(4)]
#[case::ipv6(6)]
#[actix_web::test]
async fn filter_networks_by_family(#[case] family: u8) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    for (cidr, desc) in [("10.0.0.0/24", "v4"), ("fd00::/64", "v6")] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/networks")
                .set_json(json!({"cidr": cidr, "description": desc}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/networks?family={family}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 1);
}

#[rstest]
#[case::by_description("production", 1)]
#[case::by_cidr("172.16", 1)]
#[case::no_match("nonexistent", 0)]
#[actix_web::test]
async fn filter_networks_by_search(#[case] query: &str, #[case] expected: u64) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    for (cidr, desc) in [
        ("10.0.0.0/24", "Production LAN"),
        ("172.16.0.0/16", "Dev VLAN"),
    ] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/networks")
                .set_json(json!({"cidr": cidr, "description": desc}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/networks?search={query}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], expected);
}

// ═══════════════════════════════════════════════════════
// COMBINED: filter + sort + pagination
// ═══════════════════════════════════════════════════════

#[actix_web::test]
async fn combined_filter_sort_paginate_walks_filtered_set() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_zoned_hosts!(app);

    let mut all: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let uri = match &cursor {
            Some(c) => {
                format!(
                    "/inventory/hosts?zone=ftest.org&sort_by=name&sort_dir=desc&limit=2&after={c}"
                )
            }
            None => {
                "/inventory/hosts?zone=ftest.org&sort_by=name&sort_dir=desc&limit=2".to_string()
            }
        };
        let resp = test::call_service(&app, test::TestRequest::get().uri(&uri).to_request()).await;
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["total"], 5);
        all.extend(item_names(&body));
        match body["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }

    assert_eq!(
        all,
        vec![
            "echo.ftest.org",
            "delta.ftest.org",
            "charlie.ftest.org",
            "bravo.ftest.org",
            "alpha.ftest.org",
        ]
    );
}

#[actix_web::test]
async fn combined_sort_paginate_labels() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    for name in ["prod", "dev", "staging", "test"] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/labels")
                .set_json(json!({"name": name, "description": "d"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/labels?sort_by=name&sort_dir=desc&limit=2")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(item_names(&body), vec!["test", "staging"]);
    let cursor = body["next_cursor"].as_str().unwrap();

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/inventory/labels?sort_by=name&sort_dir=desc&limit=2&after={cursor}"
            ))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(item_names(&body), vec!["prod", "dev"]);
    assert!(body["next_cursor"].is_null());
}

#[actix_web::test]
async fn combined_filter_paginate_networks() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    for i in 0..4u8 {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/networks")
                .set_json(
                    json!({"cidr": format!("10.{i}.0.0/24"), "description": format!("net-{i}")}),
                )
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/networks")
            .set_json(json!({"cidr": "fd00::/64", "description": "ipv6"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/networks?family=4&limit=2")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 4);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert!(body["next_cursor"].is_string());
}

// ═══════════════════════════════════════════════════════
// RESPONSE SHAPE
// ═══════════════════════════════════════════════════════

#[rstest]
#[case::labels("/inventory/labels")]
#[case::nameservers("/dns/nameservers")]
#[case::forward_zones("/dns/forward-zones")]
#[case::reverse_zones("/dns/reverse-zones")]
#[case::hosts("/inventory/hosts")]
#[case::networks("/inventory/networks")]
#[case::records("/dns/records")]
#[actix_web::test]
async fn list_endpoint_returns_page_response_shape(#[case] endpoint: &str) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let resp = test::call_service(&app, test::TestRequest::get().uri(endpoint).to_request()).await;
    let body: Value = test::read_body_json(resp).await;
    assert!(body["items"].is_array(), "{endpoint}: missing items");
    assert!(body["total"].is_number(), "{endpoint}: missing total");
    assert!(
        body.get("next_cursor").is_some(),
        "{endpoint}: missing next_cursor"
    );
}
