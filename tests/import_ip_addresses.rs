mod common;

use actix_web::http::StatusCode;
use mreg_rust::domain::types::IpAddressValue;
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

fn attachment_items(ctx: &TestCtx) -> Value {
    json!([
        {
            "ref": "network",
            "kind": "network",
            "operation": "create",
            "attributes": { "cidr": ctx.cidr(0), "description": "Import network" }
        },
        {
            "ref": "host",
            "kind": "host",
            "operation": "create",
            "attributes": { "name": ctx.host("import") }
        },
        {
            "ref": "attachment",
            "kind": "host_attachment",
            "operation": "create",
            "attributes": {
                "host_name_ref": "host",
                "network_ref": "network",
                "mac_address": "AA:BB:CC:DD:EE:FF"
            }
        }
    ])
}

fn push_ip(items: &mut Value, attributes: Value) {
    items.as_array_mut().unwrap().push(json!({
        "ref": "ip",
        "kind": "ip_address",
        "operation": "create",
        "attributes": attributes
    }));
}

async fn stage_import(ctx: &TestCtx, items: Value) -> Uuid {
    let (status, body) = ctx
        .post_json("/workflows/imports", json!({ "items": items }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

#[rstest]
#[case::manual(false, true)]
#[case::manual_with_host(true, true)]
#[case::automatic(false, false)]
#[case::automatic_with_host(true, false)]
#[actix_web::test]
async fn import_ip_on_staged_attachment(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[case] include_host: bool,
    #[case] manual: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut items = attachment_items(&ctx);
    // An overlapping, more specific network must not redirect the assignment
    // away from the explicitly referenced attachment.
    items.as_array_mut().unwrap().push(json!({
        "ref": "narrow-network",
        "kind": "network",
        "operation": "create",
        "attributes": {
            "cidr": ctx.cidr(0).replace("/24", "/25"),
            "description": "More specific network"
        }
    }));
    let mut attributes = json!({ "attachment_id_ref": "attachment" });
    if include_host {
        attributes["host_name_ref"] = json!("host");
    }
    if manual {
        attributes["address"] = json!(ctx.ip_in_cidr(&ctx.cidr(0), 20));
    } else if include_host {
        attributes["network_ref"] = json!("network");
    }
    push_ip(&mut items, attributes);
    let id = stage_import(&ctx, items).await;
    let result = ctx.storage().imports().run_import_batch(id).await.unwrap();
    let attachment_id = &result.commit_summary().unwrap()["applied"][2]["result"];
    let address = ctx.ip_in_cidr(&ctx.cidr(0), if manual { 20 } else { 3 });
    let assignment = ctx
        .storage()
        .hosts()
        .get_ip_address(&IpAddressValue::new(address).unwrap())
        .await
        .unwrap();
    assert_eq!(json!(assignment.attachment_id()), *attachment_id);
}

#[rstest]
#[actix_web::test]
async fn import_ip_on_existing_attachment(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let id = stage_import(&ctx, attachment_items(&ctx)).await;
    let result = ctx.storage().imports().run_import_batch(id).await.unwrap();
    let attachment_id = &result.commit_summary().unwrap()["applied"][2]["result"];
    let mut items = json!([]);
    push_ip(
        &mut items,
        json!({
            "attachment_id": attachment_id,
            "address": ctx.ip_in_cidr(&ctx.cidr(0), 20)
        }),
    );
    let id = stage_import(&ctx, items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    let assignment = ctx
        .storage()
        .hosts()
        .get_ip_address(&IpAddressValue::new(ctx.ip_in_cidr(&ctx.cidr(0), 20)).unwrap())
        .await
        .unwrap();
    assert_eq!(json!(assignment.attachment_id()), *attachment_id);
}

#[rstest]
#[case::host("host_name", "other.example.org", "host_name does not match")]
#[case::network("network", "192.0.2.0/24", "network does not match")]
#[case::address("address", "192.0.2.20", "outside the requested attachment network")]
#[actix_web::test]
async fn import_ip_rejects_attachment_mismatch(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[case] field: &str,
    #[case] value: &str,
    #[case] message: &str,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut items = attachment_items(&ctx);
    let mut attributes = json!({ "attachment_id_ref": "attachment" });
    attributes[field] = json!(value);
    push_ip(&mut items, attributes);
    let id = stage_import(&ctx, items).await;
    let error = ctx
        .storage()
        .imports()
        .run_import_batch(id)
        .await
        .unwrap_err();
    assert!(error.to_string().contains(message), "{error}");
}

#[rstest]
#[actix_web::test]
async fn import_ip_outside_attachment_rolls_back(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut items = attachment_items(&ctx);
    push_ip(
        &mut items,
        json!({
            "attachment_id_ref": "attachment",
            "address": "192.0.2.20"
        }),
    );
    let id = stage_import(&ctx, items).await;
    ctx.storage()
        .imports()
        .run_import_batch(id)
        .await
        .unwrap_err();
    assert_eq!(
        ctx.get_status(&format!("/inventory/networks/{}", ctx.cidr(0)))
            .await,
        StatusCode::NOT_FOUND
    );
}

#[rstest]
#[actix_web::test]
async fn import_ip_rejects_explicit_address_and_network(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[values(false, true)] use_attachment: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut items = attachment_items(&ctx);
    let mut attributes = json!({
        "host_name_ref": "host",
        "network_ref": "network",
        "address": ctx.ip_in_cidr(&ctx.cidr(0), 20)
    });
    if use_attachment {
        attributes["attachment_id_ref"] = json!("attachment");
    }
    push_ip(&mut items, attributes);
    let id = stage_import(&ctx, items).await;
    let error = ctx
        .storage()
        .imports()
        .run_import_batch(id)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("exactly one of address or network must be provided"),
        "{error}"
    );
}

#[rstest]
#[case::manual(true)]
#[case::automatic(false)]
#[actix_web::test]
async fn import_ip_without_attachment(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[case] manual: bool,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let mut items = attachment_items(&ctx);
    items.as_array_mut().unwrap().pop();
    let mut attributes = json!({
        "host_name_ref": "host",
        "mac_address": "AA:BB:CC:DD:EE:FF"
    });
    if manual {
        attributes["address"] = json!(ctx.ip_in_cidr(&ctx.cidr(0), 20));
    } else {
        attributes["network_ref"] = json!("network");
    }
    push_ip(&mut items, attributes);
    let id = stage_import(&ctx, items).await;
    ctx.storage().imports().run_import_batch(id).await.unwrap();
    let addresses = ctx
        .get_json(&format!(
            "/inventory/hosts/{}/ip-addresses",
            ctx.host("import")
        ))
        .await;
    assert_eq!(
        json!([
            addresses["items"][0]["address"],
            addresses["items"][0]["mac_address"]
        ]),
        json!([
            ctx.ip_in_cidr(&ctx.cidr(0), if manual { 20 } else { 3 }),
            "AA:BB:CC:DD:EE:FF"
        ])
    );
}
