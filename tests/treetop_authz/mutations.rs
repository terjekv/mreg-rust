use actix_web::http::StatusCode;
use rstest::rstest;
use serde_json::json;

use crate::common::TestCtx;
use crate::fixtures::{
    ADMIN_GROUP, ADMIN_USER, READONLY_GROUP, READONLY_USER, SeededData, seed_core_data, treetop_ctx,
};

async fn seed_nested_host_target(ctx: &TestCtx, seeded: &SeededData) -> String {
    let nested_zone = format!("child.{}", seeded.zone);
    assert_eq!(
        ctx.post_as(
            "/dns/forward-zones",
            json!({
                "name": nested_zone,
                "primary_ns": seeded.nameserver,
                "nameservers": [seeded.nameserver],
                "email": format!("hostmaster@{nested_zone}"),
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );
    let nested_host = format!("zonebox.{nested_zone}");
    assert_eq!(
        ctx.post_as(
            "/inventory/hosts",
            json!({
                "name": nested_host,
                "zone": nested_zone,
                "comment": "nested host",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );
    nested_host
}

async fn create_export_template(ctx: &TestCtx, user: &str, group: &str, stem: &str) -> String {
    let template = ctx.name(stem);
    assert_eq!(
        ctx.post_as(
            "/workflows/export-templates",
            json!({
                "name": template,
                "description": "network summary",
                "engine": "minijinja",
                "scope": "inventory",
                "body": "{% for network in networks %}{{ network.cidr }}{% endfor %}",
            }),
            user,
            &[group],
        )
        .await,
        StatusCode::CREATED
    );
    template
}

#[actix_web::test]
async fn host_create_is_allowed_for_host_operators() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    let host = ctx.host("operator-host");
    assert_eq!(
        ctx.post_as(
            "/inventory/hosts",
            json!({
                "name": host,
                "comment": "created by host operator",
            }),
            "heidi",
            &["mreg-host-operators"],
        )
        .await,
        StatusCode::CREATED
    );
}

#[rstest]
#[case::comment_allowed("cora", "mreg-host-commenters", json!({"comment": "comment updated"}), StatusCode::OK)]
#[case::commenter_cannot_change_zone("cora", "mreg-host-commenters", json!({"zone": "forbidden.example.test"}), StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn host_comment_permissions(
    #[case] user: &str,
    #[case] group: &str,
    #[case] payload: serde_json::Value,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;
    let endpoint = format!("/inventory/hosts/{}", seeded.host);

    assert_eq!(
        ctx.patch_as(&endpoint, payload, user, &[group]).await,
        expected
    );
}

#[rstest]
#[case::zone_editor_allowed("zane", "mreg-host-zone-editors", StatusCode::OK)]
#[case::commenter_cannot_change_zone("cora", "mreg-host-commenters", StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn host_zone_permissions(
    #[case] user: &str,
    #[case] group: &str,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;
    let nested_host = seed_nested_host_target(&ctx, &seeded).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/inventory/hosts/{nested_host}"),
            json!({ "zone": seeded.zone }),
            user,
            &[group],
        )
        .await,
        expected
    );
}

#[actix_web::test]
async fn host_zone_editors_cannot_update_comments() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/inventory/hosts/{}", seeded.host),
            json!({ "comment": "not allowed" }),
            "zane",
            &["mreg-host-zone-editors"],
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
async fn label_managers_can_update_descriptions() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/inventory/labels/{}", seeded.label),
            json!({ "description": "updated label description" }),
            "lara",
            &["mreg-label-managers"],
        )
        .await,
        StatusCode::OK
    );
}

#[actix_web::test]
async fn nameserver_operators_can_update_ttl() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/dns/nameservers/{}", seeded.nameserver),
            json!({ "ttl": 7200 }),
            "nina",
            &["mreg-nameserver-operators"],
        )
        .await,
        StatusCode::OK
    );
}

#[actix_web::test]
async fn network_operators_can_create_networks() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let cidr = ctx.cidr(45);

    assert_eq!(
        ctx.post_as(
            "/inventory/networks",
            json!({
                "cidr": cidr,
                "description": "created by network operator",
            }),
            "nick",
            &["mreg-network-operators"],
        )
        .await,
        StatusCode::CREATED
    );
}

#[rstest]
#[case::reserved_allowed(json!({"reserved": 42}), StatusCode::OK)]
#[case::description_denied(json!({"description": "should be denied"}), StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn network_reserved_editor_permissions(
    #[case] payload: serde_json::Value,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/inventory/networks/{}", seeded.network),
            payload,
            "ruth",
            &["mreg-network-reserved-editors"],
        )
        .await,
        expected
    );
}

#[actix_web::test]
async fn zone_operators_can_create_forward_zones() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;
    let zone = ctx.zone("managed");

    assert_eq!(
        ctx.post_as(
            "/dns/forward-zones",
            json!({
                "name": zone,
                "primary_ns": seeded.nameserver,
                "nameservers": [seeded.nameserver],
                "email": format!("hostmaster@{zone}"),
            }),
            "zoe",
            &["mreg-zone-operators"],
        )
        .await,
        StatusCode::CREATED
    );
}

