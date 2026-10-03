mod common;

use std::time::Duration;

use actix_web::http::StatusCode;
use common::{TestBackend, TestCtx};
use diesel::{
    Connection, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{BigInt, Uuid as SqlUuid},
};
use mreg_rust::{
    domain::{
        imports::CreateImportBatch,
        network::CreateNetwork,
        network_policy::{CreateNetworkPolicy, CreateNetworkPolicyAttribute, UpdateNetworkPolicy},
        pagination::PageRequest,
        types::{
            CidrValue, CommunityLimit, CommunityTemplatePattern, NetworkPolicyAttributeName,
            NetworkPolicyName, RequiredDescription, ReservedCount, UpdateField,
        },
    },
    errors::AppError,
};
use rstest::rstest;
use serde_json::{Value, json};

async fn context(backend: TestBackend) -> Option<TestCtx> {
    match backend {
        TestBackend::Memory => Some(TestCtx::memory()),
        TestBackend::Postgres => {
            let ctx = TestCtx::postgres().await;
            if ctx.is_none() {
                eprintln!(
                    "{}",
                    common::postgres_skip_message("network policy regressions")
                );
            }
            ctx
        }
    }
}

async fn import(ctx: &TestCtx, items: Value) -> Result<(), AppError> {
    let storage = ctx.storage();
    let batch = storage
        .imports()
        .create_import_batch(CreateImportBatch::new(
            serde_json::from_value(json!({"items": items})).unwrap(),
            None,
        ))
        .await?;
    storage.imports().run_import_batch(batch.id()).await?;
    Ok(())
}

#[rstest]
#[case("")]
#[case("bad name")]
#[case(&"a".repeat(101))]
#[actix_web::test]
async fn invalid_attribute_import_rolls_back_without_breaking_listing(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[case] invalid: &str,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let valid = NetworkPolicyAttributeName::new(ctx.name("rollback")).unwrap();
    let result = import(
        &ctx,
        json!([
            {"ref":"valid", "kind":"network_policy_attribute", "operation":"create",
             "attributes":{"name":valid.as_str(), "description":"Roll back"}},
            {"ref":"invalid", "kind":"network_policy_attribute", "operation":"create",
             "attributes":{"name":invalid, "description":"Invalid"}}
        ]),
    )
    .await;
    let storage = ctx.storage();
    assert_eq!(
        (
            matches!(result, Err(AppError::Validation(_))),
            matches!(
                storage
                    .network_policies()
                    .get_network_policy_attribute_by_name(&valid)
                    .await,
                Err(AppError::NotFound(_))
            ),
            storage
                .network_policies()
                .list_network_policy_attributes(&PageRequest::all())
                .await
                .is_ok(),
        ),
        (true, true, true),
    );
}

dual_backend_test!(
    attribute_import_normalizes_definitions_and_memberships,
    |ctx| {
        let policy = ctx.name("import-policy");
        let first = ctx.name("first");
        let second = ctx.name("second");
        import(&ctx, json!([
        {"ref":"policy", "kind":"network_policy", "operation":"create",
         "attributes":{"name":policy, "description":"Policy"}},
        {"ref":"first", "kind":"network_policy_attribute", "operation":"create",
         "attributes":{"name":format!(" {} ", first.to_uppercase()), "description":"First"}},
        {"ref":"second", "kind":"network_policy_attribute", "operation":"create",
         "attributes":{"name":second, "description":"Second"}},
        {"ref":"first-value", "kind":"network_policy_attribute_value", "operation":"create",
         "attributes":{"policy_name_ref":"policy", "attribute_name_ref":"first", "value":false}},
        {"ref":"second-value", "kind":"network_policy_attribute_value", "operation":"create",
         "attributes":{"policy_name":format!(" {} ", policy.to_uppercase()),
                       "attribute_name":format!(" {} ", second.to_uppercase()), "value":true}}
    ])).await.unwrap();
        let values = ctx
            .storage()
            .network_policies()
            .list_network_policy_attribute_values(&NetworkPolicyName::new(&policy).unwrap())
            .await
            .unwrap();
        assert_eq!(
            values
                .iter()
                .map(|value| (value.name().as_str(), value.value()))
                .collect::<Vec<_>>(),
            vec![(first.as_str(), false), (second.as_str(), true)],
        );
    }
);

dual_backend_test!(attribute_import_rejects_normalized_duplicates, |ctx| {
    let name = ctx.name("duplicate");
    let result = import(
        &ctx,
        json!([
            {"ref":"first", "kind":"network_policy_attribute", "operation":"create",
             "attributes":{"name":name, "description":"First"}},
            {"ref":"duplicate", "kind":"network_policy_attribute", "operation":"create",
             "attributes":{"name":format!(" {} ", name.to_uppercase()), "description":"Duplicate"}}
        ]),
    )
    .await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
});

