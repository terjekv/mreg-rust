mod common;

use actix_web::{App, http::StatusCode, test, web};
use common::memory_state;
use rstest::rstest;
use serde_json::{Value, json};

#[rstest]
#[case(json!({"serialno":1}), StatusCode::NOT_IMPLEMENTED)]
#[case(json!({"serialno":null}), StatusCode::NOT_IMPLEMENTED)]
#[case(json!({"serialno":4294967296_u64}), StatusCode::BAD_REQUEST)]
#[actix_web::test]
async fn serial_writes_cannot_change_zone(#[case] payload: Value, #[case] expected: StatusCode) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::configure(cfg, false)),
    )
    .await;
    for (uri, body) in [
        ("/api/v2/dns/nameservers", json!({"name":"ns.example.org"})),
        (
            "/api/v2/dns/forward-zones",
            json!({"name":"example.org","primary_ns":"ns.example.org","email":"hostmaster@example.org"}),
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
    let before: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v2/dns/forward-zones/example.org")
            .to_request(),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/api/v1/zones/forward/example.org")
            .set_json(payload)
            .to_request(),
    )
    .await;
    let status = response.status();
    let after: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v2/dns/forward-zones/example.org")
            .to_request(),
    )
    .await;
    assert_eq!((status, after), (expected, before));
}

#[rstest]
#[case(json!({"comment":"Changed"}), StatusCode::NO_CONTENT, "Changed")]
#[case(json!({}), StatusCode::NO_CONTENT, "Original")]
#[case(json!({"nameservers":["next.example.org"]}), StatusCode::NO_CONTENT, "Original")]
#[case(json!({"nameservers":[]}), StatusCode::BAD_REQUEST, "Original")]
#[case(json!({"nameservers":null}), StatusCode::BAD_REQUEST, "Original")]
#[case(json!({"comment":null}), StatusCode::BAD_REQUEST, "Original")]
#[actix_web::test]
async fn delegation_patch_preserves_records_and_absent_fields(
    #[case] payload: Value,
    #[case] expected: StatusCode,
    #[case] comment: &str,
) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::configure(cfg, false)),
    )
    .await;
    for (uri, body) in [
        ("/api/v2/dns/nameservers", json!({"name":"ns.example.org"})),
        (
            "/api/v2/dns/nameservers",
            json!({"name":"next.example.org"}),
        ),
        (
            "/api/v2/dns/forward-zones",
            json!({"name":"example.org","primary_ns":"ns.example.org","email":"hostmaster@example.org"}),
        ),
        (
            "/api/v2/dns/forward-zones/example.org/delegations",
            json!({"name":"child.example.org","comment":"Original","nameservers":["ns.example.org"]}),
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
    let ds: Value = test::call_and_read_body_json(&app, test::TestRequest::post().uri("/api/v2/dns/records").set_json(json!({
        "type_name":"DS","owner_kind":"forward_zone_delegation","owner_name":"child.example.org",
        "data":{"key_tag":12345,"algorithm":13,"digest_type":2,"digest":"ab".repeat(32)}
    })).to_request()).await;
    let ds_uri = format!(
        "/api/v2/dns/records/{}",
        ds["id"].as_str().expect("created DS record")
    );
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/api/v1/zones/forward/example.org/delegations/child.example.org")
            .set_json(payload)
            .to_request(),
    )
    .await;
    let status = response.status();
    let after: Value =
        test::call_and_read_body_json(&app, test::TestRequest::get().uri(&ds_uri).to_request())
            .await;
    let delegations: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v2/dns/forward-zones/example.org/delegations")
            .to_request(),
    )
    .await;
    assert_eq!(
        (status, after, delegations["items"][0]["comment"].clone()),
        (expected, ds, json!(comment))
    );
}

#[actix_web::test]
async fn community_replacement_preserves_existing_membership() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(memory_state()))
            .configure(|cfg| mreg_rust::api::configure(cfg, false)),
    )
    .await;
    for (uri, body) in [
        (
            "/api/v2/policy/network/policies",
            json!({"name":"campus","description":"Campus"}),
        ),
        (
            "/api/v2/inventory/networks",
            json!({"cidr":"10.0.0.0/24","description":"LAN","policy_name":"campus"}),
        ),
        (
            "/api/v2/inventory/hosts",
            json!({"name":"host.example.org"}),
        ),
        (
            "/api/v2/inventory/ip-addresses",
            json!({"host_name":"host.example.org","address":"10.0.0.10","mac_address":"02:00:00:00:00:01"}),
        ),
        (
            "/api/v2/policy/network/communities",
            json!({"policy_name":"campus","network":"10.0.0.0/24","name":"first","description":"First"}),
        ),
        (
            "/api/v2/policy/network/communities",
            json!({"policy_name":"campus","network":"10.0.0.0/24","name":"second","description":"Second"}),
        ),
        (
            "/api/v2/policy/network/host-community-assignments",
            json!({"host_name":"host.example.org","address":"10.0.0.10","policy_name":"campus","community_name":"first"}),
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
    let communities: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/networks/10.0.0.0/24/communities/")
            .to_request(),
    )
    .await;
    let second = communities["results"]
        .as_array()
        .expect("communities")
        .iter()
        .find(|item| item["name"] == "second")
        .expect("second community")["id"]
        .as_u64()
        .unwrap();
    let mapping_uri = "/api/v2/policy/network/host-community-assignments";
    let before: Value =
        test::call_and_read_body_json(&app, test::TestRequest::get().uri(mapping_uri).to_request())
            .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/networks/10.0.0.0/24/communities/{second}/hosts/"
            ))
            .set_json(json!({"ipaddress":"10.0.0.10"}))
            .to_request(),
    )
    .await;
    let status = response.status();
    let after: Value =
        test::call_and_read_body_json(&app, test::TestRequest::get().uri(mapping_uri).to_request())
            .await;
    assert_eq!((status, after), (StatusCode::NOT_IMPLEMENTED, before));
}
