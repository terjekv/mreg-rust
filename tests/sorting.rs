//! Sort tests (asc/desc, default sort, per-entity).

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

#[rstest]
#[case::asc("asc", vec!["alpha", "bravo", "charlie", "delta"])]
#[case::desc("desc", vec!["delta", "charlie", "bravo", "alpha"])]
#[actix_web::test]
async fn sort_labels_by_name(#[case] dir: &str, #[case] expected: Vec<&str>) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    for name in ["delta", "alpha", "charlie", "bravo"] {
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
            .uri(&format!("/inventory/labels?sort_by=name&sort_dir={dir}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(item_names(&body), expected);
}

#[rstest]
#[case::asc("asc", vec!["alpha.s.org", "mike.s.org", "zulu.s.org"])]
#[case::desc("desc", vec!["zulu.s.org", "mike.s.org", "alpha.s.org"])]
#[actix_web::test]
async fn sort_hosts_by_name(#[case] dir: &str, #[case] expected: Vec<&str>) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    for name in ["zulu.s.org", "alpha.s.org", "mike.s.org"] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(json!({"name": name, "comment": "t"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/hosts?sort_by=name&sort_dir={dir}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(item_names(&body), expected);
}

#[actix_web::test]
async fn sort_default_is_ascending_by_name() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    for name in ["charlie", "alpha", "bravo"] {
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
            .uri("/inventory/labels")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(item_names(&body), vec!["alpha", "bravo", "charlie"]);
}
