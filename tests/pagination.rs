//! Pagination tests (cursor walking, limits, empty collection).

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
#[case::all_fit(5, 10, 5, false)]
#[case::exact_fit(5, 5, 5, false)]
#[case::page_smaller(5, 3, 3, true)]
#[case::page_of_one(5, 1, 1, true)]
#[actix_web::test]
async fn pagination_limit_and_cursor_presence(
    #[case] num_items: usize,
    #[case] limit: u64,
    #[case] expected_count: usize,
    #[case] expect_cursor: bool,
) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    for i in 0..num_items {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/labels")
                .set_json(json!({"name": format!("p-{i:03}"), "description": "d"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/inventory/labels?limit={limit}"))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["items"].as_array().unwrap().len(), expected_count);
    assert_eq!(body["total"], num_items as u64);
    assert_eq!(body["next_cursor"].is_string(), expect_cursor);
}

#[actix_web::test]
async fn pagination_walks_all_items_without_duplicates() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    let total = 7usize;
    for i in 0..total {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/labels")
                .set_json(json!({"name": format!("w-{i:03}"), "description": "d"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let mut collected: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;

    loop {
        let uri = match &cursor {
            Some(c) => format!("/inventory/labels?limit=3&after={c}"),
            None => "/inventory/labels?limit=3".to_string(),
        };
        let resp = test::call_service(&app, test::TestRequest::get().uri(&uri).to_request()).await;
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["total"], total as u64);
        collected.extend(item_names(&body));
        pages += 1;
        match body["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }

    assert_eq!(collected.len(), total);
    let unique: std::collections::HashSet<&String> = collected.iter().collect();
    assert_eq!(unique.len(), total, "no duplicates");
    assert_eq!(pages, 3);
}

#[actix_web::test]
async fn pagination_empty_collection() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(mreg_rust::api::v1::configure),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/labels")
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["total"], 0);
    assert!(body["next_cursor"].is_null());
}
