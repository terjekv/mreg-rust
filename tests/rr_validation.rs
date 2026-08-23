//! Comprehensive validation tests for all 25 built-in DNS record types.
//!
//! Each record type is tested with valid payloads (should succeed) and
//! invalid payloads (should return validation errors). Uses rstest
//! parameterized cases for thorough coverage.

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

/// Seed a host and zone for record creation tests.
macro_rules! seed_host_and_zone {
    ($app:expr) => {{
        for (uri, body) in [
            ("/dns/nameservers", json!({"name": "ns1.rr.org"})),
            ("/dns/forward-zones", json!({
                "name": "rr.org",
                "primary_ns": "ns1.rr.org",
                "nameservers": ["ns1.rr.org"],
                "email": "admin@rr.org"
            })),
            ("/inventory/hosts", json!({"name": "test.rr.org", "zone": "rr.org", "comment": "rr test"})),
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

// ═══════════════════════════════════════════════════════
// VALID RECORD PAYLOADS — all should succeed (201)
// ═══════════════════════════════════════════════════════

#[rstest]
#[case::a("A", "host", json!({"address": "10.0.0.1"}))]
#[case::aaaa("AAAA", "host", json!({"address": "2001:db8::1"}))]
#[case::cname("CNAME", "host", json!({"target": "other.rr.org"}))]
#[case::mx("MX", "forward_zone", json!({"preference": 10, "exchange": "mail.rr.org"}))]
#[case::txt("TXT", "host", json!({"value": "v=spf1 -all"}))]
#[case::srv("SRV", "unanchored_srv", json!({"priority": 10, "weight": 5, "port": 5060, "target": "sip.rr.org"}))]
#[case::ns("NS", "forward_zone", json!({"nsdname": "ns2.rr.org"}))]
#[case::ptr("PTR", "reverse_zone", json!({"ptrdname": "test.rr.org"}))]
#[case::hinfo("HINFO", "host", json!({"cpu": "x86_64", "os": "Linux"}))]
#[case::loc("LOC", "host", json!({"latitude": 59.9, "longitude": 10.7, "altitude_m": 50.0}))]
#[case::caa("CAA", "host", json!({"flags": 0, "tag": "issue", "value": "letsencrypt.org"}))]
#[case::ds("DS", "forward_zone", json!({"key_tag": 12345, "algorithm": 13, "digest_type": 2, "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}))]
#[case::dnskey("DNSKEY", "forward_zone", json!({"flags": 257, "protocol": 3, "algorithm": 13, "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}))]
#[case::dnskey_ml_dsa_44("DNSKEY", "forward_zone", json!({"flags": 257, "protocol": 3, "algorithm": 18, "public_key": "dGVzdA=="}))]
#[case::tlsa("TLSA", "tlsa", json!({"usage": 3, "selector": 1, "matching_type": 1, "certificate_data": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}))]
#[case::tlsa_c509("TLSA", "tlsa", json!({"usage": 3, "selector": 2, "matching_type": 0, "certificate_data": "abcd"}))]
#[case::sshfp_sha1("SSHFP", "host", json!({"algorithm": 1, "fp_type": 1, "fingerprint": "abcdef0123456789abcdef0123456789abcdef01"}))]
#[case::sshfp_sha256("SSHFP", "host", json!({"algorithm": 2, "fp_type": 2, "fingerprint": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}))]
#[case::naptr("NAPTR", "host", json!({"order": 100, "preference": 10, "flags": "s", "services": "SIP+D2U", "regexp": "", "replacement": "sip.rr.org"}))]
#[case::dname("DNAME", "unanchored", json!({"target": "other.example.org"}))]
#[case::cds("CDS", "forward_zone", json!({"key_tag": 12345, "algorithm": 13, "digest_type": 2, "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}))]
#[case::cds_delete("CDS", "forward_zone", json!({"key_tag": 0, "algorithm": 0, "digest_type": 0, "digest": "00"}))]
#[case::cdnskey("CDNSKEY", "forward_zone", json!({"flags": 257, "protocol": 3, "algorithm": 13, "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="}))]
#[case::cdnskey_delete("CDNSKEY", "forward_zone", json!({"flags": 0, "protocol": 3, "algorithm": 0, "public_key": "AA=="}))]
#[case::csync("CSYNC", "forward_zone", json!({"soa_serial": 2024010100, "flags": 3, "type_bitmap": "A AAAA NS"}))]
#[case::uri("URI", "uri", json!({"priority": 10, "weight": 1, "target": "https://example.org/"}))]
#[case::openpgpkey("OPENPGPKEY", "openpgpkey", json!({"public_key": "dGVzdA=="}))]
#[case::smimea("SMIMEA", "smimea", json!({"usage": 3, "selector": 1, "matching_type": 1, "certificate_data": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}))]
#[case::null_mx("MX", "forward_zone", json!({"preference": 0, "exchange": "."}))]
#[case::svcb("SVCB", "host", json!({"priority": 1, "target": "svc.rr.org"}))]
#[case::https("HTTPS", "host", json!({"priority": 1, "target": "cdn.rr.org"}))]
#[case::svcb_current_registered_keys("SVCB", "host", json!({"priority": 1, "target": "svc.rr.org", "params": [{"key": "dohpath", "value": "/dns-query{?dns}"}, {"key": "ohttp"}, {"key": "pvd"}, {"key": "docpath", "value": ""}]}))]
#[actix_web::test]
async fn valid_record_is_accepted(
    #[case] type_name: &str,
    #[case] owner_kind: &str,
    #[case] data: Value,
) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_host_and_zone!(app);

    // Build the record creation payload based on owner_kind
    let body = match owner_kind {
        "host" => json!({
            "type_name": type_name,
            "owner_kind": "host",
            "owner_name": "test.rr.org",
            "data": data,
        }),
        "forward_zone" => {
            // DNAME is exclusive — can't coexist with NS at zone apex.
            // Use a sub-name for exclusive types.
            let owner = if type_name == "DNAME" {
                "sub.rr.org"
            } else {
                "rr.org"
            };
            json!({
                "type_name": type_name,
                "owner_kind": "forward_zone",
                "owner_name": owner,
                "data": data,
            })
        }
        "reverse_zone" => {
            let resp = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/dns/reverse-zones")
                    .set_json(json!({
                        "name": "2.0.10.in-addr.arpa",
                        "network": "10.0.2.0/24",
                        "primary_ns": "ns1.rr.org",
                        "nameservers": ["ns1.rr.org"],
                        "email": "admin@rr.org"
                    }))
                    .to_request(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
            json!({
                "type_name": type_name,
                "owner_kind": "reverse_zone",
                "owner_name": "2.0.10.in-addr.arpa",
                "data": data,
            })
        }
        "unanchored_srv" => {
            // SRV requires _service._proto owner name prefix
            json!({
                "type_name": type_name,
                "owner_name": "_sip._tcp.rr.org",
                "data": data,
            })
        }
        "unanchored" => {
            // DNAME and similar — unanchored record at a sub-name
            json!({
                "type_name": type_name,
                "owner_name": "sub.rr.org",
                "data": data,
            })
        }
        "tlsa" => json!({
            "type_name": type_name,
            "owner_name": "_443._tcp.test.rr.org",
            "data": data,
        }),
        "uri" => json!({
            "type_name": type_name,
            "owner_name": "_https._tcp.rr.org",
            "data": data,
        }),
        "openpgpkey" => json!({
            "type_name": type_name,
            "owner_name": format!("{}._openpgpkey.rr.org", "a".repeat(56)),
            "data": data,
        }),
        "smimea" => json!({
            "type_name": type_name,
            "owner_name": format!("{}._smimecert.rr.org", "a".repeat(56)),
            "data": data,
        }),
        _ => panic!("unknown owner_kind: {owner_kind}"),
    };

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(body)
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "{type_name} with valid data should be accepted"
    );
}

// ═══════════════════════════════════════════════════════
// INVALID RECORD PAYLOADS — all should fail (400 or 409)
// ═══════════════════════════════════════════════════════

#[rstest]
#[case::a_bad_ip("A", "host", json!({"address": "not-an-ip"}), "invalid")]
#[case::a_ipv6_in_a("A", "host", json!({"address": "2001:db8::1"}), "invalid")]
#[case::aaaa_ipv4_in_aaaa("AAAA", "host", json!({"address": "10.0.0.1"}), "invalid")]
#[case::aaaa_bad_ip("AAAA", "host", json!({"address": "not-an-ip"}), "invalid")]
#[case::mx_missing_exchange("MX", "forward_zone", json!({"preference": 10}), "required")]
#[case::mx_missing_preference("MX", "forward_zone", json!({"exchange": "mail.rr.org"}), "required")]
#[case::mx_null_mx_bad_preference("MX", "forward_zone", json!({"preference": 5, "exchange": "."}), "null MX")]
#[case::sshfp_bad_algorithm("SSHFP", "host", json!({"algorithm": 99, "fp_type": 2, "fingerprint": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}), "algorithm")]
#[case::sshfp_bad_fp_type("SSHFP", "host", json!({"algorithm": 1, "fp_type": 99, "fingerprint": "abcdef0123456789abcdef0123456789abcdef01"}), "fp_type")]
#[case::sshfp_sha1_wrong_length("SSHFP", "host", json!({"algorithm": 1, "fp_type": 1, "fingerprint": "abcdef"}), "hex characters")]
#[case::sshfp_sha256_wrong_length("SSHFP", "host", json!({"algorithm": 2, "fp_type": 2, "fingerprint": "abcdef0123456789abcdef0123456789abcdef01"}), "hex characters")]
#[case::naptr_both_regexp_and_replacement("NAPTR", "host", json!({"order": 100, "preference": 10, "flags": "s", "services": "SIP+D2U", "regexp": "!^.*$!sip:info@rr.org!", "replacement": "sip.rr.org"}), "exactly one")]
#[case::naptr_neither_regexp_nor_replacement("NAPTR", "host", json!({"order": 100, "preference": 10, "flags": "s", "services": "SIP+D2U", "regexp": "", "replacement": "."}), "exactly one")]
#[case::ds_bad_algorithm("DS", "forward_zone", json!({"key_tag": 1, "algorithm": 99, "digest_type": 2, "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}), "algorithm")]
#[case::ds_non_signing_algorithm("DS", "forward_zone", json!({"key_tag": 1, "algorithm": 1, "digest_type": 2, "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}), "zone signing")]
#[case::ds_bad_digest_type("DS", "forward_zone", json!({"key_tag": 1, "algorithm": 13, "digest_type": 99, "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}), "digest_type")]
#[case::dnskey_bad_protocol("DNSKEY", "forward_zone", json!({"flags": 257, "protocol": 1, "algorithm": 13, "public_key": "dGVzdA=="}), "protocol must be 3")]
#[case::dnskey_bad_algorithm("DNSKEY", "forward_zone", json!({"flags": 257, "protocol": 3, "algorithm": 99, "public_key": "dGVzdA=="}), "algorithm")]
#[case::dnskey_empty_key("DNSKEY", "forward_zone", json!({"flags": 257, "protocol": 3, "algorithm": 13, "public_key": ""}), "empty")]
#[case::caa_bad_tag("CAA", "host", json!({"flags": 0, "tag": "bad-tag", "value": "letsencrypt.org"}), "alphanumeric")]
#[case::caa_empty_tag("CAA", "host", json!({"flags": 0, "tag": "", "value": "letsencrypt.org"}), "non-empty")]
#[case::caa_flags_too_large("CAA", "host", json!({"flags": 999, "tag": "issue", "value": "letsencrypt.org"}), "issuer-critical")]
#[case::tlsa_bad_usage("TLSA", "host", json!({"usage": 9, "selector": 1, "matching_type": 1, "certificate_data": "abcd"}), "usage")]
#[case::tlsa_bad_selector("TLSA", "host", json!({"usage": 3, "selector": 9, "matching_type": 1, "certificate_data": "abcd"}), "selector")]
#[case::tlsa_bad_matching_type("TLSA", "host", json!({"usage": 3, "selector": 1, "matching_type": 9, "certificate_data": "abcd"}), "matching_type")]
#[case::smimea_bad_usage("SMIMEA", "host", json!({"usage": 9, "selector": 1, "matching_type": 1, "certificate_data": "abcd"}), "usage")]
#[case::cds_bad_algorithm("CDS", "forward_zone", json!({"key_tag": 1, "algorithm": 99, "digest_type": 2, "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}), "algorithm")]
#[case::cds_malformed_delete("CDS", "forward_zone", json!({"key_tag": 0, "algorithm": 0, "digest_type": 0, "digest": "01"}), "exactly")]
#[case::cdnskey_bad_protocol("CDNSKEY", "forward_zone", json!({"flags": 257, "protocol": 1, "algorithm": 13, "public_key": "dGVzdA=="}), "protocol must be 3")]
#[case::cdnskey_malformed_delete("CDNSKEY", "forward_zone", json!({"flags": 0, "protocol": 3, "algorithm": 0, "public_key": "AQ=="}), "exactly")]
#[case::csync_unknown_flag("CSYNC", "forward_zone", json!({"soa_serial": 1, "flags": 4, "type_bitmap": "NS"}), "flags")]
#[case::csync_bad_type_bitmap("CSYNC", "forward_zone", json!({"soa_serial": 1, "flags": 0, "type_bitmap": "TYPE65535"}), "type_bitmap")]
#[case::uri_empty_target("URI", "host", json!({"priority": 1, "weight": 1, "target": ""}), "empty")]
#[case::openpgpkey_bad_base64("OPENPGPKEY", "host", json!({"public_key": "not base64"}), "base64")]
#[case::loc_bad_latitude("LOC", "host", json!({"latitude": 999.0, "longitude": 10.7, "altitude_m": 50.0}), "latitude")]
#[case::loc_bad_longitude("LOC", "host", json!({"latitude": 59.9, "longitude": 999.0, "altitude_m": 50.0}), "longitude")]
#[case::txt_missing_value("TXT", "host", json!({}), "required")]
#[case::srv_missing_port("SRV", "host", json!({"priority": 10, "weight": 5, "target": "sip.rr.org"}), "port")]
#[case::cname_missing_target("CNAME", "host", json!({}), "required")]
#[case::svcb_invalid_key("SVCB", "host", json!({"priority": 1, "target": "svc.rr.org", "params": [{"key": "key65535", "value": "invalid"}]}), "invalid key")]
#[actix_web::test]
async fn invalid_record_is_rejected(
    #[case] type_name: &str,
    #[case] owner_kind: &str,
    #[case] data: Value,
    #[case] error_contains: &str,
) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_host_and_zone!(app);

    let specialized_owner = match type_name {
        "SRV" => Some("_sip._tcp.rr.org".to_string()),
        "TLSA" => Some("_443._tcp.test.rr.org".to_string()),
        "URI" => Some("_https._tcp.rr.org".to_string()),
        "OPENPGPKEY" => Some(format!("{}._openpgpkey.rr.org", "a".repeat(56))),
        "SMIMEA" => Some(format!("{}._smimecert.rr.org", "a".repeat(56))),
        _ => None,
    };
    let has_specialized_owner = specialized_owner.is_some();
    let mut body = json!({
        "type_name": type_name,
        "owner_name": specialized_owner.unwrap_or_else(|| if owner_kind == "forward_zone" {
            "rr.org".to_string()
        } else {
            "test.rr.org".to_string()
        }),
        "data": data,
    });
    if !has_specialized_owner {
        body["owner_kind"] = Value::String(owner_kind.to_string());
    }

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(body)
            .to_request(),
    )
    .await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;

    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::CONFLICT,
        "{type_name} with invalid data should be rejected (got {status}): {body}"
    );
    let msg = body["message"].as_str().unwrap_or("");
    assert!(
        msg.to_lowercase().contains(&error_contains.to_lowercase()),
        "{type_name}: error message '{msg}' should contain '{error_contains}'"
    );
}