dual_backend_test!(imported_attribute_is_accessible_by_canonical_name, |ctx| {
    let name = ctx.name("canonical");
    import(
        &ctx,
        json!([
            {"ref":"attribute", "kind":"network_policy_attribute", "operation":"create",
             "attributes":{"name":format!(" {} ", name.to_uppercase()), "description":"Imported"}}
        ]),
    )
    .await
    .unwrap();
    assert_eq!(
        ctx.get_status(&format!("/policy/network/attributes/{name}"))
            .await,
        StatusCode::OK
    );
});

#[derive(Clone, Copy)]
enum TemplatePatch {
    Unchanged,
    Clear,
    Set,
}

#[rstest]
#[case(TemplatePatch::Unchanged)]
#[case(TemplatePatch::Clear)]
#[case(TemplatePatch::Set)]
#[actix_web::test]
async fn policy_patch_preserves_pattern_wire_semantics(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[case] patch: TemplatePatch,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    let name = NetworkPolicyName::new(ctx.name("patch-policy")).unwrap();
    let original = format!("original_{}", ctx.namespace());
    let replacement = format!("replacement_{}", ctx.namespace());
    let (payload, expected) = match patch {
        TemplatePatch::Unchanged => (json!({}), Some(original.clone())),
        TemplatePatch::Clear => (json!({"community_template_pattern":null}), None),
        TemplatePatch::Set => (
            json!({"community_template_pattern":format!(" {replacement} ")}),
            Some(replacement),
        ),
    };
    ctx.storage()
        .network_policies()
        .create_network_policy(
            CreateNetworkPolicy::new(name.clone(), "Original", Some(original.clone())).unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = ctx
        .patch_json(&format!("/policy/network/policies/{name}"), payload)
        .await;
    assert_eq!(
        (status, body["community_template_pattern"].clone()),
        (StatusCode::OK, json!(expected))
    );
}

dual_backend_test!(missing_policy_patch_returns_not_found, |ctx| {
    let name = NetworkPolicyName::new(ctx.name("missing")).unwrap();
    assert!(matches!(
        ctx.storage()
            .network_policies()
            .update_network_policy(&name, UpdateNetworkPolicy::default())
            .await,
        Err(AppError::NotFound(_))
    ));
});

dual_backend_test!(policy_deletion_clears_network_community_limit, |ctx| {
    let storage = ctx.storage();
    let policy = NetworkPolicyName::new(ctx.name("delete-policy")).unwrap();
    storage
        .network_policies()
        .create_network_policy(CreateNetworkPolicy::new(policy.clone(), "Policy", None).unwrap())
        .await
        .unwrap();
    let cidr = CidrValue::new(ctx.cidr(1)).unwrap();
    storage
        .networks()
        .create_network(
            CreateNetwork::new(cidr.clone(), "Network", ReservedCount::new(3).unwrap())
                .unwrap()
                .with_policy(Some(policy.clone()))
                .with_max_communities(Some(CommunityLimit::new(2).unwrap())),
        )
        .await
        .unwrap();
    storage
        .network_policies()
        .delete_network_policy(&policy)
        .await
        .unwrap();
    let network = storage.networks().get_network_by_cidr(&cidr).await.unwrap();
    assert_eq!(
        (network.policy_id(), network.max_communities()),
        (None, None)
    );
});

async fn seed_attributes(ctx: &TestCtx, count: usize) {
    for index in 0..count {
        ctx.storage()
            .network_policies()
            .create_network_policy_attribute(CreateNetworkPolicyAttribute::new(
                NetworkPolicyAttributeName::new(ctx.name(&format!("attribute-{index:04}")))
                    .unwrap(),
                "Attribute",
            ))
            .await
            .unwrap();
    }
}

dual_backend_test!(attribute_list_honors_limit_and_cursor, |ctx| {
    seed_attributes(&ctx, 2).await;
    let first = ctx.get_json("/policy/network/attributes?limit=1").await;
    let cursor = first["next_cursor"].as_str().expect("another page");
    let second = ctx
        .get_json(&format!(
            "/policy/network/attributes?limit=1&after={cursor}"
        ))
        .await;
    assert_eq!(
        (
            first["items"].as_array().unwrap().len(),
            second["items"].as_array().unwrap().len(),
            first["items"][0]["id"] != second["items"][0]["id"]
        ),
        (1, 1, true),
    );
});

#[rstest]
#[case("asc")]
#[case("desc")]
#[actix_web::test]
async fn attribute_list_honors_sorting(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[case] direction: &str,
) {
    let Some(ctx) = context(backend).await else {
        return;
    };
    seed_attributes(&ctx, 2).await;
    let page = ctx
        .get_json(&format!(
            "/policy/network/attributes?sort_by=name&sort_dir={direction}&limit=2"
        ))
        .await;
    let names = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        names.len() == 2
            && if direction == "asc" {
                names[0] < names[1]
            } else {
                names[0] > names[1]
            }
    );
}

