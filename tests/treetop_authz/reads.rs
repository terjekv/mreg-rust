use actix_web::http::StatusCode;
use rstest::rstest;
use serde_json::json;

use crate::fixtures::{
    ADMIN_GROUP, ADMIN_USER, LEGACY_ALLOWED_HOST_NETWORK, LEGACY_OTHER_HOST_NETWORK,
    READONLY_GROUP, READONLY_USER, seed_core_data, seed_host_on_network, treetop_ctx,
};

#[rstest]
#[case::history("/system/history")]
#[case::tasks("/workflows/tasks")]
#[case::imports("/workflows/imports")]
#[case::export_templates("/workflows/export-templates")]
#[case::export_runs("/workflows/export-runs")]
#[case::record_types("/dns/record-types")]
#[case::rrsets("/dns/rrsets")]
#[case::records("/dns/records")]
#[case::labels("/inventory/labels")]
#[case::nameservers("/dns/nameservers")]
#[case::forward_zones("/dns/forward-zones")]
#[case::reverse_zones("/dns/reverse-zones")]
#[case::hosts("/inventory/hosts")]
#[case::ip_addresses("/inventory/ip-addresses")]
#[case::networks("/inventory/networks")]
#[case::host_contacts("/inventory/host-contacts")]
#[case::host_groups("/inventory/host-groups")]
#[case::bacnet_ids("/inventory/bacnet-ids")]
#[case::ptr_overrides("/dns/ptr-overrides")]
#[case::network_policies("/policy/network/policies")]
#[case::communities("/policy/network/communities")]
#[case::host_community_assignments("/policy/network/host-community-assignments")]
#[case::host_policy_atoms("/policy/host/atoms")]
#[case::host_policy_roles("/policy/host/roles")]
#[actix_web::test]
async fn readonly_can_list_allowed_endpoints(#[case] endpoint: &str) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    assert_eq!(
        ctx.get_status_as(endpoint, READONLY_USER, &[READONLY_GROUP])
            .await,
        StatusCode::OK
    );
}

#[rstest]
#[case::host("host")]
#[case::label("label")]
#[case::nameserver("nameserver")]
#[case::forward_zone("forward_zone")]
#[case::network("network")]
#[case::record("record")]
#[case::rrset("rrset")]
#[case::atom("atom")]
#[case::role("role")]
#[actix_web::test]
async fn readonly_can_get_seeded_resources(#[case] target: &str) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };
    let seeded = seed_core_data(&ctx).await;

    let endpoint = match target {
        "host" => format!("/inventory/hosts/{}", seeded.host),
        "label" => format!("/inventory/labels/{}", seeded.label),
        "nameserver" => format!("/dns/nameservers/{}", seeded.nameserver),
        "forward_zone" => format!("/dns/forward-zones/{}", seeded.zone),
        "network" => format!("/inventory/networks/{}", seeded.network),
        "record" => format!("/dns/records/{}", seeded.record_id),
        "rrset" => format!("/dns/rrsets/{}", seeded.rrset_id),
        "atom" => format!("/policy/host/atoms/{}", seeded.atom),
        "role" => format!("/policy/host/roles/{}", seeded.role),
        other => panic!("unknown get target: {other}"),
    };

    assert_eq!(
        ctx.get_status_as(&endpoint, READONLY_USER, &[READONLY_GROUP])
            .await,
        StatusCode::OK
    );
}

#[rstest]
#[case::allowed_get("allowed_get", StatusCode::OK)]
#[case::allowed_comment("allowed_comment", StatusCode::OK)]
#[case::other_network_get("other_network_get", StatusCode::FORBIDDEN)]
#[case::host_without_network("host_without_network", StatusCode::FORBIDDEN)]
#[actix_web::test]
async fn host_permissions_can_key_off_legacy_network_context(
    #[case] scenario: &str,
    #[case] expected: StatusCode,
) {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    let status = match scenario {
        "allowed_get" => {
            let host =
                seed_host_on_network(&ctx, "legacy-allow", LEGACY_ALLOWED_HOST_NETWORK, 10).await;
            ctx.get_status_as(
                &format!("/inventory/hosts/{host}"),
                "legacy-net",
                &["mreg-host-network-commenters"],
            )
            .await
        }
        "allowed_comment" => {
            let host =
                seed_host_on_network(&ctx, "legacy-comment", LEGACY_ALLOWED_HOST_NETWORK, 11).await;
            ctx.patch_as(
                &format!("/inventory/hosts/{host}"),
                json!({
                    "comment": "legacy-network-approved",
                }),
                "legacy-net",
                &["mreg-host-network-commenters"],
            )
            .await
        }
        "other_network_get" => {
            let host =
                seed_host_on_network(&ctx, "legacy-deny", LEGACY_OTHER_HOST_NETWORK, 10).await;
            ctx.get_status_as(
                &format!("/inventory/hosts/{host}"),
                "legacy-net",
                &["mreg-host-network-commenters"],
            )
            .await
        }
        "host_without_network" => {
            let host = ctx.host("legacy-no-network");
            assert_eq!(
                ctx.post_as(
                    "/inventory/hosts",
                    json!({
                        "name": host,
                        "comment": "no network attached",
                    }),
                    ADMIN_USER,
                    &[ADMIN_GROUP],
                )
                .await,
                StatusCode::CREATED
            );
            ctx.get_status_as(
                &format!("/inventory/hosts/{host}"),
                "legacy-net",
                &["mreg-host-network-commenters"],
            )
            .await
        }
        other => panic!("unknown legacy network scenario: {other}"),
    };

    assert_eq!(status, expected);
}
