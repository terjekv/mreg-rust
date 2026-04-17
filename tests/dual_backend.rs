//! Dual-backend conformance suite.
//!
//! Each scenario runs against both the in-memory and PostgreSQL backends via a
//! shared `TestCtx`. The scenarios exercise backend-neutral behavior only.

mod common;

use std::sync::OnceLock;

use actix_web::http::StatusCode;
use serde_json::json;
use tokio::sync::Mutex;

use common::TestCtx;
use mreg_rust::domain::tasks::{CreateTask, TaskStatus};

fn task_queue_mutex() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn drain_task_queue(ctx: &TestCtx) {
    let storage = ctx.storage();
    while let Some(task) = storage
        .tasks()
        .claim_next_task()
        .await
        .expect("drain queued task")
    {
        let _ = storage
            .tasks()
            .complete_task(task.id(), json!({ "drained": true }))
            .await;
    }
}

async fn label_create_scenario(ctx: &TestCtx) {
    let name = ctx.name("label");
    let status = ctx
        .post(
            "/inventory/labels",
            json!({ "name": name, "description": "test" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    ctx.assert_audit_exists("label", &name, "create").await;
}

async fn label_get_scenario(ctx: &TestCtx) {
    let name = ctx.name("label-get");
    let status = ctx
        .post(
            "/inventory/labels",
            json!({ "name": name, "description": "test" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let body = ctx.get_json(&format!("/inventory/labels/{name}")).await;
    assert_eq!(body["name"], name);
}

async fn label_update_scenario(ctx: &TestCtx) {
    let name = ctx.name("label-update");
    let status = ctx
        .post(
            "/inventory/labels",
            json!({ "name": name, "description": "old" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = ctx
        .patch_json(
            &format!("/inventory/labels/{name}"),
            json!({ "description": "new" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["description"], "new");

    let event = ctx.find_audit_event("label", &name, "update").await;
    assert!(
        event["data"]["old"].is_object(),
        "update event should contain old values"
    );
    assert!(
        event["data"]["new"].is_object(),
        "update event should contain new values"
    );
}

async fn label_delete_scenario(ctx: &TestCtx) {
    let name = ctx.name("label-delete");
    let status = ctx
        .post(
            "/inventory/labels",
            json!({ "name": name, "description": "test" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx.delete(&format!("/inventory/labels/{name}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    ctx.assert_audit_exists("label", &name, "delete").await;

    let status = ctx.get_status(&format!("/inventory/labels/{name}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn label_not_found_scenario(ctx: &TestCtx) {
    let status = ctx
        .get_status(&format!("/inventory/labels/{}", ctx.name("missing")))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn label_duplicate_scenario(ctx: &TestCtx) {
    let name = ctx.name("label-dup");
    let status = ctx
        .post(
            "/inventory/labels",
            json!({ "name": name, "description": "test" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            "/inventory/labels",
            json!({ "name": name, "description": "test" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

async fn label_pagination_walks_without_duplicates_scenario(ctx: &TestCtx) {
    let total = 7usize;
    for index in 0..total {
        let status = ctx
            .post(
                "/inventory/labels",
                json!({
                    "name": ctx.name(&format!("page-{index:03}")),
                    "description": "paged",
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let mut collected = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    let ns = ctx.namespace();

    loop {
        let uri = match &cursor {
            Some(cursor) => format!("/inventory/labels?limit=3&after={cursor}&name__contains={ns}"),
            None => format!("/inventory/labels?limit=3&name__contains={ns}"),
        };
        let body = ctx.get_json(&uri).await;
        assert!(
            body["total"].as_u64().unwrap() >= total as u64,
            "expected at least {total} labels, got {}",
            body["total"]
        );
        collected.extend(
            body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["name"].as_str().unwrap().to_string()),
        );
        pages += 1;
        match body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_string()),
            None => break,
        }
    }

    assert!(
        collected.len() >= total,
        "expected at least {total} labels, got {}",
        collected.len()
    );
    let unique: std::collections::HashSet<&String> = collected.iter().collect();
    assert_eq!(
        unique.len(),
        collected.len(),
        "pagination produced duplicates"
    );
    assert!(pages >= 3, "expected at least 3 pages, got {pages}");
}

async fn host_create_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("hosts");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("app", &zone);
    ctx.seed_zone(&zone, &nameserver).await;

    let status = ctx
        .post(
            "/inventory/hosts",
            json!({ "name": host, "zone": zone, "comment": "created" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    ctx.assert_audit_exists("host", &host, "create").await;
}

async fn host_get_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("host-get");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("app", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&host, &zone).await;

    let body = ctx.get_json(&format!("/inventory/hosts/{host}")).await;
    assert_eq!(body["name"], host);
}

async fn host_rename_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("host-rename");
    let nameserver = ctx.nameserver("ns1", &zone);
    let old = ctx.host_in_zone("old", &zone);
    let new = ctx.host_in_zone("new", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&old, &zone).await;

    let status = ctx
        .patch(&format!("/inventory/hosts/{old}"), json!({ "name": new }))
        .await;
    assert_eq!(status, StatusCode::OK);

    let status = ctx.get_status(&format!("/inventory/hosts/{new}")).await;
    assert_eq!(status, StatusCode::OK);
}

async fn host_delete_cleans_records_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("host-clean");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("cleanup", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&host, &zone).await;

    let status = ctx
        .post(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": host,
                "data": { "value": "x" },
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx.delete(&format!("/inventory/hosts/{host}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let body = ctx
        .get_json(&format!("/dns/records?owner_name={host}"))
        .await;
    assert_eq!(body["total"], 0);
}

async fn host_rename_bumps_serial_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("host-serial");
    let nameserver = ctx.nameserver("ns1", &zone);
    let old = ctx.host_in_zone("serial-old", &zone);
    let new = ctx.host_in_zone("serial-new", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&old, &zone).await;

    let before = ctx.get_json(&format!("/dns/forward-zones/{zone}")).await;
    let status = ctx
        .patch(&format!("/inventory/hosts/{old}"), json!({ "name": new }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let after = ctx.get_json(&format!("/dns/forward-zones/{zone}")).await;

    assert!(after["serial_no"].as_u64().unwrap() > before["serial_no"].as_u64().unwrap());
}

async fn host_zone_mismatch_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("host-zone");
    let nameserver = ctx.nameserver("ns1", &zone);
    let wrong_host = ctx.host("wrong");
    ctx.seed_zone(&zone, &nameserver).await;

    let status = ctx
        .post(
            "/inventory/hosts",
            json!({ "name": wrong_host, "zone": zone, "comment": "mismatch" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

async fn host_not_found_scenario(ctx: &TestCtx) {
    let status = ctx
        .get_status(&format!("/inventory/hosts/{}", ctx.host("missing")))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn ip_assign_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(0);
    let host = ctx.host("ip");
    let address = ctx.ip_in_cidr(&cidr, 10);
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({ "host_name": host, "address": address }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn ip_creates_a_record_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(1);
    let host = ctx.host("arec");
    let address = ctx.ip_in_cidr(&cidr, 10);
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({ "host_name": host, "address": address }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let body = ctx
        .get_json(&format!("/dns/records?type_name=A&owner_name={host}"))
        .await;
    assert!(body["total"].as_u64().unwrap() >= 1);
}

async fn ip_unassign_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(2);
    let host = ctx.host("unassign");
    let address = ctx.ip_in_cidr(&cidr, 10);
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({ "host_name": host, "address": address }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .delete(&format!("/inventory/ip-addresses/{address}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

async fn ip_patch_mac_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(3);
    let host = ctx.host("mac");
    let address = ctx.ip_in_cidr(&cidr, 10);
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({ "host_name": host, "address": address }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = ctx
        .patch_json(
            &format!("/inventory/ip-addresses/{address}"),
            json!({ "mac_address": "aa:bb:cc:dd:ee:ff" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["mac_address"]
            .as_str()
            .unwrap()
            .eq_ignore_ascii_case("aa:bb:cc:dd:ee:ff")
    );
}

async fn zone_create_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("zone-create");
    let nameserver = ctx.nameserver("ns1", &zone);
    let status = ctx
        .post("/dns/nameservers", json!({ "name": nameserver }))
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            "/dns/forward-zones",
            json!({
                "name": zone,
                "primary_ns": nameserver,
                "nameservers": [nameserver],
                "email": format!("hostmaster@{zone}"),
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    ctx.assert_audit_exists("nameserver", &nameserver, "create")
        .await;
    ctx.assert_audit_exists("forward_zone", &zone, "create")
        .await;
}

async fn zone_auto_ns_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("zone-ns");
    let nameserver = ctx.nameserver("ns1", &zone);
    ctx.seed_zone(&zone, &nameserver).await;

    let body = ctx
        .get_json(&format!("/dns/records?type_name=NS&owner_name={zone}"))
        .await;
    assert!(body["total"].as_u64().unwrap() >= 1);
}

async fn zone_update_soa_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("zone-soa");
    let nameserver = ctx.nameserver("ns1", &zone);
    ctx.seed_zone(&zone, &nameserver).await;

    let status = ctx
        .patch(
            &format!("/dns/forward-zones/{zone}"),
            json!({ "refresh": 7200 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

async fn delegation_creates_ns_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("zone-deleg");
    let nameserver = ctx.nameserver("ns1", &zone);
    let delegated_ns = ctx.nameserver("ns-d", &zone);
    let delegation = ctx.host_in_zone("deleg", &zone);
    ctx.seed_zone(&zone, &nameserver).await;

    let status = ctx
        .post("/dns/nameservers", json!({ "name": delegated_ns }))
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            &format!("/dns/forward-zones/{zone}/delegations"),
            json!({ "name": delegation, "comment": "", "nameservers": [delegated_ns] }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let body = ctx
        .get_json(&format!(
            "/dns/records?type_name=NS&owner_name={delegation}"
        ))
        .await;
    assert!(body["total"].as_u64().unwrap() >= 1);
}

async fn zone_not_found_scenario(ctx: &TestCtx) {
    let status = ctx
        .get_status(&format!("/dns/forward-zones/{}", ctx.zone("missing")))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn network_create_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(4);
    let status = ctx
        .post(
            "/inventory/networks",
            json!({ "cidr": cidr, "description": "test network" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn network_update_frozen_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(5);
    ctx.seed_network(&cidr).await;

    let (status, body) = ctx
        .patch_json(
            &format!("/inventory/networks/{cidr}"),
            json!({ "frozen": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["frozen"], true);
}

async fn network_allocation_rules_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(6);
    let excluded_start = ctx.ip_in_cidr(&cidr, 10);
    let excluded_end = ctx.ip_in_cidr(&cidr, 20);
    let good_ip = ctx.ip_in_cidr(&cidr, 50);
    let first_host = ctx.host("alloc-first");
    let second_host = ctx.host("alloc-second");
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&first_host).await;
    ctx.seed_host(&second_host).await;

    let status = ctx
        .post(
            "/inventory/networks/excluded-ranges",
            json!({
                "network": cidr,
                "start_ip": excluded_start,
                "end_ip": excluded_end,
                "description": "reserved",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({ "host_name": first_host, "address": ctx.ip_in_cidr(&cidr, 15) }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({ "host_name": first_host, "address": good_ip }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({ "host_name": second_host, "address": good_ip }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

async fn record_create_txt_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("record-create");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("txt", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&host, &zone).await;

    let status = ctx
        .post(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": host,
                "data": { "value": "hello" },
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn record_create_bumps_serial_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("record-serial");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("txt", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&host, &zone).await;

    let before = ctx.get_json(&format!("/dns/forward-zones/{zone}")).await;
    let status = ctx
        .post(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": host,
                "data": { "value": "x" },
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let after = ctx.get_json(&format!("/dns/forward-zones/{zone}")).await;

    assert!(after["serial_no"].as_u64().unwrap() > before["serial_no"].as_u64().unwrap());
}

async fn cname_exclusivity_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("cname");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("alias", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&host, &zone).await;

    let status = ctx
        .post(
            "/dns/records",
            json!({
                "type_name": "CNAME",
                "owner_kind": "host",
                "owner_name": host,
                "data": { "target": ctx.host_in_zone("target", &zone) },
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": host,
                "data": { "value": "should fail" },
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

async fn wildcard_record_scenario(ctx: &TestCtx) {
    let owner = format!("*.{}", ctx.zone("wild"));
    let (status, body) = ctx
        .post_json(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_name": owner,
                "data": { "value": "spf" },
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["owner_kind"].is_null());
}

async fn record_not_found_scenario(ctx: &TestCtx) {
    let status = ctx
        .get_status("/dns/records/00000000-0000-0000-0000-000000000000")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn policy_atom_create_scenario(ctx: &TestCtx) {
    let name = ctx.name("atom");
    let status = ctx
        .post(
            "/policy/host/atoms",
            json!({ "name": name, "description": "test" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    ctx.assert_audit_exists("host_policy_atom", &name, "create")
        .await;
}

async fn policy_role_with_atom_scenario(ctx: &TestCtx) {
    let atom = ctx.name("atom");
    let role = ctx.name("role");

    let status = ctx
        .post(
            "/policy/host/atoms",
            json!({ "name": atom, "description": "atom" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let status = ctx
        .post(
            "/policy/host/roles",
            json!({ "name": role, "description": "role" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            &format!("/policy/host/roles/{role}/atoms/{atom}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let body = ctx.get_json(&format!("/policy/host/roles/{role}")).await;
    assert!(
        body["atoms"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == &atom)
    );
}

async fn policy_atom_in_use_reject_delete_scenario(ctx: &TestCtx) {
    let atom = ctx.name("used-atom");
    let role = ctx.name("used-role");

    let status = ctx
        .post(
            "/policy/host/atoms",
            json!({ "name": atom, "description": "atom" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let status = ctx
        .post(
            "/policy/host/roles",
            json!({ "name": role, "description": "role" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let status = ctx
        .post(
            &format!("/policy/host/roles/{role}/atoms/{atom}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let status = ctx.delete(&format!("/policy/host/atoms/{atom}")).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

async fn import_batch_is_atomic_scenario(ctx: &TestCtx) {
    let _guard = task_queue_mutex().lock().await;
    drain_task_queue(ctx).await;

    let cidr = ctx.cidr(7);
    let import_zone = ctx.zone("missing-zone");
    let import_host = ctx.host_in_zone("bad-host", &import_zone);
    let (_, created) = ctx
        .post_json(
            "/workflows/imports",
            json!({
                "requested_by": "tester",
                "items": [
                    {
                        "ref": "network-1",
                        "kind": "network",
                        "operation": "create",
                        "attributes": {
                            "cidr": cidr,
                            "description": "Imported network",
                        }
                    },
                    {
                        "ref": "bad-host",
                        "kind": "host",
                        "operation": "create",
                        "attributes": {
                            "name": import_host,
                            "zone": import_zone,
                        }
                    }
                ]
            }),
        )
        .await;
    let import_id = created["id"].as_str().unwrap().to_string();

    let status = ctx.post("/workflows/tasks/run-next", json!({})).await;
    assert!(
        matches!(
            status,
            StatusCode::OK | StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
        ),
        "unexpected import failure status: {status}"
    );

    let status = ctx.get_status(&format!("/inventory/networks/{cidr}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let imports = ctx.get_json("/workflows/imports").await;
    let stored = imports["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == import_id)
        .expect("import batch should exist");
    assert_eq!(stored["status"], "failed");
}

async fn task_claiming_advances_state_scenario(ctx: &TestCtx) {
    let _guard = task_queue_mutex().lock().await;
    drain_task_queue(ctx).await;

    let storage = ctx.storage();
    let first = storage
        .tasks()
        .create_task(
            CreateTask::new(
                "import_batch",
                Some("tester".to_string()),
                json!({ "import_id": "00000000-0000-0000-0000-000000000001" }),
                Some(ctx.name("task-1")),
                1,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let second = storage
        .tasks()
        .create_task(
            CreateTask::new(
                "export_run",
                Some("tester".to_string()),
                json!({ "run_id": "00000000-0000-0000-0000-000000000002" }),
                Some(ctx.name("task-2")),
                1,
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let claimed_one = storage
        .tasks()
        .claim_next_task()
        .await
        .unwrap()
        .expect("first task");
    let claimed_two = storage
        .tasks()
        .claim_next_task()
        .await
        .unwrap()
        .expect("second task");

    assert_eq!(claimed_one.status(), &TaskStatus::Running);
    assert_eq!(claimed_two.status(), &TaskStatus::Running);
    assert_eq!(claimed_one.attempts(), 1);
    assert_eq!(claimed_two.attempts(), 1);
    assert_ne!(claimed_one.id(), claimed_two.id());
    assert!(
        [first.id(), second.id()].contains(&claimed_one.id())
            && [first.id(), second.id()].contains(&claimed_two.id())
    );
}

// --- Part 1: Delete operation scenarios ---

async fn host_delete_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("host-del");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("doomed", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&host, &zone).await;

    let status = ctx.delete(&format!("/inventory/hosts/{host}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let status = ctx.get_status(&format!("/inventory/hosts/{host}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    ctx.assert_audit_exists("host", &host, "delete").await;
}

async fn zone_delete_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("zone-del");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("txt", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_host_in_zone(&host, &zone).await;

    let status = ctx
        .post(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": host,
                "data": { "value": "ephemeral" },
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx.delete(&format!("/dns/forward-zones/{zone}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let status = ctx.get_status(&format!("/dns/forward-zones/{zone}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn network_delete_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(9);
    ctx.seed_network(&cidr).await;

    let encoded_cidr = cidr.replace("/", "%2F");
    let status = ctx
        .delete(&format!("/inventory/networks/{encoded_cidr}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let status = ctx
        .get_status(&format!("/inventory/networks/{encoded_cidr}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn nameserver_delete_scenario(ctx: &TestCtx) {
    let ns = ctx.nameserver("standalone", &ctx.zone("ns-del"));
    let status = ctx.post("/dns/nameservers", json!({ "name": ns })).await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx.delete(&format!("/dns/nameservers/{ns}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let status = ctx.get_status(&format!("/dns/nameservers/{ns}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// --- Part 2: Host policy membership scenarios ---

async fn policy_role_add_host_scenario(ctx: &TestCtx) {
    let host = ctx.host("pol-host");
    let role = ctx.name("host-role");
    ctx.seed_host(&host).await;

    let status = ctx
        .post(
            "/policy/host/roles",
            json!({ "name": role, "description": "role with host" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            &format!("/policy/host/roles/{role}/hosts/{host}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let body = ctx.get_json(&format!("/policy/host/roles/{role}")).await;
    assert!(
        body["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == &host),
        "role should contain the host"
    );

    let status = ctx
        .delete(&format!("/policy/host/roles/{role}/hosts/{host}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let body = ctx.get_json(&format!("/policy/host/roles/{role}")).await;
    assert!(
        body["hosts"].as_array().unwrap().is_empty(),
        "role hosts should be empty after removal"
    );
}

async fn policy_role_add_label_scenario(ctx: &TestCtx) {
    let label = ctx.name("pol-label");
    let role = ctx.name("label-role");

    let status = ctx
        .post(
            "/inventory/labels",
            json!({ "name": label, "description": "test label" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            "/policy/host/roles",
            json!({ "name": role, "description": "role with label" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let status = ctx
        .post(
            &format!("/policy/host/roles/{role}/labels/{label}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let body = ctx.get_json(&format!("/policy/host/roles/{role}")).await;
    assert!(
        body["labels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == &label),
        "role should contain the label"
    );

    let status = ctx
        .delete(&format!("/policy/host/roles/{role}/labels/{label}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let body = ctx.get_json(&format!("/policy/host/roles/{role}")).await;
    assert!(
        body["labels"].as_array().unwrap().is_empty(),
        "role labels should be empty after removal"
    );
}

dual_backend_test!(label_create, |ctx| {
    label_create_scenario(&ctx).await;
});

dual_backend_test!(label_get, |ctx| {
    label_get_scenario(&ctx).await;
});

dual_backend_test!(label_update, |ctx| {
    label_update_scenario(&ctx).await;
});

dual_backend_test!(label_delete, |ctx| {
    label_delete_scenario(&ctx).await;
});

dual_backend_test!(label_not_found, |ctx| {
    label_not_found_scenario(&ctx).await;
});

dual_backend_test!(label_duplicate, |ctx| {
    label_duplicate_scenario(&ctx).await;
});

dual_backend_test!(label_pagination_walks_without_duplicates, |ctx| {
    label_pagination_walks_without_duplicates_scenario(&ctx).await;
});

dual_backend_test!(host_create, |ctx| {
    host_create_scenario(&ctx).await;
});

dual_backend_test!(host_get, |ctx| {
    host_get_scenario(&ctx).await;
});

dual_backend_test!(host_rename, |ctx| {
    host_rename_scenario(&ctx).await;
});

dual_backend_test!(host_delete_cleans_records, |ctx| {
    host_delete_cleans_records_scenario(&ctx).await;
});

dual_backend_test!(host_rename_bumps_serial, |ctx| {
    host_rename_bumps_serial_scenario(&ctx).await;
});

dual_backend_test!(host_zone_mismatch, |ctx| {
    host_zone_mismatch_scenario(&ctx).await;
});

dual_backend_test!(host_not_found, |ctx| {
    host_not_found_scenario(&ctx).await;
});

dual_backend_test!(ip_assign, |ctx| {
    ip_assign_scenario(&ctx).await;
});

dual_backend_test!(ip_creates_a_record, |ctx| {
    ip_creates_a_record_scenario(&ctx).await;
});

dual_backend_test!(ip_unassign, |ctx| {
    ip_unassign_scenario(&ctx).await;
});

dual_backend_test!(ip_patch_mac, |ctx| {
    ip_patch_mac_scenario(&ctx).await;
});

dual_backend_test!(zone_create, |ctx| {
    zone_create_scenario(&ctx).await;
});

dual_backend_test!(zone_auto_ns, |ctx| {
    zone_auto_ns_scenario(&ctx).await;
});

dual_backend_test!(zone_update_soa, |ctx| {
    zone_update_soa_scenario(&ctx).await;
});

dual_backend_test!(delegation_creates_ns, |ctx| {
    delegation_creates_ns_scenario(&ctx).await;
});

dual_backend_test!(zone_not_found, |ctx| {
    zone_not_found_scenario(&ctx).await;
});

dual_backend_test!(network_create, |ctx| {
    network_create_scenario(&ctx).await;
});

dual_backend_test!(network_update_frozen, |ctx| {
    network_update_frozen_scenario(&ctx).await;
});

dual_backend_test!(network_allocation_rules_hold, |ctx| {
    network_allocation_rules_scenario(&ctx).await;
});

dual_backend_test!(record_create_txt, |ctx| {
    record_create_txt_scenario(&ctx).await;
});

dual_backend_test!(record_create_bumps_serial, |ctx| {
    record_create_bumps_serial_scenario(&ctx).await;
});

dual_backend_test!(cname_exclusivity, |ctx| {
    cname_exclusivity_scenario(&ctx).await;
});

dual_backend_test!(wildcard_record, |ctx| {
    wildcard_record_scenario(&ctx).await;
});

dual_backend_test!(record_not_found, |ctx| {
    record_not_found_scenario(&ctx).await;
});

dual_backend_test!(policy_atom_create, |ctx| {
    policy_atom_create_scenario(&ctx).await;
});

dual_backend_test!(policy_role_with_atom, |ctx| {
    policy_role_with_atom_scenario(&ctx).await;
});

dual_backend_test!(policy_atom_in_use_reject_delete, |ctx| {
    policy_atom_in_use_reject_delete_scenario(&ctx).await;
});

dual_backend_test!(import_batch_is_atomic, |ctx| {
    import_batch_is_atomic_scenario(&ctx).await;
});

dual_backend_test!(task_claiming_advances_state, |ctx| {
    task_claiming_advances_state_scenario(&ctx).await;
});

dual_backend_test!(host_delete, |ctx| {
    host_delete_scenario(&ctx).await;
});

dual_backend_test!(zone_delete, |ctx| {
    zone_delete_scenario(&ctx).await;
});

dual_backend_test!(network_delete, |ctx| {
    network_delete_scenario(&ctx).await;
});

dual_backend_test!(nameserver_delete, |ctx| {
    nameserver_delete_scenario(&ctx).await;
});

dual_backend_test!(policy_role_add_host, |ctx| {
    policy_role_add_host_scenario(&ctx).await;
});

dual_backend_test!(policy_role_add_label, |ctx| {
    policy_role_add_label_scenario(&ctx).await;
});

async fn network_excluded_range_crud_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(8);
    let start_ip = ctx.ip_in_cidr(&cidr, 10);
    let end_ip = ctx.ip_in_cidr(&cidr, 20);
    ctx.seed_network(&cidr).await;

    // List excluded ranges — should be empty
    let encoded_cidr = cidr.replace("/", "%2F");
    let list_uri = format!("/inventory/networks/{encoded_cidr}/excluded-ranges");
    let body = ctx.get_json(&list_uri).await;
    assert_eq!(body["total"], 0);

    // Create an excluded range
    let (status, created) = ctx
        .post_json(
            "/inventory/networks/excluded-ranges",
            json!({
                "network": cidr,
                "start_ip": start_ip,
                "end_ip": end_ip,
                "description": "reserved block",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["start_ip"], start_ip);
    assert_eq!(created["end_ip"], end_ip);
    assert_eq!(created["description"], "reserved block");
    assert!(
        created["id"].is_string(),
        "excluded range should have an id"
    );

    // List excluded ranges — should now contain one
    let body = ctx.get_json(&list_uri).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["start_ip"], start_ip);
    assert_eq!(body["items"][0]["end_ip"], end_ip);
}

dual_backend_test!(network_excluded_range_crud, |ctx| {
    network_excluded_range_crud_scenario(&ctx).await;
});

// ---------------------------------------------------------------------------
// Ancillary entity and attachment scenarios
// ---------------------------------------------------------------------------

async fn host_contact_update_and_delete_scenario(ctx: &TestCtx) {
    // Seed a host that the contact will reference.
    let host = ctx.host("contact-host");
    ctx.seed_host(&host).await;

    let email = format!("ops-{}@example.org", ctx.namespace());

    // POST a host contact.
    let (status, body) = ctx
        .post_json(
            "/inventory/host-contacts",
            json!({
                "email": email,
                "display_name": "Ops Team",
                "hosts": [host],
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["email"], email);
    assert_eq!(body["display_name"], "Ops Team");

    // Verify GET returns the contact.
    let body = ctx
        .get_json(&format!("/inventory/host-contacts/{email}"))
        .await;
    assert_eq!(body["email"], email);
    assert_eq!(body["display_name"], "Ops Team");

    // DELETE the contact.
    let status = ctx
        .delete(&format!("/inventory/host-contacts/{email}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify GET now returns 404.
    let status = ctx
        .get_status(&format!("/inventory/host-contacts/{email}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn host_group_create_and_delete_scenario(ctx: &TestCtx) {
    let name = ctx.name("hgroup");

    // POST a host group.
    let (status, body) = ctx
        .post_json(
            "/inventory/host-groups",
            json!({
                "name": name,
                "description": "test host group",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["name"], name);

    // Verify GET returns the group.
    let body = ctx
        .get_json(&format!("/inventory/host-groups/{name}"))
        .await;
    assert_eq!(body["name"], name);
    assert_eq!(body["description"], "test host group");

    // DELETE the group.
    let status = ctx.delete(&format!("/inventory/host-groups/{name}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify GET now returns 404.
    let status = ctx
        .get_status(&format!("/inventory/host-groups/{name}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn network_policy_update_and_delete_scenario(ctx: &TestCtx) {
    let name = ctx.name("netpol");

    // POST a network policy.
    let (status, body) = ctx
        .post_json(
            "/policy/network/policies",
            json!({
                "name": name,
                "description": "initial description",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["name"], name);
    assert_eq!(body["description"], "initial description");

    // Verify GET returns the policy.
    let body = ctx
        .get_json(&format!("/policy/network/policies/{name}"))
        .await;
    assert_eq!(body["name"], name);
    assert_eq!(body["description"], "initial description");

    // DELETE the policy.
    let status = ctx
        .delete(&format!("/policy/network/policies/{name}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify GET now returns 404.
    let status = ctx
        .get_status(&format!("/policy/network/policies/{name}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn community_create_and_delete_scenario(ctx: &TestCtx) {
    let policy_name = ctx.name("compol");
    let cidr = ctx.cidr(12);
    let community_name = ctx.name("comm");

    // Seed: create network and policy.
    ctx.seed_network(&cidr).await;
    let status = ctx
        .post(
            "/policy/network/policies",
            json!({
                "name": policy_name,
                "description": "community policy",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // POST a community.
    let (status, body) = ctx
        .post_json(
            "/policy/network/communities",
            json!({
                "policy_name": policy_name,
                "network": cidr,
                "name": community_name,
                "description": "test community",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let community_id = body["id"].as_str().unwrap().to_string();

    // Verify GET returns the community.
    let body = ctx
        .get_json(&format!("/policy/network/communities/{community_id}"))
        .await;
    assert_eq!(body["name"], community_name);

    // DELETE the community.
    let status = ctx
        .delete(&format!("/policy/network/communities/{community_id}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify GET now returns 404.
    let status = ctx
        .get_status(&format!("/policy/network/communities/{community_id}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn attachment_update_and_delete_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(13);
    let host = ctx.host("attach");

    // Seed host and network.
    ctx.seed_host(&host).await;
    ctx.seed_network(&cidr).await;

    // POST an attachment.
    let (status, body) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({
                "network": cidr,
                "comment": "initial comment",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let attachment_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["comment"], "initial comment");

    // Duplicate POST must remain a strict create, not an idempotent upsert.
    let status = ctx
        .post(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({
                "network": cidr,
                "comment": "second comment",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // PATCH the attachment (update comment).
    let (status, body) = ctx
        .patch_json(
            &format!("/inventory/attachments/{attachment_id}"),
            json!({ "comment": "updated comment" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["comment"], "updated comment");

    // Verify the change persisted via GET.
    let body = ctx
        .get_json(&format!("/inventory/attachments/{attachment_id}"))
        .await;
    assert_eq!(body["comment"], "updated comment");

    // PATCH the attachment (set mac_address).
    let (status, body) = ctx
        .patch_json(
            &format!("/inventory/attachments/{attachment_id}"),
            json!({ "mac_address": "aa:bb:cc:dd:ee:ff" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["mac_address"]
            .as_str()
            .unwrap()
            .eq_ignore_ascii_case("aa:bb:cc:dd:ee:ff")
    );

    // DELETE the attachment.
    let status = ctx
        .delete(&format!("/inventory/attachments/{attachment_id}"))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify GET now returns 404.
    let status = ctx
        .get_status(&format!("/inventory/attachments/{attachment_id}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

dual_backend_test!(host_contact_update_and_delete, |ctx| {
    host_contact_update_and_delete_scenario(&ctx).await;
});

dual_backend_test!(host_group_create_and_delete, |ctx| {
    host_group_create_and_delete_scenario(&ctx).await;
});

dual_backend_test!(network_policy_update_and_delete, |ctx| {
    network_policy_update_and_delete_scenario(&ctx).await;
});

dual_backend_test!(community_create_and_delete, |ctx| {
    community_create_and_delete_scenario(&ctx).await;
});

dual_backend_test!(attachment_update_and_delete, |ctx| {
    attachment_update_and_delete_scenario(&ctx).await;
});

// ---------------------------------------------------------------------------
// Host creation with IP assignment scenarios
// ---------------------------------------------------------------------------

async fn host_create_with_explicit_ip_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("hip-explicit");
    let nameserver = ctx.nameserver("ns1", &zone);
    let cidr = ctx.cidr(14);
    let host = ctx.host_in_zone("explicit", &zone);
    let address = ctx.ip_in_cidr(&cidr, 42);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_network(&cidr).await;

    let (status, _body) = ctx
        .post_json(
            "/inventory/hosts",
            json!({
                "name": host,
                "zone": zone,
                "comment": "explicit ip",
                "ip_addresses": [{ "address": address }],
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // Verify IP was assigned
    let body = ctx
        .get_json(&format!("/inventory/hosts/{host}/ip-addresses"))
        .await;
    let items = body["items"].as_array().expect("ip list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["address"], address);
}

async fn host_create_with_first_free_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("hip-ff");
    let nameserver = ctx.nameserver("ns1", &zone);
    let cidr = ctx.cidr(15);
    let host = ctx.host_in_zone("firstfree", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_network(&cidr).await;

    let (status, _body) = ctx
        .post_json(
            "/inventory/hosts",
            json!({
                "name": host,
                "zone": zone,
                "comment": "first free",
                "ip_addresses": [{ "network": cidr }],
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // Verify an IP was assigned within the network
    let body = ctx
        .get_json(&format!("/inventory/hosts/{host}/ip-addresses"))
        .await;
    let items = body["items"].as_array().expect("ip list");
    assert_eq!(items.len(), 1);
    let assigned_addr = items[0]["address"].as_str().expect("address string");
    // The address should start with the network prefix
    let network_prefix = cidr.strip_suffix("/24").expect("expected /24");
    let net_parts: Vec<&str> = network_prefix.split('.').collect();
    let addr_parts: Vec<&str> = assigned_addr.split('.').collect();
    assert_eq!(addr_parts[0], net_parts[0]);
    assert_eq!(addr_parts[1], net_parts[1]);
    assert_eq!(addr_parts[2], net_parts[2]);
}

async fn host_create_with_random_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("hip-rnd");
    let nameserver = ctx.nameserver("ns1", &zone);
    let cidr = ctx.cidr(15);
    let host = ctx.host_in_zone("random", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_network(&cidr).await;

    let (status, _body) = ctx
        .post_json(
            "/inventory/hosts",
            json!({
                "name": host,
                "zone": zone,
                "comment": "random alloc",
                "ip_addresses": [{ "network": cidr, "allocation": "random" }],
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // Verify an IP was assigned within the network
    let body = ctx
        .get_json(&format!("/inventory/hosts/{host}/ip-addresses"))
        .await;
    let items = body["items"].as_array().expect("ip list");
    assert_eq!(items.len(), 1);
    let assigned_addr = items[0]["address"].as_str().expect("address string");
    let network_prefix = cidr.strip_suffix("/24").expect("expected /24");
    let net_parts: Vec<&str> = network_prefix.split('.').collect();
    let addr_parts: Vec<&str> = assigned_addr.split('.').collect();
    assert_eq!(addr_parts[0], net_parts[0]);
    assert_eq!(addr_parts[1], net_parts[1]);
    assert_eq!(addr_parts[2], net_parts[2]);
}

async fn host_create_with_multiple_ips_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("hip-multi");
    let nameserver = ctx.nameserver("ns1", &zone);
    let cidr1 = ctx.cidr(14);
    let cidr2 = ctx.cidr(15);
    let host = ctx.host_in_zone("multi", &zone);
    ctx.seed_zone(&zone, &nameserver).await;
    ctx.seed_network(&cidr1).await;
    ctx.seed_network(&cidr2).await;

    let (status, _body) = ctx
        .post_json(
            "/inventory/hosts",
            json!({
                "name": host,
                "zone": zone,
                "comment": "multi ip",
                "ip_addresses": [
                    { "network": cidr1 },
                    { "network": cidr2 },
                ],
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let body = ctx
        .get_json(&format!("/inventory/hosts/{host}/ip-addresses"))
        .await;
    let items = body["items"].as_array().expect("ip list");
    assert_eq!(items.len(), 2);
}

async fn host_create_with_bad_ip_rolls_back_scenario(ctx: &TestCtx) {
    let zone = ctx.zone("hip-bad");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("badip", &zone);
    ctx.seed_zone(&zone, &nameserver).await;

    let (status, _body) = ctx
        .post_json(
            "/inventory/hosts",
            json!({
                "name": host,
                "zone": zone,
                "comment": "should rollback",
                "ip_addresses": [{ "address": "999.999.999.999" }],
            }),
        )
        .await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::INTERNAL_SERVER_ERROR,
        "expected error status for invalid IP, got {status}"
    );

    // Host should not exist (rolled back)
    let status = ctx.get_status(&format!("/inventory/hosts/{host}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

dual_backend_test!(host_create_with_explicit_ip, |ctx| {
    host_create_with_explicit_ip_scenario(&ctx).await;
});

dual_backend_test!(host_create_with_first_free, |ctx| {
    host_create_with_first_free_scenario(&ctx).await;
});

dual_backend_test!(host_create_with_random, |ctx| {
    host_create_with_random_scenario(&ctx).await;
});

dual_backend_test!(host_create_with_multiple_ips, |ctx| {
    host_create_with_multiple_ips_scenario(&ctx).await;
});

dual_backend_test!(host_create_with_bad_ip_rolls_back, |ctx| {
    host_create_with_bad_ip_rolls_back_scenario(&ctx).await;
});

// ---------------------------------------------------------------------------
// Auto-DHCP identifier scenarios
// ---------------------------------------------------------------------------

async fn ip_assign_auto_creates_v4_client_id_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(0);
    let host = ctx.host("dhcp4");
    let address = ctx.ip_in_cidr(&cidr, 10);
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({
                "host_name": host,
                "address": address,
                "mac_address": "aa:bb:cc:dd:ee:01"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // GET host detail to check attachment DHCP identifiers
    let body = ctx.get_json(&format!("/inventory/hosts/{host}")).await;
    let attachments = body["attachments"].as_array().expect("attachments list");
    assert!(!attachments.is_empty(), "host should have attachments");
    let dhcp_ids = attachments[0]["dhcp_identifiers"]
        .as_array()
        .expect("dhcp_identifiers list");
    let v4_id = dhcp_ids.iter().find(|id| id["family"] == 4);
    assert!(
        v4_id.is_some(),
        "auto-created client_id should exist for v4"
    );
    let v4_id = v4_id.unwrap();
    assert_eq!(v4_id["kind"], "client_id");
    assert_eq!(v4_id["value"], "01:aa:bb:cc:dd:ee:01");
    assert_eq!(v4_id["priority"], 1000);
}

async fn ip_assign_auto_creates_v6_duid_ll_scenario(ctx: &TestCtx) {
    // Use the IPv4 CIDR slot to derive a unique /120 IPv6 prefix.
    // The cidr(3) slot gives us a unique 10.X.Y.0/24 — we reuse
    // X and Y as the IPv6 group to avoid collisions.
    let v4_cidr = ctx.cidr(3);
    let prefix = v4_cidr.strip_suffix("/24").expect("expected /24");
    let octets: Vec<&str> = prefix.split('.').collect();
    let group = format!(
        "{:x}{:02x}",
        octets[1].parse::<u16>().unwrap(),
        octets[2].parse::<u16>().unwrap()
    );
    let cidr = format!("fd00:{group}::/120");
    let address = format!("fd00:{group}::10");
    let host = ctx.host("dhcp6");
    ctx.seed_host(&host).await;

    // Seed an IPv6 network
    let (status, body) = ctx
        .post_json(
            "/inventory/networks",
            json!({
                "cidr": cidr,
                "description": format!("ipv6 network {cidr}"),
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "creating IPv6 network failed: {body}"
    );

    let (status, body) = ctx
        .post_json(
            "/inventory/ip-addresses",
            json!({
                "host_name": host,
                "address": address,
                "mac_address": "aa:bb:cc:dd:ee:02"
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "assigning IPv6 address failed: {body}"
    );

    // GET host detail to check attachment DHCP identifiers
    let body = ctx.get_json(&format!("/inventory/hosts/{host}")).await;
    let attachments = body["attachments"].as_array().expect("attachments list");
    assert!(!attachments.is_empty(), "host should have attachments");
    let dhcp_ids = attachments[0]["dhcp_identifiers"]
        .as_array()
        .expect("dhcp_identifiers list");
    let v6_id = dhcp_ids.iter().find(|id| id["family"] == 6);
    assert!(v6_id.is_some(), "auto-created duid_ll should exist for v6");
    let v6_id = v6_id.unwrap();
    assert_eq!(v6_id["kind"], "duid_ll");
    assert_eq!(v6_id["value"], "00:03:00:01:aa:bb:cc:dd:ee:02");
    assert_eq!(v6_id["priority"], 1000);
}

async fn ip_assign_no_duplicate_dhcp_identifier_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(1);
    let host = ctx.host("dhcp-nodup");
    let address1 = ctx.ip_in_cidr(&cidr, 10);
    let address2 = ctx.ip_in_cidr(&cidr, 11);
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    // Assign first IP with MAC
    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({
                "host_name": host,
                "address": address1,
                "mac_address": "aa:bb:cc:dd:ee:03"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // Assign second IP with same MAC (same attachment)
    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({
                "host_name": host,
                "address": address2,
                "mac_address": "aa:bb:cc:dd:ee:03"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // GET host detail — should have exactly one v4 DHCP identifier, not two
    let body = ctx.get_json(&format!("/inventory/hosts/{host}")).await;
    let attachments = body["attachments"].as_array().expect("attachments list");
    // Both IPs are on the same attachment (same host+network+mac)
    let all_v4_ids: Vec<&serde_json::Value> = attachments
        .iter()
        .flat_map(|a| {
            a["dhcp_identifiers"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|id| id["family"] == 4)
        })
        .collect();
    assert_eq!(
        all_v4_ids.len(),
        1,
        "should have exactly one v4 DHCP identifier, not {}",
        all_v4_ids.len()
    );
}

async fn ip_assign_no_auto_dhcp_without_mac_scenario(ctx: &TestCtx) {
    let cidr = ctx.cidr(2);
    let host = ctx.host("dhcp-nomac");
    let address = ctx.ip_in_cidr(&cidr, 10);
    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    // Assign IP without MAC
    let status = ctx
        .post(
            "/inventory/ip-addresses",
            json!({
                "host_name": host,
                "address": address
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // GET host detail — should have no DHCP identifiers
    let body = ctx.get_json(&format!("/inventory/hosts/{host}")).await;
    let attachments = body["attachments"].as_array().expect("attachments list");
    let all_ids: Vec<&serde_json::Value> = attachments
        .iter()
        .flat_map(|a| a["dhcp_identifiers"].as_array().unwrap().iter())
        .collect();
    assert!(
        all_ids.is_empty(),
        "should have no DHCP identifiers without MAC"
    );
}

dual_backend_test_auto_dhcp!(ip_assign_auto_creates_v4_client_id, |ctx| {
    ip_assign_auto_creates_v4_client_id_scenario(&ctx).await;
});

dual_backend_test_auto_dhcp!(ip_assign_auto_creates_v6_duid_ll, |ctx| {
    ip_assign_auto_creates_v6_duid_ll_scenario(&ctx).await;
});

dual_backend_test_auto_dhcp!(ip_assign_no_duplicate_dhcp_identifier, |ctx| {
    ip_assign_no_duplicate_dhcp_identifier_scenario(&ctx).await;
});

dual_backend_test_auto_dhcp!(ip_assign_no_auto_dhcp_without_mac, |ctx| {
    ip_assign_no_auto_dhcp_without_mac_scenario(&ctx).await;
});