#[rstest]
#[case::timing_allowed(json!({"refresh": 9000}), StatusCode::OK)]
#[case::email_denied(json!({"email": "dns@example.test"}), StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn zone_timing_editor_permissions(
    #[case] payload: serde_json::Value,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/dns/forward-zones/{}", seeded.zone),
            payload,
            "tina",
            &["mreg-zone-timing-editors"],
        )
        .await,
        expected
    );
}

#[actix_web::test]
async fn record_operators_can_create_records() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.post_as(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": seeded.host,
                "ttl": 300,
                "data": {
                    "value": "created by record operator",
                }
            }),
            "rex",
            &["mreg-record-operators"],
        )
        .await,
        StatusCode::CREATED
    );
}

#[rstest]
#[case::ttl_allowed("troy", "mreg-record-ttl-editors", json!({"ttl": 600}), StatusCode::OK)]
#[case::ttl_editor_cannot_change_data("troy", "mreg-record-ttl-editors", json!({"data": {"value": "should be denied"}}), StatusCode::FORBIDDEN)]
#[case::data_allowed("dana", "mreg-record-data-editors", json!({"data": {"value": "updated data"}}), StatusCode::OK)]
#[case::data_editor_cannot_change_ttl("dana", "mreg-record-data-editors", json!({"ttl": 1200}), StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn record_patch_permissions(
    #[case] user: &str,
    #[case] group: &str,
    #[case] payload: serde_json::Value,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/dns/records/{}", seeded.record_id),
            payload,
            user,
            &[group],
        )
        .await,
        expected
    );
}

#[actix_web::test]
async fn host_contact_managers_can_create_contacts() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;
    let email = format!("{}@example.test", ctx.name("contact"));

    assert_eq!(
        ctx.post_as(
            "/inventory/host-contacts",
            json!({
                "email": email,
                "display_name": "Host Contact",
                "hosts": [seeded.host],
            }),
            "hank",
            &["mreg-host-contact-managers"],
        )
        .await,
        StatusCode::CREATED
    );
}

#[actix_web::test]
async fn host_policy_managers_can_update_atoms() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.patch_as(
            &format!("/policy/host/atoms/{}", seeded.atom),
            json!({ "description": "updated atom description" }),
            "polly",
            &["mreg-host-policy-managers"],
        )
        .await,
        StatusCode::OK
    );
}

#[actix_web::test]
async fn host_policy_managers_can_attach_atoms_to_roles() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    assert_eq!(
        ctx.post_as(
            &format!("/policy/host/roles/{}/atoms/{}", seeded.role, seeded.atom),
            json!({}),
            "polly",
            &["mreg-host-policy-managers"],
        )
        .await,
        StatusCode::NO_CONTENT
    );
}

#[actix_web::test]
async fn workflow_creators_can_create_imports() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    assert_eq!(
        ctx.post_as(
            "/workflows/imports",
            json!({
                "requested_by": "wendy",
                "items": [
                    {
                        "ref": ctx.name("import-network"),
                        "kind": "network",
                        "operation": "create",
                        "attributes": {
                            "cidr": ctx.cidr(52),
                            "description": "created by workflow creator"
                        }
                    }
                ]
            }),
            "wendy",
            &["mreg-workflow-creators"],
        )
        .await,
        StatusCode::CREATED
    );
}

#[actix_web::test]
async fn workflow_creators_can_create_export_templates() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    create_export_template(&ctx, "wendy", "mreg-workflow-creators", "network-summary").await;
}

#[actix_web::test]
async fn workflow_creators_can_create_export_runs() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    let template =
        create_export_template(&ctx, "wendy", "mreg-workflow-creators", "workflow-template").await;
    assert_eq!(
        ctx.post_as(
            "/workflows/export-runs",
            json!({
                "template_name": template,
                "requested_by": "wendy",
                "scope": "inventory",
            }),
            "wendy",
            &["mreg-workflow-creators"],
        )
        .await,
        StatusCode::CREATED
    );
}

#[rstest]
#[case::create_host(
    "/inventory/hosts",
    json!({"name": "readonly-create.test", "comment": "denied"}),
    StatusCode::FORBIDDEN
)]
#[case::create_import(
    "/workflows/imports",
    json!({
        "requested_by": "bob",
        "items": [
            {
                "ref": "readonly-import",
                "kind": "network",
                "operation": "create",
                "attributes": {
                    "cidr": "10.200.1.0/24",
                    "description": "denied"
                }
            }
        ]
    }),
    StatusCode::FORBIDDEN
)]
#[case::run_worker("/workflows/tasks/run-next", json!({}), StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn readonly_post_permissions(
    #[case] endpoint: &str,
    #[case] body: serde_json::Value,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    assert_eq!(
        ctx.post_as(endpoint, body, READONLY_USER, &[READONLY_GROUP])
            .await,
        expected
    );
}

#[actix_web::test]
async fn readonly_cannot_patch_labels() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    assert_eq!(
        ctx.post_as(
            "/inventory/labels",
            json!({
                "name": "readonly-label",
                "description": "seed",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );
    assert_eq!(
        ctx.patch_as(
            "/inventory/labels/readonly-label",
            json!({"description": "denied"}),
            READONLY_USER,
            &[READONLY_GROUP],
        )
        .await,
        StatusCode::FORBIDDEN
    );
}
