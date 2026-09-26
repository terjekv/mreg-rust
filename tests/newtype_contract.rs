//! Constructor, transport, and backend contracts for validated domain values.
mod common;

use actix_web::http::StatusCode;
use mreg_rust::{
    api::ApiDoc,
    domain::{
        imports::{ImportBatch, ImportItem},
        pagination::{PageLimit, PageRequest},
        resource_records::{RawRdataValue, RecordFieldSchema, RecordTypeSchema},
        types::{HostPolicyName, Hostname, LabelName, Ttl, UpdateField},
    },
};
use rstest::rstest;
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::OpenApi;

#[rstest]
#[case(1, 1)]
#[case(1000, 1000)]
#[case(1001, 1000)]
#[case(u64::MAX, 1000)]
fn public_page_sizes_are_bounded(#[case] input: u64, #[case] expected: u64) {
    assert_eq!(PageLimit::new(input).unwrap().as_u64(), expected);
}

#[test]
fn zero_page_size_cannot_be_constructed() {
    assert!(PageLimit::new(0).is_err());
}

#[rstest]
#[case("limit=18446744073709551615", 1000)]
#[case("limit=18446744073709551615&fetch_all=true", 1000)]
#[case("fetch_all=true", 100)]
fn public_queries_cannot_enable_internal_fetch_all(#[case] query: &str, #[case] expected: u64) {
    let page: PageRequest = serde_urlencoded::from_str(query).unwrap();
    assert_eq!(page.limit(), expected);
}

#[test]
fn internal_fetch_all_is_explicit() {
    assert_eq!(PageRequest::all().limit(), u64::MAX);
}

#[test]
fn empty_import_batches_cannot_be_deserialized() {
    assert!(serde_json::from_value::<ImportBatch>(json!({"items": []})).is_err());
}

#[rstest]
#[case("")]
#[case(" \t ")]
fn import_references_cannot_bypass_validation(#[case] reference: &str) {
    let result = serde_json::from_value::<ImportItem>(json!({
        "ref": reference, "kind": "label", "operation": "create"
    }));
    assert!(result.is_err());
}

#[test]
fn import_deserialization_applies_constructor_normalization() {
    let item: ImportItem = serde_json::from_value(json!({
        "ref": " label-one ", "kind": "label", "operation": "create"
    }))
    .unwrap();
    assert_eq!(item.reference(), "label-one");
}

#[rstest]
#[case(0, true)]
#[case(65535, true)]
#[case(65536, false)]
fn raw_rdata_deserialization_enforces_wire_size(#[case] length: usize, #[case] valid: bool) {
    let result = serde_json::from_value::<RawRdataValue>(json!({"wire_bytes": vec![0; length]}));
    assert_eq!(result.is_ok(), valid);
}

#[rstest]
#[case("", "text", json!([]))]
#[case("9field", "text", json!([]))]
#[case("field", "enum", json!([]))]
#[case("field", "enum", json!(["one", "one"]))]
#[case("field", "enum", json!([""]))]
#[case("field", "text", json!(["one"]))]
fn record_fields_cannot_bypass_constructor_validation(
    #[case] name: &str,
    #[case] kind: &str,
    #[case] options: Value,
) {
    assert!(
        serde_json::from_value::<RecordFieldSchema>(json!({
            "name": name, "kind": kind, "required": false, "repeated": false, "options": options
        }))
        .is_err()
    );
}

fn field() -> Value {
    json!({"name": "value", "kind": "text", "required": true, "repeated": false, "options": []})
}

#[rstest]
#[case(json!([]), None)]
#[case(json!([field(), field()]), None)]
#[case(json!([field()]), Some("{{"))]
fn record_schemas_cannot_bypass_constructor_validation(
    #[case] fields: Value,
    #[case] template: Option<&str>,
) {
    assert!(
        serde_json::from_value::<RecordTypeSchema>(json!({
            "owner_kind": "host", "cardinality": "multiple", "zone_bound": true,
            "fields": fields, "behavior_flags": {}, "render_template": template
        }))
        .is_err()
    );
}

#[test]
fn valid_record_schema_round_trips() {
    let value = json!({
        "owner_kind": "host", "cardinality": "multiple", "zone_bound": true,
        "fields": [field()], "behavior_flags": {}, "render_template": "{{ value }}"
    });
    let schema: RecordTypeSchema = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(schema).unwrap(), value);
}

#[derive(Deserialize)]
struct Patch {
    #[serde(default)]
    ttl: UpdateField<Ttl>,
}

#[rstest]
#[case(json!({}), UpdateField::Unchanged)]
#[case(json!({"ttl": null}), UpdateField::Clear)]
#[case(json!({"ttl": 0}), UpdateField::Set(Ttl::new(0).unwrap()))]
fn typed_patch_fields_preserve_absent_null_and_set(
    #[case] value: Value,
    #[case] expected: UpdateField<Ttl>,
) {
    assert_eq!(
        serde_json::from_value::<Patch>(value).unwrap().ttl,
        expected
    );
}

#[test]
fn typed_patch_fields_reject_invalid_values() {
    assert!(serde_json::from_value::<Patch>(json!({"ttl": 2147483648_u64})).is_err());
}

