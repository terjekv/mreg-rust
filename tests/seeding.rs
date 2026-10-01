mod common;

use std::sync::{Arc, Mutex};

use actix_web::{App, test, web};
use async_trait::async_trait;
use mreg_rust::{
    domain::{
        label::{CreateLabel, UpdateLabel},
        network_policy::CreateNetworkPolicyAttribute,
        pagination::PageRequest,
        seeds::SeedData,
        types::{
            DnsName, HostPolicyName, LabelName, NetworkPolicyAttributeName, NetworkPolicyName,
        },
    },
    errors::AppError,
    events::{DomainEvent, EventSink, EventSinkClient, EventSinkResult},
    services::Services,
};
use serde_json::json;

use common::TestCtx;

fn services(ctx: &TestCtx) -> Services {
    Services::new(ctx.storage(), EventSinkClient::noop())
}

fn catalog(ctx: &TestCtx) -> SeedData {
    serde_json::from_value(json!({"items": [
        {"kind": "network_policy_attribute", "name": ctx.name("flag"), "description": "Custom flag"},
        {"kind": "network_policy", "name": ctx.name("policy"), "description": "Site policy",
         "attributes": [{"name": ctx.name("flag"), "value": false}]},
        {"kind": "label", "name": ctx.name("label"), "description": "Site label"},
        {"kind": "nameserver", "name": ctx.host("ns"), "ttl": 7200},
        {"kind": "host_policy_atom", "name": ctx.name("atom"), "description": "Site atom"},
        {"kind": "host_policy_role", "name": ctx.name("role"), "description": "Site role"}
    ]})).unwrap()
}

dual_backend_test!(seed_catalog_without_http_or_migrations, |ctx| {
    services(&ctx).seed(&catalog(&ctx)).await.unwrap();
    let storage = ctx.storage();
    let attribute = storage
        .network_policies()
        .get_network_policy_attribute_by_name(
            &NetworkPolicyAttributeName::new(ctx.name("flag")).unwrap(),
        )
        .await
        .unwrap();
    let values = storage
        .network_policies()
        .list_network_policy_attribute_values(&NetworkPolicyName::new(ctx.name("policy")).unwrap())
        .await
        .unwrap();
    let label = storage
        .labels()
        .get_label_by_name(&LabelName::new(ctx.name("label")).unwrap())
        .await
        .unwrap();
    let ns = storage
        .nameservers()
        .get_nameserver_by_name(&DnsName::new(ctx.host("ns")).unwrap())
        .await
        .unwrap();
    let atom = storage
        .host_policy()
        .get_atom_by_name(&HostPolicyName::new(ctx.name("atom")).unwrap())
        .await
        .unwrap();
    let role = storage
        .host_policy()
        .get_role_by_name(&HostPolicyName::new(ctx.name("role")).unwrap())
        .await
        .unwrap();
    assert_eq!(
        (
            attribute.description(),
            values.len(),
            values[0].value(),
            label.description(),
            ns.ttl().unwrap().as_u32(),
            atom.description(),
            role.description()
        ),
        (
            "Custom flag",
            1,
            false,
            "Site label",
            7200,
            "Site atom",
            "Site role"
        )
    );
});

dual_backend_test!(seed_repeated_startup_preserves_api_edits, |ctx| {
    let services = services(&ctx);
    let seeds = catalog(&ctx);
    services.seed(&seeds).await.unwrap();
    let name = LabelName::new(ctx.name("label")).unwrap();
    let edited = services
        .labels()
        .update(
            &name,
            UpdateLabel::new(Some("Edited by operator".into())).unwrap(),
        )
        .await
        .unwrap();
    services.seed(&seeds).await.unwrap();
    let actual = ctx
        .storage()
        .labels()
        .get_label_by_name(&name)
        .await
        .unwrap();
    assert_eq!(actual, edited);
});

dual_backend_test!(seed_preserves_preexisting_data, |ctx| {
    let name = LabelName::new(ctx.name("label")).unwrap();
    let services = services(&ctx);
    let original = services
        .labels()
        .create(CreateLabel::new(name.clone(), "Existing data").unwrap())
        .await
        .unwrap();
    services.seed(&catalog(&ctx)).await.unwrap();
    assert_eq!(
        ctx.storage()
            .labels()
            .get_label_by_name(&name)
            .await
            .unwrap(),
        original
    );
});

