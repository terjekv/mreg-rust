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
use serde_json::{Value, json};

fn memory_state() -> AppState {
    let config = Config {
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: true,
        ..Config::default()
    };

    let storage = build_storage(&config).expect("memory storage should initialize");
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

fn redact(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            for key in [
                "id",
                "created_at",
                "updated_at",
                "host_id",
                "ip_address_id",
                "community_id",
                "policy_id",
            ] {
                if map.contains_key(key) {
                    map.insert(key.to_string(), Value::String("<redacted>".to_string()));
                }
            }
            for child in map.values_mut() {
                *child = redact(child.take());
            }
            Value::Object(map.clone())
        }
        Value::Array(items) => Value::Array(items.drain(..).map(redact).collect()),
        other => other.clone(),
    }
}

#[actix_web::test]
async fn host_contact_contract_shape_is_stable() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .wrap(mreg_rust::middleware::Authn)
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/hosts")
            .set_json(json!({
                "name": "app.example.org",
                "comment": "app host"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/host-contacts")
            .set_json(json!({
                "email": "ops@example.org",
                "display_name": "Ops Team",
                "hosts": ["app.example.org"]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/host-contacts")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;

    assert_eq!(
        redact(body),
        json!({
            "items": [
                {
                    "id": "<redacted>",
                    "email": "ops@example.org",
                    "display_name": "Ops Team",
                    "hosts": ["app.example.org"],
                    "created_at": "<redacted>",
                    "updated_at": "<redacted>"
                }
            ],
            "total": 1,
            "next_cursor": null
        })
    );
}

#[actix_web::test]
async fn policy_mapping_contract_shape_is_stable() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    for (uri, body) in [
        (
            "/inventory/networks",
            json!({"cidr":"10.0.0.0/24","description":"LAN","reserved":3}),
        ),
        (
            "/inventory/hosts",
            json!({"name":"app.example.org","comment":"app host"}),
        ),
        (
            "/policy/network/policies",
            json!({"name":"campus-core","description":"Campus core policy"}),
        ),
        (
            "/policy/network/communities",
            json!({
                "policy_name": "campus-core",
                "network": "10.0.0.0/24",
                "name": "prod-network",
                "description": "Production network community"
            }),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/ip-addresses")
            .set_json(json!({
                "host_name": "app.example.org",
                "address": "10.0.0.25"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/policy/network/host-community-assignments")
            .set_json(json!({
                "host_name": "app.example.org",
                "address": "10.0.0.25",
                "policy_name": "campus-core",
                "community_name": "prod-network"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/policy/network/host-community-assignments?host=app.example.org")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;

    assert_eq!(
        redact(body),
        json!({
            "items": [
                {
                    "id": "<redacted>",
                    "host_id": "<redacted>",
                    "host_name": "app.example.org",
                    "ip_address_id": "<redacted>",
                    "address": "10.0.0.25",
                    "community_id": "<redacted>",
                    "community_name": "prod-network",
                    "policy_name": "campus-core",
                    "created_at": "<redacted>",
                    "updated_at": "<redacted>"
                }
            ],
            "total": 1,
            "next_cursor": null
        })
    );
}

#[actix_web::test]
async fn full_dns_lifecycle_with_auto_records() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // 1. Create nameserver
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/nameservers")
            .set_json(json!({"name": "ns1.lifecycle.org"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Create forward zone -> should auto-create NS record
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/forward-zones")
            .set_json(json!({
                "name": "lifecycle.org",
                "primary_ns": "ns1.lifecycle.org",
                "nameservers": ["ns1.lifecycle.org"],
                "email": "admin@lifecycle.org"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let zone_body: Value = test::read_body_json(response).await;
    let initial_serial = zone_body["serial_no"].as_u64().unwrap();

    // 3. Create network and host
    for (uri, body) in [
        (
            "/inventory/networks",
            json!({"cidr":"10.1.0.0/24","description":"Lifecycle LAN"}),
        ),
        (
            "/inventory/hosts",
            json!({"name":"web.lifecycle.org","zone":"lifecycle.org","comment":"web server"}),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // 4. Assign IP -> should auto-create A record
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/ip-addresses")
            .set_json(json!({"host_name":"web.lifecycle.org","address":"10.1.0.10"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // 5. Create another host for CNAME (can't coexist with A record at same owner)
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/hosts")
            .set_json(
                json!({"name":"alias.lifecycle.org","zone":"lifecycle.org","comment":"alias"}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // Create a CNAME record -> should bump zone serial
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(json!({
                "type_name": "CNAME",
                "owner_kind": "host",
                "owner_name": "alias.lifecycle.org",
                "data": {"target": "web.lifecycle.org"}
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let cname_body: Value = test::read_body_json(response).await;
    let cname_id = cname_body["id"].as_str().unwrap().to_string();

    // 6. Verify records: should have NS, A, and CNAME
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/dns/records").to_request(),
    )
    .await;
    let body: Value = test::read_body_json(response).await;
    let records = body["items"].as_array().unwrap();
    let type_names: Vec<&str> = records
        .iter()
        .map(|r| r["type_name"].as_str().unwrap())
        .collect();
    assert!(
        type_names.contains(&"NS"),
        "NS record missing: {type_names:?}"
    );
    assert!(
        type_names.contains(&"A"),
        "A record missing: {type_names:?}"
    );
    assert!(
        type_names.contains(&"CNAME"),
        "CNAME record missing: {type_names:?}"
    );

    // 7. Verify zone serial was bumped
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/dns/forward-zones/lifecycle.org")
            .to_request(),
    )
    .await;
    let zone_body: Value = test::read_body_json(response).await;
    let bumped_serial = zone_body["serial_no"].as_u64().unwrap();
    assert!(bumped_serial > initial_serial, "serial should have bumped");

    // 8. PATCH the host
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/inventory/hosts/web.lifecycle.org")
            .set_json(json!({"comment": "updated comment"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["comment"], "updated comment");

    // 9. Delete the CNAME record -> should bump serial again
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/dns/records/{cname_id}"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // 10. Verify serial bumped again
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/dns/forward-zones/lifecycle.org")
            .to_request(),
    )
    .await;
    let zone_body: Value = test::read_body_json(response).await;
    let final_serial = zone_body["serial_no"].as_u64().unwrap();
    assert!(
        final_serial > bumped_serial,
        "serial should have bumped on delete"
    );

    // 11. Verify history endpoint works
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/system/history").to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[actix_web::test]
async fn delegation_anchored_record_validates_scope() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // Setup: nameserver + zone
    for (uri, body) in [
        ("/dns/nameservers", json!({"name": "ns1.deleg.org"})),
        ("/dns/nameservers", json!({"name": "ns-child.deleg.org"})),
        (
            "/dns/forward-zones",
            json!({
                "name": "deleg.org",
                "primary_ns": "ns1.deleg.org",
                "nameservers": ["ns1.deleg.org"],
                "email": "admin@deleg.org"
            }),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // Create a delegation
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/forward-zones/deleg.org/delegations")
            .set_json(json!({
                "name": "child.deleg.org",
                "comment": "delegated child zone",
                "nameservers": ["ns-child.deleg.org"]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // List delegations
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/dns/forward-zones/deleg.org/delegations")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["name"], "child.deleg.org");
}

#[actix_web::test]
async fn inventory_detail_responses_include_attachment_graph() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    for (uri, body) in [
        (
            "/inventory/networks",
            json!({"cidr": "192.0.2.0/24", "description": "Edge LAN"}),
        ),
        (
            "/inventory/hosts",
            json!({"name": "edge.example.org", "comment": "edge host"}),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/hosts/edge.example.org/attachments")
            .set_json(json!({
                "network": "192.0.2.0/24",
                "mac_address": "aa:bb:cc:dd:ee:ff",
                "comment": "uplink"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let attachment: Value = test::read_body_json(response).await;
    let attachment_id = attachment["id"].as_str().expect("attachment id");

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/inventory/attachments/{attachment_id}/ip-addresses"
            ))
            .set_json(json!({"address": "192.0.2.20"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/inventory/attachments/{attachment_id}/dhcp-identifiers"
            ))
            .set_json(json!({
                "family": 4,
                "kind": "client_id",
                "value": "01:aa:bb:cc:dd:ee:ff",
                "priority": 10
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts/edge.example.org")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["attachments"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["attachments"][0]["network"], "192.0.2.0/24");
    assert_eq!(
        body["attachments"][0]["ip_addresses"][0]["address"],
        "192.0.2.20"
    );
    assert_eq!(
        body["attachments"][0]["dhcp_identifiers"][0]["kind"],
        "client_id"
    );

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/networks/192.0.2.0%2F24")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["hosts"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["hosts"][0]["host_name"], "edge.example.org");
    assert_eq!(
        body["hosts"][0]["attachments"][0]["ip_addresses"][0]["address"],
        "192.0.2.20"
    );
}

#[actix_web::test]
async fn network_detail_reports_full_available_capacity() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/networks")
            .set_json(json!({
                "cidr": "198.51.100.0/24",
                "description": "capacity test",
                "reserved": 3
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/hosts")
            .set_json(json!({
                "name": "capacity.example.org",
                "comment": "capacity test host"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/inventory/ip-addresses")
            .set_json(json!({
                "host_name": "capacity.example.org",
                "address": "198.51.100.20"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/networks/198.51.100.0%2F24")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;

    assert_eq!(body["capacity"]["total_used"], 1);
    assert_eq!(body["capacity"]["total_available"], 251);
}

#[actix_web::test]
async fn rfc3597_raw_record_round_trip() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // Create a custom record type that allows raw RDATA
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/record-types")
            .set_json(json!({
                "name": "TYPE65534",
                "dns_type": 65534,
                "owner_kind": "host",
                "cardinality": "multiple",
                "fields": [],
                "behavior_flags": {
                    "rfc3597": { "allow_raw_rdata": true }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // Create a record with raw RDATA
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(json!({
                "type_name": "TYPE65534",
                "owner_name": "raw.example.org",
                "ttl": 300,
                "raw_rdata": "\\# 6 cafe01020304"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(response).await;

    // Verify raw RDATA is preserved in the POST response (uses RecordResponse)
    assert_eq!(body["raw_rdata"], "\\# 6 cafe01020304");
    assert!(body["data"].is_null());

    // Verify the record can be retrieved by ID (also uses RecordResponse)
    let record_id = body["id"].as_str().unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/dns/records/{record_id}"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["raw_rdata"], "\\# 6 cafe01020304");
    assert_eq!(body["type_name"], "TYPE65534");
}

#[actix_web::test]
async fn host_list_supports_sort_and_filter() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // Create nameserver, zone, and 3 hosts in different zones
    for (uri, body) in [
        ("/dns/nameservers", json!({"name": "ns1.sort.org"})),
        (
            "/dns/forward-zones",
            json!({
                "name": "sort.org",
                "primary_ns": "ns1.sort.org",
                "nameservers": ["ns1.sort.org"],
                "email": "admin@sort.org"
            }),
        ),
        (
            "/inventory/hosts",
            json!({"name": "charlie.sort.org", "zone": "sort.org", "comment": "third"}),
        ),
        (
            "/inventory/hosts",
            json!({"name": "alpha.sort.org", "zone": "sort.org", "comment": "first"}),
        ),
        (
            "/inventory/hosts",
            json!({"name": "bravo.sort.org", "comment": "no zone"}),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // Filter by zone — should return only the 2 hosts in sort.org
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?zone=sort.org")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["total"], 2);
    let names: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"alpha.sort.org"));
    assert!(names.contains(&"charlie.sort.org"));
    assert!(!names.contains(&"bravo.sort.org"));

    // Sort by name descending — all 3 hosts
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?sort_by=name&sort_dir=desc")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    let names: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["charlie.sort.org", "bravo.sort.org", "alpha.sort.org"]
    );

    // Sort + filter + pagination: zone=sort.org, sort_by=name, limit=1
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?zone=sort.org&sort_by=name&limit=1")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["total"], 2);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["name"], "alpha.sort.org");
    let cursor = body["next_cursor"].as_str().expect("should have next page");

    // Follow cursor for page 2
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/inventory/hosts?zone=sort.org&sort_by=name&limit=1&after={cursor}"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["total"], 2);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["name"], "charlie.sort.org");
    assert!(body["next_cursor"].is_null(), "no more pages");

    // Search across all hosts
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/inventory/hosts?search=no%20zone")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "bravo.sort.org");
}

#[actix_web::test]
async fn wildcard_dns_records_work_as_unanchored_records() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // Create a TXT record at *.example.org (no host entity needed)
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(json!({
                "type_name": "TXT",
                "owner_name": "*.example.org",
                "data": {"value": "v=spf1 -all"}
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["owner_name"], "*.example.org");
    assert!(
        body["owner_kind"].is_null(),
        "wildcard should be unanchored"
    );
}

#[actix_web::test]
async fn export_template_list_returns_builtin_templates() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/workflows/export-templates")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;

    let names: Vec<&str> = body["items"]
        .as_array()
        .expect("items should be an array")
        .iter()
        .map(|t| t["name"].as_str().expect("template should have a name"))
        .collect();
    assert!(
        names.contains(&"bind-forward-zone"),
        "bind-forward-zone missing from templates: {names:?}"
    );
    assert!(
        names.contains(&"bind-reverse-zone"),
        "bind-reverse-zone missing from templates: {names:?}"
    );
}

#[actix_web::test]
async fn export_run_lifecycle_and_listing() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // Create a zone so the export has data to render
    for (uri, body) in [
        ("/dns/nameservers", json!({"name": "ns1.export-run.org"})),
        (
            "/dns/forward-zones",
            json!({
                "name": "export-run.org",
                "primary_ns": "ns1.export-run.org",
                "nameservers": ["ns1.export-run.org"],
                "email": "admin@export-run.org"
            }),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // Create an export run for bind-forward-zone (builtin template)
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/workflows/export-runs")
            .set_json(json!({
                "template_name": "bind-forward-zone",
                "scope": "forward_zone",
                "parameters": { "zone_name": "export-run.org" }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // Execute the task
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/workflows/tasks/run-next")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let task_body: Value = test::read_body_json(response).await;
    assert_eq!(
        task_body["workflow_result"]["status"], "succeeded",
        "export run should succeed: {task_body:#}"
    );
    assert!(
        task_body["workflow_result"]["rendered_output"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "rendered_output should be non-empty"
    );

    // List export runs and verify our run appears
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/workflows/export-runs")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    assert!(
        !body["items"].as_array().unwrap().is_empty(),
        "export runs list should contain at least one run"
    );
    let run = &body["items"][0];
    assert_eq!(run["status"], "succeeded");
    assert!(
        run["rendered_output"].as_str().is_some(),
        "rendered_output should be present in listed run"
    );
}

#[actix_web::test]
async fn import_batch_appears_in_listing() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;

    // Create an import batch with a simple network item
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/workflows/imports")
            .set_json(json!({
                "requested_by": "contract-test",
                "items": [
                    {
                        "ref": "net-1",
                        "kind": "network",
                        "operation": "create",
                        "attributes": {
                            "cidr": "172.30.0.0/24",
                            "description": "Import listing test network"
                        }
                    }
                ]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value = test::read_body_json(response).await;
    let import_id = created["id"].as_str().expect("import should have an id");

    // List imports and verify the batch appears
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/workflows/imports")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    let items = body["items"].as_array().expect("items should be an array");
    let found = items.iter().any(|item| item["id"] == import_id);
    assert!(
        found,
        "import batch {} should appear in listing, got {} items",
        import_id,
        items.len()
    );
}