#[rstest]
#[case("/inventory/hosts", json!({"name": "bad_name.test"}))]
#[case("/inventory/hosts", json!({"name": "host.test", "ip_addresses": [{"address": "bad"}]}))]
#[case("/inventory/hosts", json!({"name": "host.test", "ttl": 2147483648_u64}))]
#[case("/inventory/ip-addresses", json!({"host_name": "host.test", "allocation": "unknown"}))]
#[case("/inventory/labels", json!({"name": "bad label", "description": "test"}))]
#[case("/inventory/networks", json!({"cidr": "bad", "description": "test"}))]
#[case("/inventory/networks", json!({"cidr": "10.0.0.0/24", "vlan": 4095, "description": "test"}))]
#[case("/inventory/host-groups", json!({"name": "valid", "hosts": ["bad_name.test"], "description": "test"}))]
#[case("/inventory/host-contacts", json!({"email": "invalid"}))]
#[case("/inventory/bacnet-ids", json!({"bacnet_id": 4194303, "host_name": "host.test"}))]
#[case("/policy/network/policies", json!({"name": "bad policy", "description": "test"}))]
#[case("/policy/network/communities", json!({"name": "valid", "policy_name": "bad policy", "network": "10.0.0.0/24", "description": "test"}))]
#[case("/policy/host/atoms", json!({"name": "bad atom", "description": "test"}))]
#[case("/policy/host/roles", json!({"name": "bad role", "description": "test"}))]
#[case("/dns/nameservers", json!({"name": "bad..test"}))]
#[case("/dns/forward-zones", json!({"name": "test", "primary_ns": "ns.test", "email": "invalid"}))]
#[case("/dns/reverse-zones", json!({"name": "0.10.in-addr.arpa", "primary_ns": "ns.test", "email": "ops@test.org", "refresh": 2147483648_u64}))]
#[case("/dns/records", json!({"type_name": "A", "owner_name": "host.test", "ttl": 2147483648_u64, "data": {"address": "10.0.0.1"}}))]
#[actix_web::test]
async fn invalid_json_values_return_structured_boundary_errors(
    #[case] path: &str,
    #[case] body: Value,
) {
    let ctx = common::TestCtx::memory();
    let (status, body) = ctx.post_json(path, body).await;
    assert_eq!(
        (
            status,
            body["error"].as_str(),
            body["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        ),
        (StatusCode::BAD_REQUEST, Some("validation_error"), true)
    );
}

#[rstest]
#[case("/inventory/hosts/missing.test", json!({"name": "bad_name.test"}))]
#[case("/inventory/hosts/missing.test", json!({"ttl": 2147483648_u64}))]
#[case("/inventory/hosts/missing.test", json!({"zone": "bad..test"}))]
#[case("/inventory/networks/10.0.0.0%2F24", json!({"vlan": 4095}))]
#[actix_web::test]
async fn invalid_patch_values_fail_before_resource_lookup(#[case] path: &str, #[case] body: Value) {
    let ctx = common::TestCtx::memory();
    assert_eq!(ctx.patch_json(path, body).await.0, StatusCode::BAD_REQUEST);
}

#[rstest]
#[case("/inventory/hosts/bad_name.test")]
#[case("/inventory/labels/bad%20label")]
#[case("/dns/forward-zones/bad..test")]
#[case("/inventory/hosts?limit=0")]
#[case("/inventory/labels?limit=0")]
#[actix_web::test]
async fn invalid_paths_and_queries_return_validation_errors(#[case] path: &str) {
    let ctx = common::TestCtx::memory();
    let (status, body) = ctx.get_json_as(path, "test-user", &[]).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation_error"))
    );
}

#[rstest]
#[case("CreateHostRequest", "name", "string")]
#[case("CreateForwardZoneRequest", "refresh", "integer")]
#[case("CreateNetworkRequest", "cidr", "string")]
#[case("CreateLabelRequest", "name", "string")]
fn newtypes_keep_primitive_openapi_shapes(
    #[case] schema: &str,
    #[case] field: &str,
    #[case] expected: &str,
) {
    let document = serde_json::to_value(ApiDoc::openapi()).unwrap();
    assert_eq!(
        document["components"]["schemas"][schema]["properties"][field]["type"],
        expected
    );
}

dual_backend_test!(policy_membership_normalizes_names, |ctx| {
    let host = Hostname::new(ctx.host("typed-role")).unwrap();
    ctx.seed_host(host.as_str()).await;
    let label = LabelName::new(ctx.name("typed-label")).unwrap();
    ctx.post(
        "/inventory/labels",
        json!({"name": label, "description": "test"}),
    )
    .await;
    let role = HostPolicyName::new(ctx.name("typed-role")).unwrap();
    ctx.post(
        "/policy/host/roles",
        json!({"name": role, "description": "test"}),
    )
    .await;
    ctx.post(
        &format!(
            "/policy/host/roles/{}/hosts/{}",
            role.as_str().to_uppercase(),
            host.as_str().to_uppercase()
        ),
        json!({}),
    )
    .await;
    ctx.post(
        &format!(
            "/policy/host/roles/{}/labels/{}",
            role.as_str().to_uppercase(),
            label.as_str().to_uppercase()
        ),
        json!({}),
    )
    .await;
    let stored = ctx
        .storage()
        .host_policy()
        .get_role_by_name(&role)
        .await
        .unwrap();
    assert_eq!(
        (stored.hosts(), stored.labels()),
        ([host].as_slice(), [label].as_slice())
    );
});
