mod common;

use std::collections::BTreeSet;

use actix_web::http::StatusCode;
use mreg_rust::domain::types::{IpAddressValue, ip_to_ptr_name};
use rstest::rstest;
use serde_json::{Value, json};
use uuid::Uuid;

use common::{TestBackend, TestCtx};

async fn context(backend: TestBackend) -> Option<TestCtx> {
    match backend {
        TestBackend::Memory => Some(TestCtx::memory()),
        TestBackend::Postgres => TestCtx::postgres().await,
    }
}

struct Fixture {
    items: Vec<Value>,
    host: String,
    zone: String,
    reverse_zone: String,
    address: String,
    ptr_owner: String,
    nameserver: String,
    address_type: &'static str,
}

fn fixture(ctx: &TestCtx, ipv6: bool, ip_before_zones: bool) -> Fixture {
    let zone = ctx.zone("import-dns");
    let host = ctx.host_in_zone("app", &zone);
    let nameserver = ctx.host("ns");
    let v4_cidr = ctx.cidr(0);
    let parts: Vec<_> = v4_cidr.split('.').collect();
    let (cidr, address) = if ipv6 {
        let prefix = format!("2001:db8:{}:{}", parts[1], parts[2]);
        (format!("{prefix}::/64"), format!("{prefix}::20"))
    } else {
        (v4_cidr.clone(), ctx.ip_in_cidr(&v4_cidr, 20))
    };
    let ptr_owner = ip_to_ptr_name(&IpAddressValue::new(&address).unwrap());
    let reverse_zone = ptr_owner
        .split('.')
        .skip(if ipv6 { 16 } else { 1 })
        .collect::<Vec<_>>()
        .join(".");
    let mut items = vec![
        json!({"ref":"network", "kind":"network", "operation":"create", "attributes":{"cidr":cidr,"description":"Import network"}}),
        json!({"ref":"ns", "kind":"nameserver", "operation":"create", "attributes":{"name":nameserver}}),
    ];
    let zones = vec![
        json!({"ref":"zone", "kind":"forward_zone", "operation":"create", "attributes":{
            "name":zone,"primary_ns":nameserver,"nameservers":["ns"],"email":"dns@example.org"
        }}),
        json!({"ref":"reverse", "kind":"reverse_zone", "operation":"create", "attributes":{
            "name":reverse_zone,"primary_ns":nameserver,"nameservers":["ns"],"email":"dns@example.org"
        }}),
    ];
    if !ip_before_zones {
        items.extend(zones.clone());
    }
    let mut host_attributes = json!({"name":host,"ttl":600});
    if !ip_before_zones {
        host_attributes["zone_ref"] = json!("zone");
    }
    items.extend([
        json!({"ref":"host", "kind":"host", "operation":"create", "attributes":host_attributes}),
        json!({"ref":"attachment", "kind":"host_attachment", "operation":"create", "attributes":{
            "host_name_ref":"host","network_ref":"network"
        }}),
        json!({"ref":"ip", "kind":"ip_address", "operation":"create", "attributes":{
            "attachment_id_ref":"attachment","address":address
        }}),
    ]);
    if ip_before_zones {
        items.extend(zones);
    }
    Fixture {
        items,
        host,
        zone,
        reverse_zone,
        address,
        ptr_owner,
        nameserver,
        address_type: if ipv6 { "AAAA" } else { "A" },
    }
}

async fn stage(ctx: &TestCtx, items: &[Value]) -> Uuid {
    let (status, body) = ctx
        .post_json("/workflows/imports", json!({"items":items}))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

async fn records(ctx: &TestCtx, fixture: &Fixture) -> BTreeSet<(String, String, String)> {
    let result = ctx.get_json("/dns/records?limit=1000").await;
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| {
            [
                fixture.host.as_str(),
                &fixture.ptr_owner,
                &fixture.zone,
                &fixture.reverse_zone,
            ]
            .contains(&record["owner_name"].as_str().unwrap())
        })
        .map(|record| {
            (
                record["type_name"].as_str().unwrap().to_string(),
                record["owner_name"].as_str().unwrap().to_string(),
                record["data"].to_string(),
            )
        })
        .collect()
}

#[rstest]
#[actix_web::test]
async fn imports_generate_addresses_ptrs_and_apex_nameservers(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[values(false, true)] ipv6: bool,
    #[values(false, true)] ip_before_zones: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let f = fixture(&ctx, ipv6, ip_before_zones);
    let id = stage(&ctx, &f.items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    assert_eq!(
        records(&ctx, &f).await,
        BTreeSet::from([
            (
                f.address_type.into(),
                f.host.clone(),
                json!({"address":f.address}).to_string()
            ),
            (
                "PTR".into(),
                f.ptr_owner.clone(),
                json!({"ptrdname":f.host}).to_string()
            ),
            (
                "NS".into(),
                f.zone.clone(),
                json!({"nsdname":f.nameserver}).to_string()
            ),
            (
                "NS".into(),
                f.reverse_zone.clone(),
                json!({"nsdname":f.nameserver}).to_string()
            ),
        ])
    );
}

#[rstest]
#[actix_web::test]
async fn generated_import_records_are_removed_with_the_assignment(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let f = fixture(&ctx, false, false);
    let id = stage(&ctx, &f.items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    ctx.storage()
        .hosts()
        .unassign_ip_address(&IpAddressValue::new(&f.address).unwrap())
        .await
        .unwrap();
    assert_eq!(
        records(&ctx, &f).await,
        BTreeSet::from([
            (
                "NS".into(),
                f.zone.clone(),
                json!({"nsdname":f.nameserver}).to_string()
            ),
            (
                "NS".into(),
                f.reverse_zone.clone(),
                json!({"nsdname":f.nameserver}).to_string()
            ),
        ])
    );
}

#[rstest]
#[actix_web::test]
async fn generated_import_records_roll_back_on_late_failure(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut f = fixture(&ctx, false, false);
    f.items.push(
        json!({"ref":"bad","kind":"record","operation":"create","attributes":{
            "type_name":"UNKNOWN","owner_name":f.host,"data":{}
        }}),
    );
    let id = stage(&ctx, &f.items).await;
    ctx.storage()
        .imports()
        .run_import_batch(id)
        .await
        .unwrap_err();
    assert!(records(&ctx, &f).await.is_empty());
}

#[rstest]
#[actix_web::test]
async fn imports_preserve_host_ttl(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let f = fixture(&ctx, false, false);
    let id = stage(&ctx, &f.items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    let host = ctx.get_json(&format!("/inventory/hosts/{}", f.host)).await;
    assert_eq!(host["ttl"], 600);
}