dual_backend_test!(seed_concurrent_startup_creates_once, |ctx| {
    let services = services(&ctx);
    let seeds = catalog(&ctx);
    let (first, second) = tokio::join!(services.seed(&seeds), services.seed(&seeds));
    let mut counts = [first.unwrap(), second.unwrap()];
    counts.sort();
    assert_eq!(counts, [0, 6]);
});

fn invalid_catalog(ctx: &TestCtx) -> SeedData {
    serde_json::from_value(json!({"items": [
        {"kind": "label", "name": ctx.name("rollback"), "description": "Must roll back"},
        {"kind": "network_policy", "name": ctx.name("bad"), "description": "Missing dependency",
         "attributes": [{"name": ctx.name("missing"), "value": true}]}
    ]}))
    .unwrap()
}

dual_backend_test!(seed_failure_rolls_back_catalog_and_audit, |ctx| {
    services(&ctx)
        .seed(&invalid_catalog(&ctx))
        .await
        .unwrap_err();
    let storage = ctx.storage();
    let result = storage
        .labels()
        .get_label_by_name(&LabelName::new(ctx.name("rollback")).unwrap())
        .await;
    let history = storage
        .audit()
        .list_events(&PageRequest::all())
        .await
        .unwrap();
    assert_eq!(
        (
            matches!(result, Err(AppError::NotFound(_))),
            history
                .items
                .iter()
                .any(|event| event.resource_name() == ctx.name("rollback"))
        ),
        (true, false)
    );
});

dual_backend_test!(seed_audits_each_creation_once, |ctx| {
    let services = services(&ctx);
    let seeds = catalog(&ctx);
    services.seed(&seeds).await.unwrap();
    services.seed(&seeds).await.unwrap();
    let history = ctx
        .storage()
        .audit()
        .list_events(&PageRequest::all())
        .await
        .unwrap();
    let events = history
        .items
        .iter()
        .filter(|event| event.resource_name().contains(ctx.namespace()))
        .collect::<Vec<_>>();
    assert_eq!(
        (
            events.len(),
            events
                .iter()
                .all(|event| event.actor() == "system:seed" && event.action() == "create")
        ),
        (6, true)
    );
});

dual_backend_test!(seed_attribute_available_in_native_api, |ctx| {
    services(&ctx).seed(&catalog(&ctx)).await.unwrap();
    let body = ctx
        .get_json(&format!("/policy/network/attributes/{}", ctx.name("flag")))
        .await;
    assert_eq!(body["description"], "Custom flag");
});

#[actix_web::test]
async fn configured_seed_is_visible_through_legacy_api() {
    let state = common::memory_state();
    let seeds: SeedData = toml::from_str(include_str!("../scripts/mreg-cli-seeds.toml")).unwrap();
    state.services.seed(&seeds).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(|cfg| mreg_rust::api::configure(cfg, false)),
    )
    .await;
    let request = test::TestRequest::get()
        .uri("/api/v1/networkpolicyattributes/")
        .to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(
        body["results"][0]["description"],
        "The network uses client isolation."
    );
}

dual_backend_test!(no_implicit_isolated_attribute, |ctx| {
    assert!(matches!(
        ctx.storage()
            .network_policies()
            .get_network_policy_attribute_by_name(
                &NetworkPolicyAttributeName::new("isolated").unwrap()
            )
            .await,
        Err(AppError::NotFound(_))
    ));
});

#[actix_web::test]
async fn isolated_is_an_ordinary_attribute_without_explicit_protection() {
    let ctx = TestCtx::memory();
    let services = services(&ctx);
    let name = NetworkPolicyAttributeName::new("isolated").unwrap();
    services
        .network_policies()
        .create_attribute(CreateNetworkPolicyAttribute::new(
            name.clone(),
            "Operator defined",
        ))
        .await
        .unwrap();
    assert!(
        services
            .network_policies()
            .delete_attribute(&name)
            .await
            .is_ok()
    );
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<String>>);

#[async_trait]
impl EventSink for RecordingSink {
    async fn emit(&self, event: &DomainEvent) -> EventSinkResult {
        self.0.lock().unwrap().push(event.resource_name.clone());
        Ok(())
    }
}

dual_backend_test!(seed_emits_only_committed_creations, |ctx| {
    let sink = Arc::new(RecordingSink::default());
    let services = Services::new(ctx.storage(), EventSinkClient::with_sink(sink.clone()));
    services.seed(&invalid_catalog(&ctx)).await.unwrap_err();
    services.seed(&catalog(&ctx)).await.unwrap();
    services.seed(&catalog(&ctx)).await.unwrap();
    assert_eq!(sink.0.lock().unwrap().len(), 6);
});