#[rstest]
#[case("limit=0")]
#[case("sort_dir=invalid")]
#[case("sort_by=invalid")]
#[case("after=invalid")]
#[actix_web::test]
async fn attribute_list_rejects_invalid_query(#[case] query: &str) {
    let ctx = TestCtx::memory();
    seed_attributes(&ctx, 1).await;
    assert_eq!(
        ctx.get_status(&format!("/policy/network/attributes?{query}"))
            .await,
        StatusCode::BAD_REQUEST
    );
}

#[rstest]
#[case("", 100)]
#[case("?limit=18446744073709551615", 1000)]
#[actix_web::test]
async fn attribute_list_caps_public_page_sizes(#[case] query: &str, #[case] expected: usize) {
    let ctx = TestCtx::memory();
    seed_attributes(&ctx, 1001).await;
    let page = ctx
        .get_json(&format!("/policy/network/attributes{query}"))
        .await;
    assert_eq!(page["items"].as_array().unwrap().len(), expected);
}

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

#[rstest]
#[case(false)]
#[case(true)]
#[actix_web::test]
async fn postgres_concurrent_policy_patches_preserve_unrelated_fields(#[case] clear: bool) {
    let Some(ctx) = context(TestBackend::Postgres).await else {
        return;
    };
    let storage = ctx.storage();
    let name = NetworkPolicyName::new(ctx.name("concurrent-policy")).unwrap();
    let pattern = format!("pattern_{}", ctx.namespace());
    let policy = storage
        .network_policies()
        .create_network_policy(
            CreateNetworkPolicy::new(name.clone(), "Original", clear.then(|| pattern.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    let mut connection =
        PgConnection::establish(&common::postgres_test_database_url().unwrap().unwrap()).unwrap();
    sql_query("BEGIN").execute(&mut connection).unwrap();
    sql_query("SELECT id FROM network_policies WHERE id = $1 FOR UPDATE")
        .bind::<SqlUuid, _>(policy.id())
        .execute(&mut connection)
        .unwrap();
    let first_storage = storage.clone();
    let first_name = name.clone();
    let first = tokio::spawn(async move {
        first_storage
            .network_policies()
            .update_network_policy(
                &first_name,
                UpdateNetworkPolicy {
                    description: Some(RequiredDescription::new("Updated").unwrap()),
                    ..Default::default()
                },
            )
            .await
    });
    let second_storage = storage.clone();
    let second_name = name.clone();
    let pattern_update = if clear {
        UpdateField::Clear
    } else {
        UpdateField::Set(CommunityTemplatePattern::new(&pattern).unwrap())
    };
    let second = tokio::spawn(async move {
        second_storage
            .network_policies()
            .update_network_policy(
                &second_name,
                UpdateNetworkPolicy {
                    community_template_pattern: pattern_update,
                    ..Default::default()
                },
            )
            .await
    });

    // Both writers must reach their UPDATE before either can commit. Follow the
    // blocking chain because PostgreSQL can queue one writer behind the other.
    let blocked = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            sql_query("SELECT pg_stat_clear_snapshot()")
                .execute(&mut connection)
                .unwrap();
            let count = sql_query(
                "WITH RECURSIVE blocked(pid) AS (
                    SELECT pg_backend_pid()
                    UNION
                    SELECT activity.pid FROM pg_stat_activity activity
                    JOIN blocked ON blocked.pid = ANY(pg_blocking_pids(activity.pid))
                 ) SELECT count(*) FROM blocked WHERE pid <> pg_backend_pid()",
            )
            .get_result::<Count>(&mut connection)
            .unwrap()
            .count;
            if count == 2 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    sql_query("COMMIT").execute(&mut connection).unwrap();
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    blocked.expect("both policy updates must have waited for the row lock");
    let actual = storage
        .network_policies()
        .get_network_policy_by_name(&name)
        .await
        .unwrap();
    assert_eq!(
        (actual.description(), actual.community_template_pattern()),
        ("Updated", if clear { None } else { Some(pattern.as_str()) }),
    );
}
