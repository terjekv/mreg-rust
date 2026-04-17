use actix_web::http::StatusCode;
use serde_json::json;

use crate::common::TestCtx;

pub(crate) const ADMIN_USER: &str = "alice";
pub(crate) const ADMIN_GROUP: &str = "mreg-admins";
pub(crate) const READONLY_USER: &str = "bob";
pub(crate) const READONLY_GROUP: &str = "mreg-readonly";
pub(crate) const LEGACY_ALLOWED_HOST_NETWORK: &str = "10.250.1.0/24";
pub(crate) const LEGACY_OTHER_HOST_NETWORK: &str = "10.250.2.0/24";

#[derive(Debug, Clone)]
pub(crate) struct SeededData {
    pub(crate) label: String,
    pub(crate) nameserver: String,
    pub(crate) zone: String,
    pub(crate) host: String,
    pub(crate) network: String,
    pub(crate) record_id: String,
    pub(crate) rrset_id: String,
    pub(crate) atom: String,
    pub(crate) role: String,
}

pub(crate) fn treetop_ctx() -> Option<TestCtx> {
    TestCtx::treetop_memory()
}

pub(crate) fn one_group(group: &str) -> Vec<&str> {
    if group.is_empty() {
        Vec::new()
    } else {
        vec![group]
    }
}

pub(crate) async fn seed_core_data(ctx: &TestCtx) -> SeededData {
    let label = ctx.name("ops");
    assert_eq!(
        ctx.post_as(
            "/inventory/labels",
            json!({
                "name": label,
                "description": "operators",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let nameserver = ctx.nameserver("ns1", "seed.test");
    assert_eq!(
        ctx.post_as(
            "/dns/nameservers",
            json!({
                "name": nameserver,
                "ttl": 3600,
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let zone = ctx.zone("authz");
    assert_eq!(
        ctx.post_as(
            "/dns/forward-zones",
            json!({
                "name": zone,
                "primary_ns": nameserver,
                "nameservers": [nameserver],
                "email": format!("hostmaster@{zone}"),
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let host = ctx.host_in_zone("web", &zone);
    assert_eq!(
        ctx.post_as(
            "/inventory/hosts",
            json!({
                "name": host,
                "zone": zone,
                "comment": "seed host",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let network = ctx.cidr(30);
    assert_eq!(
        ctx.post_as(
            "/inventory/networks",
            json!({
                "cidr": network,
                "description": "seed network",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let (record_status, record_body) = ctx
        .post_json_as(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": host,
                "ttl": 300,
                "data": {
                    "value": "seed"
                }
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await;
    assert_eq!(record_status, StatusCode::CREATED);

    let atom = ctx.name("atom");
    assert_eq!(
        ctx.post_as(
            "/policy/host/atoms",
            json!({
                "name": atom,
                "description": "seed atom",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let role = ctx.name("role");
    assert_eq!(
        ctx.post_as(
            "/policy/host/roles",
            json!({
                "name": role,
                "description": "seed role",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    SeededData {
        label,
        nameserver,
        zone,
        host,
        network,
        record_id: record_body["id"].as_str().expect("record id").to_string(),
        rrset_id: record_body["rrset_id"]
            .as_str()
            .expect("rrset id")
            .to_string(),
        atom,
        role,
    }
}

pub(crate) async fn seed_host_on_network(
    ctx: &TestCtx,
    host_stem: &str,
    cidr: &str,
    host_octet: u8,
) -> String {
    assert_eq!(
        ctx.post_as(
            "/inventory/networks",
            json!({
                "cidr": cidr,
                "description": format!("seed network {cidr}"),
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let host = ctx.host(host_stem);
    assert_eq!(
        ctx.post_as(
            "/inventory/hosts",
            json!({
                "name": host,
                "comment": "seed host on network",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    assert_eq!(
        ctx.post_as(
            "/inventory/ip-addresses",
            json!({
                "host_name": host,
                "address": ctx.ip_in_cidr(cidr, host_octet),
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    host
}
