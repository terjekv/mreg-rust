use actix_web::http::StatusCode;
use serde_json::json;

use crate::fixtures::{ADMIN_GROUP, ADMIN_USER, treetop_ctx};

#[actix_web::test]
async fn treetop_worker_can_run_imports_but_denied_exports_fail_the_task() {
    let Some(ctx) = treetop_ctx() else {
        eprintln!("skipping: MREG_TEST_TREETOP_URL not set");
        return;
    };

    let import_cidr = ctx.cidr(60);
    let (import_status, import_body) = ctx
        .post_json_as(
            "/workflows/imports",
            json!({
                "requested_by": ADMIN_USER,
                "items": [
                    {
                        "ref": ctx.name("import-network"),
                        "kind": "network",
                        "operation": "create",
                        "attributes": {
                            "cidr": import_cidr,
                            "description": "imported via worker"
                        }
                    }
                ]
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await;
    assert_eq!(import_status, StatusCode::CREATED);
    assert_eq!(
        ctx.post_as(
            "/workflows/tasks/run-next",
            json!({}),
            "worker-import",
            &["mreg-workers"],
        )
        .await,
        StatusCode::OK
    );

    let import_task_id = import_body["task_id"]
        .as_str()
        .expect("import task id")
        .to_string();
    let (tasks_status, tasks_body) = ctx
        .get_json_as("/workflows/tasks", "worker-import", &["mreg-workers"])
        .await;
    assert_eq!(tasks_status, StatusCode::OK);
    let import_task = tasks_body["items"]
        .as_array()
        .expect("task items")
        .iter()
        .find(|item| item["id"] == import_task_id)
        .expect("import task present");
    assert_eq!(import_task["status"], "succeeded");

    let template_name = ctx.name("net-summary");
    assert_eq!(
        ctx.post_as(
            "/workflows/export-templates",
            json!({
                "name": template_name,
                "description": "network summary",
                "engine": "minijinja",
                "scope": "inventory",
                "body": "{% for network in networks %}{{ network.cidr }}{% endfor %}",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await,
        StatusCode::CREATED
    );

    let (run_status, run_body) = ctx
        .post_json_as(
            "/workflows/export-runs",
            json!({
                "template_name": template_name,
                "requested_by": ADMIN_USER,
                "scope": "inventory",
            }),
            ADMIN_USER,
            &[ADMIN_GROUP],
        )
        .await;
    assert_eq!(run_status, StatusCode::CREATED);

    assert_eq!(
        ctx.post_as(
            "/workflows/tasks/run-next",
            json!({}),
            "worker-export",
            &["mreg-workers"],
        )
        .await,
        StatusCode::FORBIDDEN
    );

    let export_task_id = run_body["task_id"]
        .as_str()
        .expect("export task id")
        .to_string();
    let (admin_tasks_status, admin_tasks_body) = ctx
        .get_json_as("/workflows/tasks", ADMIN_USER, &[ADMIN_GROUP])
        .await;
    assert_eq!(admin_tasks_status, StatusCode::OK);
    let export_task = admin_tasks_body["items"]
        .as_array()
        .expect("task items")
        .iter()
        .find(|item| item["id"] == export_task_id)
        .expect("export task present");
    assert_eq!(export_task["status"], "failed");
    assert!(
        export_task["error_summary"]
            .as_str()
            .expect("error summary")
            .contains("permission denied")
    );
}
