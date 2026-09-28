mod common;

use actix_web::http::StatusCode;
use common::{TestBackend, TestCtx};
use rstest::rstest;
use serde_json::json;

#[rstest]
#[case::no_address_condition("", vec!["both", "empty", "first", "second"])]
#[case::one_address("address__endswith=0", vec!["both", "first"])]
#[case::negated_address("address__not_endswith=0", vec!["both", "second"])]
#[case::independent_conditions("address__endswith=0&address__not_endswith=0", vec!["both"])]
#[case::no_matches("address__endswith=9", vec![])]
#[actix_web::test]
async fn host_address_filters_preserve_exists_semantics(
    #[values(TestBackend::Memory, TestBackend::Postgres)] backend: TestBackend,
    #[case] query: &str,
    #[case] expected: Vec<&str>,
) {
    let ctx = match backend {
        TestBackend::Memory => TestCtx::memory(),
        TestBackend::Postgres => {
            let Some(ctx) = TestCtx::postgres().await else {
                eprintln!(
                    "{}",
                    common::postgres_skip_message("host address filtering")
                );
                return;
            };
            ctx
        }
    };
    let cidr = ctx.cidr(0);
    ctx.seed_network(&cidr).await;
    // Unique addresses cannot be shared by hosts. Match their last octet so
    // each of these four hosts can independently satisfy the same condition.
    for (name, addresses) in [
        ("both", vec![10, 11]),
        ("empty", vec![]),
        ("first", vec![20]),
        ("second", vec![21]),
    ] {
        let host = ctx.host(name);
        ctx.seed_host(&host).await;
        for offset in addresses {
            let (status, body) = ctx
                .post_json(
                    "/inventory/ip-addresses",
                    json!({"host_name": host, "address": ctx.ip_in_cidr(&cidr, offset)}),
                )
                .await;
            assert_eq!(status, StatusCode::CREATED, "{body}");
        }
    }
    let body = ctx
        .get_json(&format!(
            "/inventory/hosts?name__contains={}&sort_by=name&{query}",
            ctx.namespace()
        ))
        .await;
    let actual = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|host| host["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        expected
            .into_iter()
            .map(|name| ctx.host(name))
            .collect::<Vec<_>>()
    );
}
