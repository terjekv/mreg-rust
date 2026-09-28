mod common;

use std::collections::BTreeSet;

use actix_web::http::StatusCode;
use mreg_rust::{
    db::{take_query_capture, with_query_capture},
    domain::{
        host::UpdateHost,
        types::{Hostname, IpAddressValue, UpdateField, ip_to_ptr_name},
    },
};
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

#[derive(Clone, Copy, Debug)]
enum DnsZones {
    None,
    Forward,
    Reverse,
    Both,
}

impl DnsZones {
    fn forward(self) -> bool {
        matches!(self, Self::Forward | Self::Both)
    }

    fn reverse(self) -> bool {
        matches!(self, Self::Reverse | Self::Both)
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

impl Fixture {
    fn with_zones(mut self, zones: DnsZones) -> Self {
        self.items
            .retain(|item| match item["kind"].as_str().unwrap() {
                "forward_zone" => zones.forward(),
                "reverse_zone" => zones.reverse(),
                _ => true,
            });
        if !zones.forward() {
            for item in &mut self.items {
                if item["kind"] == "host" {
                    item["attributes"]
                        .as_object_mut()
                        .unwrap()
                        .remove("zone_ref");
                }
            }
        }
        self
    }

    fn expected_records(&self, zones: DnsZones) -> BTreeSet<(String, String, String)> {
        let mut expected = BTreeSet::new();
        if zones.forward() {
            expected.insert((
                self.address_type.into(),
                self.host.clone(),
                json!({"address":self.address}).to_string(),
            ));
            expected.insert((
                "NS".into(),
                self.zone.clone(),
                json!({"nsdname":self.nameserver}).to_string(),
            ));
        }
        if zones.reverse() {
            expected.insert((
                "PTR".into(),
                self.ptr_owner.clone(),
                json!({"ptrdname":self.host}).to_string(),
            ));
            expected.insert((
                "NS".into(),
                self.reverse_zone.clone(),
                json!({"nsdname":self.nameserver}).to_string(),
            ));
        }
        expected
    }
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
    let owners = [
        &fixture.host,
        &fixture.ptr_owner,
        &fixture.zone,
        &fixture.reverse_zone,
    ]
    .map(String::as_str)
    .join(",");
    let result = ctx
        .get_json(&format!("/dns/records?owner_name__in={owners}&limit=1000"))
        .await;
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
async fn imports_generate_only_records_with_matching_zones(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[values(false, true)] ipv6: bool,
    #[values(false, true)] ip_before_zones: bool,
    #[values(DnsZones::None, DnsZones::Forward, DnsZones::Reverse, DnsZones::Both)] zones: DnsZones,
    #[values(false, true)] direct: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut f = fixture(&ctx, ipv6, ip_before_zones).with_zones(zones);
    if direct {
        f.items.retain(|item| item["kind"] != "host_attachment");
        let ip = f
            .items
            .iter_mut()
            .find(|item| item["kind"] == "ip_address")
            .unwrap();
        ip["attributes"] = json!({"host_name_ref":"host", "address":f.address});
    }
    let id = stage(&ctx, &f.items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    assert_eq!(records(&ctx, &f).await, f.expected_records(zones));
}

#[rstest]
#[actix_web::test]
async fn generated_import_records_are_removed_with_the_assignment(
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
    #[values(false, true)] ipv6: bool,
    #[values(false, true)] ip_before_zones: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut f = fixture(&ctx, ipv6, ip_before_zones);
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

#[rstest]
#[actix_web::test]
async fn imported_ptr_overrides_survive_generation_and_backfill(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[values(false, true)] ipv6: bool,
    #[values(false, true)] ip_before_zones: bool,
    #[values(false, true)] suppress: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut f = fixture(&ctx, ipv6, ip_before_zones);
    let target = ctx.host("ptr-target");
    let ip_position = f
        .items
        .iter()
        .position(|item| item["kind"] == "ip_address")
        .unwrap();
    let mut attributes = json!({"host_name_ref":"host", "address_ref":"ip"});
    if !suppress {
        attributes["target_name"] = json!(target);
    }
    f.items.insert(
        ip_position + 1,
        json!({
            "ref":"override", "kind":"ptr_override", "operation":"create", "attributes":attributes
        }),
    );
    let id = stage(&ctx, &f.items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    let ptrs = records(&ctx, &f)
        .await
        .into_iter()
        .filter(|(kind, _, _)| kind == "PTR")
        .collect::<BTreeSet<_>>();
    let expected = if suppress {
        BTreeSet::new()
    } else {
        BTreeSet::from([(
            "PTR".into(),
            f.ptr_owner,
            json!({"ptrdname":target}).to_string(),
        )])
    };
    assert_eq!(ptrs, expected);
}

#[rstest]
#[actix_web::test]
async fn removing_imported_ptr_override_uses_renamed_host(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[values(false, true)] ipv6: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut f = fixture(&ctx, ipv6, false);
    f.items.push(json!({
        "ref":"override", "kind":"ptr_override", "operation":"create",
        "attributes":{"host_name_ref":"host", "address_ref":"ip", "target_name":ctx.host("ptr-target")}
    }));
    let id = stage(&ctx, &f.items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    let name = Hostname::new(ctx.host_in_zone("renamed", &f.zone)).unwrap();
    ctx.storage()
        .hosts()
        .update_host(
            &Hostname::new(&f.host).unwrap(),
            UpdateHost {
                name: Some(name.clone()),
                ttl: UpdateField::Unchanged,
                comment: None,
                zone: UpdateField::Unchanged,
            },
        )
        .await
        .unwrap();
    ctx.storage()
        .ptr_overrides()
        .delete_ptr_override(&IpAddressValue::new(&f.address).unwrap())
        .await
        .unwrap();
    let ptrs = records(&ctx, &f)
        .await
        .into_iter()
        .filter(|(kind, _, _)| kind == "PTR")
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ptrs,
        BTreeSet::from([(
            "PTR".into(),
            f.ptr_owner,
            json!({"ptrdname":name}).to_string()
        )])
    );
}

#[rstest]
#[actix_web::test]
async fn postgres_import_query_budget(
    #[values(DnsZones::None, DnsZones::Forward, DnsZones::Reverse, DnsZones::Both)] zones: DnsZones,
    #[values(2, 32)] host_count: usize,
) {
    let Some(ctx) = TestCtx::postgres().await else {
        return;
    };
    let mut f = fixture(&ctx, false, false).with_zones(zones);
    // Set up zones outside capture: zone backfill intentionally visits existing
    // assignments, whose count depends on other tests sharing this database.
    let first_host = f
        .items
        .iter()
        .position(|item| item["kind"] == "host")
        .unwrap();
    let inventory = f.items.split_off(first_host);
    let setup = stage(&ctx, &f.items).await;
    ctx.storage()
        .imports()
        .run_import_batch(setup)
        .await
        .unwrap();
    f.items = inventory;
    for index in 1..host_count {
        let host_ref = format!("host-{index}");
        let attachment_ref = format!("attachment-{index}");
        f.items.extend([
            json!({"ref":host_ref, "kind":"host", "operation":"create", "attributes":{"name":ctx.host_in_zone(&format!("app-{index}"), &f.zone)}}),
            json!({"ref":attachment_ref, "kind":"host_attachment", "operation":"create", "attributes":{"host_name_ref":host_ref,"network_ref":"network"}}),
            json!({"ref":format!("ip-{index}"), "kind":"ip_address", "operation":"create", "attributes":{"attachment_id_ref":attachment_ref,"address":ctx.ip_in_cidr(&ctx.cidr(0), 20 + index as u8)}}),
        ]);
    }
    for item in &mut f.items {
        let attributes = item["attributes"].as_object_mut().unwrap();
        if attributes.remove("network_ref").is_some() {
            attributes.insert("network".into(), json!(ctx.cidr(0)));
        }
        if attributes.remove("zone_ref").is_some() {
            attributes.insert("zone".into(), json!(f.zone));
        }
    }
    let id = stage(&ctx, &f.items).await;
    let capture_id = format!("{}-import-budget", ctx.namespace());
    with_query_capture(&capture_id, ctx.storage().imports().run_import_batch(id))
        .await
        .unwrap();
    let queries = take_query_capture(&capture_id);
    let count = queries.total_queries();
    // Allow the transaction/status queries and a connection health check in
    // addition to the per-host work; an extra query per host exceeds the budget
    // in the larger case.
    let per_host = match zones {
        DnsZones::None => 16,
        DnsZones::Forward => 64,
        DnsZones::Reverse => 66,
        DnsZones::Both => 114,
    };
    assert!(
        (1..=8 + per_host * host_count).contains(&count),
        "import query budget exceeded: {count}, {:?}",
        queries.query_counts()
    );
}
