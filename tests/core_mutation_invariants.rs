mod common;

use common::TestCtx;
use mreg_rust::{
    domain::{
        attachment::{CreateAttachmentCommunityAssignment, CreateHostAttachment},
        community::{Community, CreateCommunity, UpdateCommunity},
        host::{AssignIpAddress, Host, IpAddressAssignment},
        host_community_assignment::CreateHostCommunityAssignment,
        imports::CreateImportBatch,
        network::CreateNetwork,
        network_policy::{CreateNetworkPolicy, UpdateNetworkPolicy},
        pagination::PageRequest,
        ptr_override::CreatePtrOverride,
        resource_records::{CreateRecordInstance, RecordInstance, RecordOwnerKind},
        types::{
            CidrValue, CommunityLimit, CommunityName, DnsName, Hostname, IpAddressValue,
            MacAddressValue, NetworkPolicyName, ReservedCount, ZoneName, record_type_names,
        },
        zone::{CreateForwardZoneDelegation, ForwardZoneDelegation, UpdateForwardZoneDelegation},
    },
    errors::AppError,
};
use serde_json::json;
use uuid::Uuid;

async fn policy_network(ctx: &TestCtx, limit: u32) -> (NetworkPolicyName, CidrValue) {
    let policy = NetworkPolicyName::new(ctx.name("policy")).unwrap();
    let cidr = CidrValue::new(ctx.cidr(1)).unwrap();
    let storage = ctx.storage();
    storage
        .network_policies()
        .create_network_policy(CreateNetworkPolicy::new(policy.clone(), "Policy", None).unwrap())
        .await
        .unwrap();
    storage
        .networks()
        .create_network(
            CreateNetwork::new(cidr.clone(), "Network", ReservedCount::new(3).unwrap())
                .unwrap()
                .with_policy(Some(policy.clone()))
                .with_max_communities(Some(CommunityLimit::new(limit).unwrap())),
        )
        .await
        .unwrap();
    (policy, cidr)
}

fn community_command(
    ctx: &TestCtx,
    policy: &NetworkPolicyName,
    cidr: &CidrValue,
    stem: &str,
) -> CreateCommunity {
    CreateCommunity::new(
        policy.clone(),
        cidr.clone(),
        CommunityName::new(ctx.name(stem)).unwrap(),
        "Community",
    )
    .unwrap()
}

dual_backend_test!(direct_community_creation_enforces_policy, |ctx| {
    let (policy, _) = policy_network(&ctx, 5).await;
    let other = CidrValue::new(ctx.cidr(2)).unwrap();
    ctx.seed_network(&other.as_str()).await;
    let result = ctx
        .storage()
        .communities()
        .create_community(community_command(&ctx, &policy, &other, "invalid"))
        .await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
});

dual_backend_test!(direct_community_creation_enforces_limit, |ctx| {
    let (policy, cidr) = policy_network(&ctx, 1).await;
    let storage = ctx.storage();
    storage
        .communities()
        .create_community(community_command(&ctx, &policy, &cidr, "first"))
        .await
        .unwrap();
    assert!(matches!(
        storage
            .communities()
            .create_community(community_command(&ctx, &policy, &cidr, "second"))
            .await,
        Err(AppError::Conflict(_))
    ));
});

dual_backend_test!(concurrent_community_creation_respects_last_slot, |ctx| {
    let (policy, cidr) = policy_network(&ctx, 1).await;
    let storage = ctx.storage();
    let commands = (0..12)
        .map(|i| community_command(&ctx, &policy, &cidr, &format!("racer{i}")))
        .collect::<Vec<_>>();
    let results = futures::future::join_all(
        commands
            .into_iter()
            .map(|command| storage.communities().create_community(command)),
    )
    .await;
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
});