// ═══════════════════════════════════════════════════════
// MISSING REQUIRED FIELDS
// ═══════════════════════════════════════════════════════

#[rstest]
#[case::a_no_data("A", "host", json!({}))]
#[case::aaaa_no_data("AAAA", "host", json!({}))]
#[case::mx_no_data("MX", "forward_zone", json!({}))]
#[case::srv_no_data("SRV", "host", json!({}))]
#[case::ns_no_data("NS", "forward_zone", json!({}))]
#[case::ds_no_data("DS", "forward_zone", json!({}))]
#[case::dnskey_no_data("DNSKEY", "forward_zone", json!({}))]
#[case::sshfp_no_data("SSHFP", "host", json!({}))]
#[case::caa_no_data("CAA", "host", json!({}))]
#[case::tlsa_no_data("TLSA", "host", json!({}))]
#[actix_web::test]
async fn record_with_empty_data_is_rejected(
    #[case] type_name: &str,
    #[case] owner_kind: &str,
    #[case] data: Value,
) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_host_and_zone!(app);

    let owner = if owner_kind == "forward_zone" {
        "rr.org"
    } else {
        "test.rr.org"
    };
    let body = json!({
        "type_name": type_name,
        "owner_kind": owner_kind,
        "owner_name": owner,
        "data": data,
    });

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(body)
            .to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "{type_name} with empty data should be rejected"
    );
}

#[rstest]
#[case::cds(
    "CDS",
    json!({"key_tag": 0, "algorithm": 0, "digest_type": 0, "digest": "00"}),
    json!({"key_tag": 12345, "algorithm": 13, "digest_type": 2, "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"})
)]
#[case::cdnskey(
    "CDNSKEY",
    json!({"flags": 0, "protocol": 3, "algorithm": 0, "public_key": "AA=="}),
    json!({"flags": 257, "protocol": 3, "algorithm": 13, "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="})
)]
#[actix_web::test]
async fn child_ds_delete_signal_must_be_singleton(
    #[case] type_name: &str,
    #[case] delete_data: Value,
    #[case] regular_data: Value,
) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state()))
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    seed_host_and_zone!(app);

    let record = |data| {
        json!({
            "type_name": type_name,
            "owner_kind": "forward_zone",
            "owner_name": "rr.org",
            "data": data,
        })
    };
    test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(record(delete_data))
            .to_request(),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/dns/records")
            .set_json(record(regular_data))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