dual_backend_test!(import_community_limit_failure_rolls_back_batch, |ctx| {
    let (policy, cidr) = policy_network(&ctx, 1).await;
    let storage = ctx.storage();
    let items = ["one", "two"].into_iter().map(|name| json!({"ref":name,"kind":"community","operation":"create",
        "attributes":{"policy_name":policy.as_str(),"network":cidr.as_str(),"name":ctx.name(name),"description":"Imported"}})).collect::<Vec<_>>();
    let batch = storage
        .imports()
        .create_import_batch(CreateImportBatch::new(
            serde_json::from_value(json!({"items":items})).unwrap(),
            None,
        ))
        .await
        .unwrap();
    storage
        .imports()
        .run_import_batch(batch.id())
        .await
        .expect_err("limit must apply to imports");
    let communities = storage
        .communities()
        .list_communities(&PageRequest::all(), &Default::default())
        .await
        .unwrap();
    assert!(
        !communities
            .items
            .iter()
            .any(|item| item.network_cidr() == &cidr)
    );
});

dual_backend_test!(import_community_requires_network_policy, |ctx| {
    let (policy, _) = policy_network(&ctx, 1).await;
    let cidr = ctx.cidr(2);
    ctx.seed_network(&cidr).await;
    let storage = ctx.storage();
    let batch = storage.imports().create_import_batch(CreateImportBatch::new(serde_json::from_value(json!({"items":[{
        "ref":"invalid","kind":"community","operation":"create","attributes":{
            "policy_name":policy.as_str(),"network":cidr,"name":ctx.name("invalid"),"description":"Imported"}
    }]})).unwrap(), None)).await.unwrap();
    assert!(
        storage
            .imports()
            .run_import_batch(batch.id())
            .await
            .is_err()
    );
});

dual_backend_test!(network_cannot_clear_policy_with_communities, |ctx| {
    let (policy, cidr) = policy_network(&ctx, 2).await;
    ctx.storage()
        .communities()
        .create_community(community_command(&ctx, &policy, &cidr, "existing"))
        .await
        .unwrap();
    let response = ctx
        .patch(
            &format!("/inventory/networks/{}", cidr.as_str()),
            json!({"policy_name":null}),
        )
        .await;
    assert_eq!(response, actix_web::http::StatusCode::CONFLICT);
});

dual_backend_test!(network_cannot_lower_limit_below_existing_count, |ctx| {
    let (policy, cidr) = policy_network(&ctx, 2).await;
    ctx.storage()
        .communities()
        .create_community(community_command(&ctx, &policy, &cidr, "existing"))
        .await
        .unwrap();
    let response = ctx
        .patch(
            &format!("/inventory/networks/{}", cidr.as_str()),
            json!({"max_communities":0}),
        )
        .await;
    assert_eq!(response, actix_web::http::StatusCode::CONFLICT);
});

struct DelegationFixture {
    zone: ZoneName,
    delegation: ForwardZoneDelegation,
    records: Vec<RecordInstance>,
    next_ns: DnsName,
}
async fn delegation_fixture(ctx: &TestCtx) -> DelegationFixture {
    let zone = ctx.zone("parent");
    let ns = ctx.nameserver("ns", &zone);
    ctx.seed_zone(&zone, &ns).await;
    let storage = ctx.storage();
    let next_ns = DnsName::new(ctx.nameserver("next", &zone)).unwrap();
    ctx.post("/dns/nameservers", json!({"name":next_ns.as_str()}))
        .await;
    let name = DnsName::new(format!("child.{zone}")).unwrap();
    let delegation = storage
        .zones()
        .create_forward_zone_delegation(
            CreateForwardZoneDelegation::new(
                ZoneName::new(&zone).unwrap(),
                name.clone(),
                "Original".into(),
                vec![DnsName::new(ns).unwrap()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let ds = storage
        .records()
        .create_record(
            CreateRecordInstance::new(
                record_type_names::ds(),
                RecordOwnerKind::ForwardZoneDelegation,
                name.as_str(),
                None,
                json!({"key_tag":12345,"algorithm":13,"digest_type":2,"digest":"ab".repeat(32)}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let glue = storage
        .records()
        .create_record(
            CreateRecordInstance::new_anchored(
                record_type_names::a(),
                RecordOwnerKind::ForwardZoneDelegation,
                format!("ns.{}", name.as_str()),
                name.as_str(),
                None,
                json!({"address":"192.0.2.10"}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    DelegationFixture {
        zone: ZoneName::new(zone).unwrap(),
        delegation,
        records: vec![ds, glue],
        next_ns,
    }
}

async fn retained_records(ctx: &TestCtx, fixture: &DelegationFixture) -> Vec<RecordInstance> {
    let mut result = Vec::new();
    for record in &fixture.records {
        result.push(
            ctx.storage()
                .records()
                .get_record(record.id())
                .await
                .unwrap(),
        );
    }
    result
}

dual_backend_test!(delegation_comment_preserves_dns_and_serial, |ctx| {
    let fixture = delegation_fixture(&ctx).await;
    let storage = ctx.storage();
    let serial = storage
        .zones()
        .get_forward_zone_by_name(&fixture.zone)
        .await
        .unwrap()
        .serial_no();
    let id = fixture.delegation.id();
    storage
        .transaction(move |tx| {
            tx.zones().update_forward_zone_delegation(
                id,
                UpdateForwardZoneDelegation::new(Some("Changed".into()), None)?,
            )
        })
        .await
        .unwrap();
    assert_eq!(
        (
            serde_json::to_value(retained_records(&ctx, &fixture).await).unwrap(),
            storage
                .zones()
                .get_forward_zone_by_name(&fixture.zone)
                .await
                .unwrap()
                .serial_no()
        ),
        (serde_json::to_value(fixture.records).unwrap(), serial)
    );
});

dual_backend_test!(delegation_nameservers_preserve_ds_and_glue, |ctx| {
    let fixture = delegation_fixture(&ctx).await;
    let id = fixture.delegation.id();
    let ns = fixture.next_ns.clone();
    ctx.storage()
        .transaction(move |tx| {
            tx.zones().update_forward_zone_delegation(
                id,
                UpdateForwardZoneDelegation::new(None, Some(vec![ns]))?,
            )
        })
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(retained_records(&ctx, &fixture).await).unwrap(),
        serde_json::to_value(fixture.records).unwrap()
    );
});

dual_backend_test!(
    delegation_nameserver_failure_preserves_metadata_and_dns,
    |ctx| {
        let fixture = delegation_fixture(&ctx).await;
        let id = fixture.delegation.id();
        let unknown = DnsName::new(ctx.host("unknown")).unwrap();
        ctx.storage()
            .transaction(move |tx| {
                tx.zones().update_forward_zone_delegation(
                    id,
                    UpdateForwardZoneDelegation::new(Some("Changed".into()), Some(vec![unknown]))?,
                )
            })
            .await
            .expect_err("unknown nameserver must fail");
        let current = ctx
            .storage()
            .zones()
            .list_forward_zone_delegations(&fixture.zone, &PageRequest::all())
            .await
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.id() == id)
            .unwrap();
        assert_eq!(
            (
                current,
                serde_json::to_value(retained_records(&ctx, &fixture).await).unwrap()
            ),
            (
                fixture.delegation,
                serde_json::to_value(fixture.records).unwrap()
            )
        );
    }
);

async fn host_ip(ctx: &TestCtx, cidr: &CidrValue) -> (Host, IpAddressAssignment) {
    let name = Hostname::new(ctx.host("host")).unwrap();
    ctx.seed_host(name.as_str()).await;
    let storage = ctx.storage();
    let host = storage.hosts().get_host_by_name(&name).await.unwrap();
    let ip = storage
        .hosts()
        .assign_ip_address(
            AssignIpAddress::new(
                name,
                Some(IpAddressValue::new(ctx.ip_in_cidr(&cidr.as_str(), 10)).unwrap()),
                None,
                None,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    (host, ip)
}

dual_backend_test!(ip_move_honors_exact_attachment, |ctx| {
    let (_, cidr) = policy_network(&ctx, 2).await;
    let (host, ip) = host_ip(&ctx, &cidr).await;
    let storage = ctx.storage();
    let attachment = storage
        .attachments()
        .create_attachment(CreateHostAttachment::new(
            host.name().clone(),
            cidr.clone(),
            Some(MacAddressValue::new("02:00:00:00:00:91").unwrap()),
            None,
        ))
        .await
        .unwrap();
    let command = AssignIpAddress::new(
        host.name().clone(),
        Some(IpAddressValue::new(ctx.ip_in_cidr(&cidr.as_str(), 11)).unwrap()),
        None,
        None,
    )
    .unwrap()
    .within_attachment(attachment.id());
    let old = *ip.address();
    let moved = storage
        .transaction(move |tx| tx.hosts().move_ip_address(&old, command))
        .await
        .unwrap();
    assert_eq!(
        (moved.id(), moved.attachment_id(), moved.mac_address()),
        (ip.id(), attachment.id(), attachment.mac_address())
    );
});

dual_backend_test!(ip_move_rejects_other_hosts_attachment_atomically, |ctx| {
    let (_, cidr) = policy_network(&ctx, 2).await;
    let (host, ip) = host_ip(&ctx, &cidr).await;
    let other = Hostname::new(ctx.host("other")).unwrap();
    ctx.seed_host(other.as_str()).await;
    let storage = ctx.storage();
    let attachment = storage
        .attachments()
        .create_attachment(CreateHostAttachment::new(other, cidr.clone(), None, None))
        .await
        .unwrap();
    let command = AssignIpAddress::new(
        host.name().clone(),
        Some(IpAddressValue::new(ctx.ip_in_cidr(&cidr.as_str(), 11)).unwrap()),
        None,
        None,
    )
    .unwrap()
    .within_attachment(attachment.id());
    let old = *ip.address();
    storage
        .transaction(move |tx| tx.hosts().move_ip_address(&old, command))
        .await
        .expect_err("attachment owner must be checked");
    assert_eq!(storage.hosts().get_ip_address(&old).await.unwrap(), ip);
});

dual_backend_test!(ip_move_with_ptr_override_is_rejected_atomically, |ctx| {
    let (_, cidr) = policy_network(&ctx, 2).await;
    let (host, ip) = host_ip(&ctx, &cidr).await;
    let storage = ctx.storage();
    let ptr = storage
        .ptr_overrides()
        .create_ptr_override(CreatePtrOverride::new(
            host.name().clone(),
            *ip.address(),
            Some(DnsName::new(ctx.host("ptr")).unwrap()),
        ))
        .await
        .unwrap();
    let command = AssignIpAddress::new(
        host.name().clone(),
        Some(IpAddressValue::new(ctx.ip_in_cidr(&cidr.as_str(), 11)).unwrap()),
        None,
        None,
    )
    .unwrap();
    let old = *ip.address();
    storage
        .transaction(move |tx| tx.hosts().move_ip_address(&old, command))
        .await
        .expect_err("override must protect the old assignment");
    assert_eq!(
        (
            storage.hosts().get_ip_address(&old).await.unwrap(),
            storage
                .ptr_overrides()
                .get_ptr_override_by_address(&old)
                .await
                .unwrap()
        ),
        (ip, ptr)
    );
});

dual_backend_test!(unassignment_removes_ptr_override, |ctx| {
    let (_, cidr) = policy_network(&ctx, 2).await;
    let (host, ip) = host_ip(&ctx, &cidr).await;
    let storage = ctx.storage();
    storage
        .ptr_overrides()
        .create_ptr_override(CreatePtrOverride::new(
            host.name().clone(),
            *ip.address(),
            None,
        ))
        .await
        .unwrap();
    storage
        .hosts()
        .unassign_ip_address(ip.address())
        .await
        .unwrap();
    assert!(matches!(
        storage
            .ptr_overrides()
            .get_ptr_override_by_address(ip.address())
            .await,
        Err(AppError::NotFound(_))
    ));
});

struct MembershipFixture {
    policy: NetworkPolicyName,
    community: Community,
    host_mapping: Uuid,
    attachment_mapping: Uuid,
}
async fn memberships(ctx: &TestCtx) -> MembershipFixture {
    let (policy, cidr) = policy_network(ctx, 5).await;
    let (host, ip) = host_ip(ctx, &cidr).await;
    let storage = ctx.storage();
    let community = storage
        .communities()
        .create_community(community_command(ctx, &policy, &cidr, "community"))
        .await
        .unwrap();
    let host_mapping = storage
        .host_community_assignments()
        .create_host_community_assignment(CreateHostCommunityAssignment::new(
            host.name().clone(),
            *ip.address(),
            policy.clone(),
            community.name().clone(),
        ))
        .await
        .unwrap();
    let attachment_mapping = storage
        .attachment_community_assignments()
        .create_attachment_community_assignment(CreateAttachmentCommunityAssignment::new(
            ip.attachment_id(),
            policy.clone(),
            community.name().clone(),
        ))
        .await
        .unwrap();
    MembershipFixture {
        policy,
        community,
        host_mapping: host_mapping.id(),
        attachment_mapping: attachment_mapping.id(),
    }
}

dual_backend_test!(policy_rename_updates_all_current_references, |ctx| {
    let fixture = memberships(&ctx).await;
    let name = NetworkPolicyName::new(ctx.name("renamed")).unwrap();
    let storage = ctx.storage();
    storage
        .network_policies()
        .update_network_policy(
            &fixture.policy,
            UpdateNetworkPolicy {
                name: Some(name.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let community = storage
        .communities()
        .get_community(fixture.community.id())
        .await
        .unwrap();
    let host = storage
        .host_community_assignments()
        .get_host_community_assignment(fixture.host_mapping)
        .await
        .unwrap();
    let attachment = storage
        .attachment_community_assignments()
        .get_attachment_community_assignment(fixture.attachment_mapping)
        .await
        .unwrap();
    assert_eq!(
        (
            community.policy_name(),
            host.policy_name(),
            attachment.policy_name()
        ),
        (&name, &name, &name)
    );
});

dual_backend_test!(community_rename_updates_both_assignment_kinds, |ctx| {
    let fixture = memberships(&ctx).await;
    let name = CommunityName::new(ctx.name("renamed")).unwrap();
    let storage = ctx.storage();
    storage
        .communities()
        .update_community(
            fixture.community.id(),
            UpdateCommunity {
                name: Some(name.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let host = storage
        .host_community_assignments()
        .get_host_community_assignment(fixture.host_mapping)
        .await
        .unwrap();
    let attachment = storage
        .attachment_community_assignments()
        .get_attachment_community_assignment(fixture.attachment_mapping)
        .await
        .unwrap();
    assert_eq!(
        (host.community_name(), attachment.community_name()),
        (&name, &name)
    );
});

#[test]
fn delegation_update_rejects_empty_nameservers() {
    assert!(UpdateForwardZoneDelegation::new(None, Some(Vec::new())).is_err());
}

#[rstest::rstest]
#[case("insert")]
#[case("policy")]
#[case("limit")]
#[actix_web::test]
async fn postgres_direct_sql_preserves_community_invariants(#[case] operation: &str) {
    use diesel::result::{DatabaseErrorKind, Error};
    use diesel::{Connection, PgConnection, RunQueryDsl, sql_query, sql_types::Uuid as SqlUuid};
    let Some(ctx) = TestCtx::postgres().await else {
        eprintln!(
            "{}",
            common::postgres_skip_message("direct_sql_community_invariants")
        );
        return;
    };
    let (policy, cidr) = policy_network(&ctx, 1).await;
    let storage = ctx.storage();
    let community = storage
        .communities()
        .create_community(community_command(&ctx, &policy, &cidr, "existing"))
        .await
        .unwrap();
    let network = storage.networks().get_network_by_cidr(&cidr).await.unwrap();
    let mut connection =
        PgConnection::establish(&common::postgres_test_database_url().unwrap().unwrap()).unwrap();
    let result = match operation {
        "insert" => sql_query("INSERT INTO communities (policy_id, network_id, name, description) VALUES ($1, $2, 'overflow', 'Overflow')")
            .bind::<SqlUuid,_>(community.policy_id()).bind::<SqlUuid,_>(network.id()).execute(&mut connection),
        "policy" => sql_query("UPDATE networks SET policy_id = NULL WHERE id = $1")
            .bind::<SqlUuid,_>(network.id()).execute(&mut connection),
        "limit" => sql_query("UPDATE networks SET max_communities = 0 WHERE id = $1")
            .bind::<SqlUuid,_>(network.id()).execute(&mut connection),
        _ => unreachable!(),
    };
    assert!(matches!(
        result,
        Err(Error::DatabaseError(DatabaseErrorKind::CheckViolation, _))
    ));
}

dual_backend_test!(
    explicit_attachment_retains_network_on_reload_and_move,
    |ctx| {
        let narrow = CidrValue::new(ctx.cidr(8)).unwrap();
        let broad = CidrValue::new(ctx.cidr(8).replace("/24", "/23")).unwrap();
        ctx.seed_network(&broad.as_str()).await;
        ctx.seed_network(&narrow.as_str()).await;
        let storage = ctx.storage();
        let name = Hostname::new(ctx.host("overlap")).unwrap();
        ctx.seed_host(name.as_str()).await;
        let attachment = storage
            .attachments()
            .create_attachment(CreateHostAttachment::new(name.clone(), broad, None, None))
            .await
            .unwrap();
        let first = IpAddressValue::new(ctx.ip_in_cidr(&narrow.as_str(), 10)).unwrap();
        let next = IpAddressValue::new(ctx.ip_in_cidr(&narrow.as_str(), 11)).unwrap();
        storage
            .hosts()
            .assign_ip_address(
                AssignIpAddress::new(name.clone(), Some(first), None, None)
                    .unwrap()
                    .within_attachment(attachment.id()),
            )
            .await
            .unwrap();
        let before = storage.hosts().get_ip_address(&first).await.unwrap();
        let command = AssignIpAddress::new(name, Some(next), None, None)
            .unwrap()
            .within_attachment(attachment.id());
        storage
            .transaction(move |tx| tx.hosts().move_ip_address(&first, command))
            .await
            .unwrap();
        let after = storage.hosts().get_ip_address(&next).await.unwrap();
        assert_eq!(
            (
                before.network_id(),
                after.network_id(),
                after.attachment_id()
            ),
            (
                attachment.network_id(),
                attachment.network_id(),
                attachment.id()
            )
        );
    }
);

dual_backend_test!(
    referenced_policy_deletion_returns_conflict_without_cascade,
    |ctx| {
        let (policy, cidr) = policy_network(&ctx, 1).await;
        let storage = ctx.storage();
        let community = storage
            .communities()
            .create_community(community_command(&ctx, &policy, &cidr, "referenced"))
            .await
            .unwrap();
        let rejected = matches!(
            storage
                .network_policies()
                .delete_network_policy(&policy)
                .await,
            Err(AppError::Conflict(_))
        );
        let current = storage
            .communities()
            .get_community(community.id())
            .await
            .unwrap();
        assert_eq!((rejected, current.id()), (true, community.id()));
    }
);
