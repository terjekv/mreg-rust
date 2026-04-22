mod common;

use std::collections::HashSet;
use std::sync::Arc;

use actix_web::{App, body::to_bytes, http::StatusCode, test, web};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use chrono::{DateTime, Duration, Utc};
use mreg_rust::domain::{
    attachment::{
        CreateAttachmentDhcpIdentifier, CreateHostAttachment, DhcpIdentifierFamily,
        DhcpIdentifierKind,
    },
    exports::{CreateExportRun, CreateExportTemplate, ExportRunStatus},
    filters::{AttachmentCommunityAssignmentFilter, RecordFilter},
    host::{AssignIpAddress, CreateHost},
    imports::{
        CreateImportBatch, ImportBatch, ImportBatchStatus, ImportItem, ImportKind, ImportOperation,
    },
    network::CreateNetwork,
    pagination::PageRequest,
    resource_records::{
        CreateRecordInstance, CreateRecordTypeDefinition, RawRdataValue, RecordCardinality,
        RecordOwnerKind, RecordTypeSchema,
    },
    tasks::{CreateTask, TaskStatus},
    types::{
        BacnetIdentifier, CidrValue, CommunityName, DhcpPriority, DnsName, DnsTypeCode,
        EmailAddressValue, HostGroupName, HostPolicyName, Hostname, IpAddressValue, LabelName,
        MacAddressValue, NetworkPolicyName, RecordTypeName, ReservedCount, SerialNumber,
        SoaSeconds, Ttl, VlanId, ZoneName,
    },
    zone::CreateForwardZone,
};
use mreg_rust::errors::AppError;
use mreg_rust::{
    AppState, BuildInfo,
    authn::AuthnClient,
    authz::AuthorizerClient,
    config::{
        AuthMode, AuthScopeBackendConfig, AuthScopeConfig, Config, LocalUserConfig,
        StorageBackendSetting,
    },
    events::EventSinkClient,
    middleware,
    services::Services,
    storage::ReadableStorage,
    storage::build_storage,
};
use serde_json::{Value, json};

use common::TestCtx;

async fn postgres_ctx(test_name: &str) -> Option<TestCtx> {
    let ctx = TestCtx::postgres().await;
    if ctx.is_none() {
        eprintln!("{}", common::postgres_skip_message(test_name));
    }
    ctx
}

fn local_password_hash(password: &str) -> String {
    let salt = SaltString::encode_b64(b"static-local-salt").expect("salt");
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("password hash")
        .to_string()
}

async fn postgres_scoped_auth_state(
    scope_name: &str,
    allow_dev_authz_bypass: bool,
) -> Option<AppState> {
    // Warm the isolated schema once so fresh scoped-auth states do not race on
    // migrations when the test binary runs in parallel.
    common::postgres_state().await?;
    let database_url = match common::postgres_test_database_url() {
        Ok(Some(url)) => url,
        Ok(None) => return None,
        Err(error) => {
            if std::env::var("CI").is_ok() {
                panic!(
                    "FATAL in CI: postgres scoped auth state setup failed for scope \
                     '{scope_name}': {error}"
                );
            }
            eprintln!(
                "warning: postgres scoped auth state setup failed for scope \
                 '{scope_name}': {error}"
            );
            return None;
        }
    };
    let config = Config {
        workers: Some(1),
        database_url: Some(database_url),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Postgres,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass,
        auth_mode: AuthMode::Scoped,
        auth_jwt_signing_key: Some("postgres-test-jwt-signing-secret".to_string()),
        auth_scopes: vec![AuthScopeConfig {
            name: scope_name.to_string(),
            backend: AuthScopeBackendConfig::Local {
                users: vec![LocalUserConfig {
                    username: "admin".to_string(),
                    password_hash: local_password_hash("secret"),
                    groups: vec!["admins".to_string(), "ops".to_string()],
                }],
            },
        }],
        ..Config::default()
    };
    let storage = match build_storage(&config) {
        Ok(storage) => storage,
        Err(error) => {
            if std::env::var("CI").is_ok() {
                panic!(
                    "FATAL in CI: failed to build postgres scoped auth storage for scope \
                     '{scope_name}': {error}"
                );
            }
            eprintln!(
                "warning: failed to build postgres scoped auth storage for scope \
                 '{scope_name}': {error}"
            );
            return None;
        }
    };
    let authn = match AuthnClient::from_config(&config, storage.clone()) {
        Ok(authn) => authn,
        Err(error) => {
            if std::env::var("CI").is_ok() {
                panic!(
                    "FATAL in CI: failed to build postgres scoped auth client for scope \
                     '{scope_name}': {error}"
                );
            }
            eprintln!(
                "warning: failed to build postgres scoped auth client for scope \
                 '{scope_name}': {error}"
            );
            return None;
        }
    };
    let authz = AuthorizerClient::from_config(&config).expect("authz config");
    Some(AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        reader: ReadableStorage::new(storage.clone()),
        services: Services::new(storage, EventSinkClient::noop()),
        authn,
        authz,
    })
}

async fn call_auth_json(request: actix_http::Request, state: AppState) -> (StatusCode, Value) {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .wrap(middleware::Authn)
            .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
    )
    .await;
    let response = test::call_service(&app, request).await;
    let status = response.status();
    let bytes = to_bytes(response.into_body()).await.expect("body bytes");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, body)
}

struct ExtendedImportFixture {
    cidr: String,
    zone: String,
    delegation: String,
    nameserver: String,
    secondary_ns: String,
    host: String,
    address: String,
    policy: String,
    community: String,
    contact: String,
    group: String,
    label: String,
    atom: String,
    role: String,
    bacnet_id: u32,
}

fn build_extended_import_batch(
    ctx: &TestCtx,
    invalid_tail: bool,
) -> Result<(ImportBatch, ExtendedImportFixture), Box<dyn std::error::Error>> {
    let cidr = ctx.cidr(31);
    let zone = ctx.zone("legacy-zone");
    let delegation = format!("deleg.{}", zone);
    let nameserver = ctx.nameserver("ns1", &zone);
    let secondary_ns = ctx.nameserver("ns2", &zone);
    let host = ctx.host_in_zone("legacy-host", &zone);
    let address = ctx.ip_in_cidr(&cidr, 20);
    let policy = ctx.name("core-policy");
    let policy_attr = ctx.name("allow-dhcp");
    let community = ctx.name("core-community");
    let contact = format!("{}@example.test", ctx.name("ops"));
    let group = ctx.name("ops-group");
    let label = ctx.name("managed");
    let atom = ctx.name("baseline");
    let role = ctx.name("server-role");
    let bacnet_id = ctx.bacnet_id(1);

    let mut items = vec![
        ImportItem::new(
            "ns-1",
            ImportKind::Nameserver,
            ImportOperation::Create,
            json!({ "name": nameserver, "ttl": 600 }),
        )?,
        ImportItem::new(
            "ns-2",
            ImportKind::Nameserver,
            ImportOperation::Create,
            json!({ "name": secondary_ns }),
        )?,
        ImportItem::new(
            "policy-1",
            ImportKind::NetworkPolicy,
            ImportOperation::Create,
            json!({
                "name": policy,
                "description": "Imported policy",
                "community_template_pattern": "campus"
            }),
        )?,
        ImportItem::new(
            "policy-attr-1",
            ImportKind::NetworkPolicyAttribute,
            ImportOperation::Create,
            json!({
                "name": policy_attr,
                "description": "Imported attribute"
            }),
        )?,
        ImportItem::new(
            "policy-attr-value-1",
            ImportKind::NetworkPolicyAttributeValue,
            ImportOperation::Create,
            json!({
                "policy_name": policy,
                "attribute_name": policy_attr,
                "value": true
            }),
        )?,
        ImportItem::new(
            "network-1",
            ImportKind::Network,
            ImportOperation::Create,
            json!({
                "cidr": cidr,
                "description": "Imported network",
                "vlan": 42,
                "dns_delegated": true,
                "category": "prod",
                "location": "dc1",
                "frozen": true,
                "reserved": 5,
                "max_communities": 7,
                "policy_name": policy
            }),
        )?,
        ImportItem::new(
            "zone-1",
            ImportKind::ForwardZone,
            ImportOperation::Create,
            json!({
                "name": zone,
                "primary_ns": nameserver,
                "nameservers": [nameserver, secondary_ns],
                "email": format!("hostmaster@{zone}")
            }),
        )?,
        ImportItem::new(
            "label-1",
            ImportKind::Label,
            ImportOperation::Create,
            json!({
                "name": label,
                "description": "Managed host"
            }),
        )?,
        ImportItem::new(
            "host-1",
            ImportKind::Host,
            ImportOperation::Create,
            json!({
                "name": host,
                "zone": zone,
                "ttl": 1800,
                "comment": "Imported host"
            }),
        )?,
        ImportItem::new(
            "attachment-1",
            ImportKind::HostAttachment,
            ImportOperation::Create,
            json!({
                "host_name": host,
                "network": cidr,
                "mac_address": "aa:bb:cc:dd:ee:ff"
            }),
        )?,
        ImportItem::new(
            "ip-1",
            ImportKind::IpAddress,
            ImportOperation::Create,
            json!({
                "host_name": host,
                "network": cidr,
                "mac_address": "aa:bb:cc:dd:ee:ff",
                "address": address
            }),
        )?,
        ImportItem::new(
            "attachment-id-1",
            ImportKind::AttachmentDhcpIdentifier,
            ImportOperation::Create,
            json!({
                "attachment_id_ref": "attachment-1",
                "family": 4,
                "kind": "client_id",
                "value": "01:aa:bb:cc:dd:ee:ff",
                "priority": 10
            }),
        )?,
        ImportItem::new(
            "contact-1",
            ImportKind::HostContact,
            ImportOperation::Create,
            json!({
                "email": contact,
                "display_name": "Ops Team",
                "hosts": [host]
            }),
        )?,
        ImportItem::new(
            "group-1",
            ImportKind::HostGroup,
            ImportOperation::Create,
            json!({
                "name": group,
                "description": "Imported host group",
                "hosts": [host],
                "owner_groups": ["ops-admins"]
            }),
        )?,
        ImportItem::new(
            "bacnet-1",
            ImportKind::BacnetId,
            ImportOperation::Create,
            json!({
                "bacnet_id": bacnet_id,
                "host_name": host
            }),
        )?,
        ImportItem::new(
            "ptr-1",
            ImportKind::PtrOverride,
            ImportOperation::Create,
            json!({
                "host_name": host,
                "address": address,
                "target_name": format!("ptr.{zone}")
            }),
        )?,
        ImportItem::new(
            "community-1",
            ImportKind::Community,
            ImportOperation::Create,
            json!({
                "policy_name": policy,
                "network": cidr,
                "name": community,
                "description": "Imported community"
            }),
        )?,
        ImportItem::new(
            "mapping-1",
            ImportKind::AttachmentCommunityAssignment,
            ImportOperation::Create,
            json!({
                "attachment_id_ref": "attachment-1",
                "policy_name_ref": "policy-1",
                "community_name": community
            }),
        )?,
        ImportItem::new(
            "atom-1",
            ImportKind::HostPolicyAtom,
            ImportOperation::Create,
            json!({
                "name": atom,
                "description": "Imported atom"
            }),
        )?,
        ImportItem::new(
            "role-1",
            ImportKind::HostPolicyRole,
            ImportOperation::Create,
            json!({
                "name": role,
                "description": "Imported role"
            }),
        )?,
        ImportItem::new(
            "role-atom-1",
            ImportKind::HostPolicyRoleAtom,
            ImportOperation::Create,
            json!({
                "role_name": role,
                "atom_name": atom
            }),
        )?,
        ImportItem::new(
            "role-host-1",
            ImportKind::HostPolicyRoleHost,
            ImportOperation::Create,
            json!({
                "role_name": role,
                "host_name": host
            }),
        )?,
        ImportItem::new(
            "delegation-1",
            ImportKind::ForwardZoneDelegation,
            ImportOperation::Create,
            json!({
                "zone": zone,
                "name": delegation,
                "comment": "Imported delegation",
                "nameservers": [nameserver]
            }),
        )?,
    ];

    items.push(if invalid_tail {
        ImportItem::new(
            "role-label-invalid",
            ImportKind::HostPolicyRoleLabel,
            ImportOperation::Create,
            json!({
                "role_name": role,
                "label_name": ctx.name("missing-label")
            }),
        )?
    } else {
        ImportItem::new(
            "role-label-1",
            ImportKind::HostPolicyRoleLabel,
            ImportOperation::Create,
            json!({
                "role_name": role,
                "label_name": label
            }),
        )?
    });

    Ok((
        ImportBatch::new(items)?,
        ExtendedImportFixture {
            cidr,
            zone,
            delegation,
            nameserver,
            secondary_ns,
            host,
            address,
            policy,
            community,
            contact,
            group,
            label,
            atom,
            role,
            bacnet_id,
        },
    ))
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_task_claiming_uses_skip_locked_under_concurrency()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_task_claiming_uses_skip_locked_under_concurrency").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    while let Some(task) = storage.tasks().claim_next_task().await? {
        let _ = storage
            .tasks()
            .complete_task(task.id(), json!({ "drained": true }))
            .await;
    }

    let first = storage
        .tasks()
        .create_task(CreateTask::new(
            "import_batch",
            Some("tester".to_string()),
            json!({"import_id":"00000000-0000-0000-0000-000000000001"}),
            Some(ctx.name("claim-1")),
            1,
        )?)
        .await?;
    let second = storage
        .tasks()
        .create_task(CreateTask::new(
            "export_run",
            Some("tester".to_string()),
            json!({"run_id":"00000000-0000-0000-0000-000000000002"}),
            Some(ctx.name("claim-2")),
            1,
        )?)
        .await?;

    let storage_a = storage.clone();
    let storage_b = storage.clone();
    let (claimed_a, claimed_b) = tokio::join!(
        async move { storage_a.tasks().claim_next_task().await },
        async move { storage_b.tasks().claim_next_task().await },
    );
    let claimed_a = claimed_a?.expect("first concurrent claim");
    let claimed_b = claimed_b?.expect("second concurrent claim");

    assert_eq!(claimed_a.status(), &TaskStatus::Running);
    assert_eq!(claimed_b.status(), &TaskStatus::Running);
    assert_ne!(claimed_a.id(), claimed_b.id());
    assert!(
        [first.id(), second.id()].contains(&claimed_a.id())
            && [first.id(), second.id()].contains(&claimed_b.id())
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_assign_ip_rolls_back_when_auto_record_creation_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) =
        postgres_ctx("postgres_assign_ip_rolls_back_when_auto_record_creation_fails").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    let cidr = ctx.cidr(20);
    let host = ctx.host("rollback");
    let address = ctx.ip_in_cidr(&cidr, 50);

    storage
        .networks()
        .create_network(CreateNetwork::new(
            CidrValue::new(&cidr)?,
            "Rollback network",
            ReservedCount::new(3)?,
        )?)
        .await?;

    storage
        .hosts()
        .create_host(CreateHost::new(
            Hostname::new(&host)?,
            None,
            None,
            "rollback host",
        )?)
        .await?;

    storage
        .records()
        .create_record(CreateRecordInstance::new(
            RecordTypeName::new("CNAME")?,
            RecordOwnerKind::Host,
            &host,
            Some(Ttl::new(300)?),
            json!({"target": ctx.host("alias-target")}),
        )?)
        .await?;

    let result = storage
        .hosts()
        .assign_ip_address(AssignIpAddress::new(
            Hostname::new(&host)?,
            Some(IpAddressValue::new(&address)?),
            None,
            None,
        )?)
        .await;
    assert!(result.is_err());

    let assignments = storage
        .hosts()
        .list_ip_addresses_for_host(&Hostname::new(&host)?, &PageRequest::default())
        .await?;
    assert!(assignments.items.is_empty());

    let records = storage
        .records()
        .list_records(&PageRequest::default(), &RecordFilter::default())
        .await?;
    let matching: Vec<_> = records
        .items
        .into_iter()
        .filter(|record| record.owner_name() == host.as_str())
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].type_name().as_str(), "CNAME");

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_record_registry_and_export_run_work() -> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_record_registry_and_export_run_work").await else {
        return Ok(());
    };
    let storage = ctx.storage();

    let host = ctx.host("export-app");
    let template_name = ctx.name("network-summary");

    storage
        .hosts()
        .create_host(CreateHost::new(
            Hostname::new(&host)?,
            None,
            None,
            "app host",
        )?)
        .await?;

    let record_types = storage
        .records()
        .list_record_types(&PageRequest::default())
        .await?;
    assert!(
        record_types
            .items
            .iter()
            .any(|record_type| record_type.name().as_str() == "CNAME")
    );

    let record = storage
        .records()
        .create_record(CreateRecordInstance::new(
            RecordTypeName::new("CNAME")?,
            RecordOwnerKind::Host,
            &host,
            Some(Ttl::new(3600)?),
            json!({"target":"alias.example.org."}),
        )?)
        .await?;
    assert_eq!(record.data()["target"], "alias.example.org");

    let mut export_network: Option<String> = None;
    for slot in 21..=40 {
        let candidate_cidr = ctx.cidr(slot);
        match storage
            .networks()
            .create_network(CreateNetwork::new(
                CidrValue::new(&candidate_cidr)?,
                "Export network",
                ReservedCount::new(3)?,
            )?)
            .await
        {
            Ok(network) => {
                export_network = Some(network.cidr().as_str().to_string());
                break;
            }
            Err(AppError::Conflict(_)) => continue,
            Err(error) => return Err(Box::new(error) as Box<dyn std::error::Error>),
        }
    }
    let export_cidr = export_network.expect("should find an unused export network cidr");

    let template = storage
        .exports()
        .create_export_template(CreateExportTemplate::new(
            &template_name,
            "Summarize networks",
            "minijinja",
            "inventory",
            "{% for network in networks %}{{ network.cidr }} {{ network.description }}\n{% endfor %}",
            json!({}),
        )?)
        .await?;

    let run = storage
        .exports()
        .create_export_run(CreateExportRun::new(
            template.name().to_string(),
            Some("tester".to_string()),
            "inventory",
            json!({}),
        )?)
        .await?;
    let rendered = storage.exports().run_export(run.id()).await?;

    assert_eq!(rendered.status(), &ExportRunStatus::Succeeded);
    assert!(
        rendered
            .rendered_output()
            .unwrap_or_default()
            .contains(&format!("{export_cidr} Export network"))
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_dhcp_export_scope_renders_attachment_graph()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_dhcp_export_scope_renders_attachment_graph").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    let host = ctx.host("dhcp-host");
    let cidr = ctx.cidr(41);
    let address = ctx.ip_in_cidr(&cidr, 30);

    storage
        .hosts()
        .create_host(CreateHost::new(
            Hostname::new(&host)?,
            None,
            None,
            "dhcp host",
        )?)
        .await?;
    storage
        .networks()
        .create_network(CreateNetwork::new(
            CidrValue::new(&cidr)?,
            "DHCP network",
            ReservedCount::new(3)?,
        )?)
        .await?;
    let attachment = storage
        .attachments()
        .create_attachment(CreateHostAttachment::new(
            Hostname::new(&host)?,
            CidrValue::new(&cidr)?,
            Some(MacAddressValue::new("aa:bb:cc:dd:ee:ff")?),
            Some("uplink".to_string()),
        ))
        .await?;
    storage
        .hosts()
        .assign_ip_address(AssignIpAddress::new(
            Hostname::new(&host)?,
            Some(IpAddressValue::new(&address)?),
            Some(CidrValue::new(&cidr)?),
            Some(MacAddressValue::new("aa:bb:cc:dd:ee:ff")?),
        )?)
        .await?;
    storage
        .attachments()
        .create_attachment_dhcp_identifier(CreateAttachmentDhcpIdentifier::new(
            attachment.id(),
            DhcpIdentifierFamily::V4,
            DhcpIdentifierKind::ClientId,
            "01:aa:bb:cc:dd:ee:ff",
            DhcpPriority::new(10),
        )?)
        .await?;

    let run = storage
        .exports()
        .create_export_run(CreateExportRun::new(
            "dhcp-canonical-json",
            Some("tester".to_string()),
            "dhcp",
            json!({}),
        )?)
        .await?;
    let rendered = storage.exports().run_export(run.id()).await?;

    assert_eq!(rendered.status(), &ExportRunStatus::Succeeded);
    let output: serde_json::Value =
        serde_json::from_str(rendered.rendered_output().unwrap_or_default())?;
    assert_eq!(output["scope"], "dhcp");
    let network = output["dhcp4_networks"]
        .as_array()
        .and_then(|networks| {
            networks
                .iter()
                .find(|network| network["cidr"].as_str() == Some(cidr.as_str()))
        })
        .expect("dhcp network should exist");
    let attachment = network["dhcp4_attachments"]
        .as_array()
        .and_then(|attachments| {
            attachments
                .iter()
                .find(|attachment| attachment["host_name"].as_str() == Some(host.as_str()))
        })
        .expect("dhcp attachment should exist");
    assert_eq!(network["cidr"], cidr);
    assert_eq!(attachment["host_name"], host);
    assert_eq!(attachment["matchers"]["ipv4"]["kind"], "client_id");
    assert_eq!(attachment["primary_ipv4_address"], address);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_import_supports_zone_and_record_entities()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_import_supports_zone_and_record_entities").await else {
        return Ok(());
    };
    let storage = ctx.storage();

    let zone = ctx.zone("import-zone");
    let nameserver = ctx.nameserver("ns1", &zone);
    let host = ctx.host_in_zone("app", &zone);

    storage
        .nameservers()
        .create_nameserver(mreg_rust::domain::nameserver::CreateNameServer::new(
            DnsName::new(&nameserver)?,
            None,
        ))
        .await?;

    let summary = storage
        .imports()
        .create_import_batch(CreateImportBatch::new(
            ImportBatch::new(vec![
                ImportItem::new(
                    "zone-1",
                    ImportKind::ForwardZone,
                    ImportOperation::Create,
                    json!({
                        "name": zone,
                        "primary_ns": nameserver,
                        "nameservers": [nameserver],
                        "email": format!("hostmaster@{zone}")
                    }),
                )?,
                ImportItem::new(
                    "host-1",
                    ImportKind::Host,
                    ImportOperation::Create,
                    json!({
                        "name": host,
                        "zone": zone
                    }),
                )?,
                ImportItem::new(
                    "record-1",
                    ImportKind::Record,
                    ImportOperation::Create,
                    json!({
                        "type_name": "CNAME",
                        "owner_kind": "host",
                        "owner_name": host,
                        "data": {"target": "alias.example.org."}
                    }),
                )?,
            ])?,
            Some("tester".to_string()),
        ))
        .await?;

    let result = storage.imports().run_import_batch(summary.id()).await?;
    assert_eq!(result.status(), &ImportBatchStatus::Succeeded);

    let loaded_host = storage
        .hosts()
        .get_host_by_name(&Hostname::new(&host)?)
        .await?;
    assert_eq!(loaded_host.zone(), Some(&ZoneName::new(&zone)?));
    let records = storage
        .records()
        .list_records(&PageRequest::default(), &RecordFilter::default())
        .await?;
    let imported_record = records
        .items
        .into_iter()
        .find(|record| record.owner_name() == host.as_str())
        .expect("imported record should exist");
    assert_eq!(imported_record.data()["target"], "alias.example.org");

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_rejects_rrset_ttl_mismatches_and_alias_mx_targets()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) =
        postgres_ctx("postgres_rejects_rrset_ttl_mismatches_and_alias_mx_targets").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    let zone = ctx.zone("mx-zone");
    let nameserver = ctx.nameserver("ns1", &zone);
    let alias_host = ctx.host_in_zone("mailalias", &zone);

    storage
        .nameservers()
        .create_nameserver(mreg_rust::domain::nameserver::CreateNameServer::new(
            DnsName::new(&nameserver)?,
            None,
        ))
        .await?;

    storage
        .zones()
        .create_forward_zone(CreateForwardZone::new(
            ZoneName::new(&zone)?,
            DnsName::new(&nameserver)?,
            vec![DnsName::new(&nameserver)?],
            EmailAddressValue::new(format!("hostmaster@{zone}"))?,
            SerialNumber::new(1)?,
            SoaSeconds::new(10800)?,
            SoaSeconds::new(3600)?,
            SoaSeconds::new(604800)?,
            Ttl::new(43_200)?,
            Ttl::new(43_200)?,
        ))
        .await?;

    storage
        .records()
        .create_record(CreateRecordInstance::new(
            RecordTypeName::new("MX")?,
            RecordOwnerKind::ForwardZone,
            &zone,
            Some(Ttl::new(300)?),
            json!({"preference": 10, "exchange": format!("mail.{zone}")}),
        )?)
        .await?;

    let ttl_mismatch = storage
        .records()
        .create_record(CreateRecordInstance::new(
            RecordTypeName::new("MX")?,
            RecordOwnerKind::ForwardZone,
            &zone,
            Some(Ttl::new(600)?),
            json!({"preference": 20, "exchange": format!("backup.{zone}")}),
        )?)
        .await;
    assert!(ttl_mismatch.is_err());

    storage
        .hosts()
        .create_host(CreateHost::new(
            Hostname::new(&alias_host)?,
            Some(ZoneName::new(&zone)?),
            None,
            "mail alias",
        )?)
        .await?;

    storage
        .records()
        .create_record(CreateRecordInstance::new(
            RecordTypeName::new("CNAME")?,
            RecordOwnerKind::Host,
            &alias_host,
            Some(Ttl::new(300)?),
            json!({"target": format!("realmail.{zone}")}),
        )?)
        .await?;

    let alias_target = storage
        .records()
        .create_record(CreateRecordInstance::new(
            RecordTypeName::new("MX")?,
            RecordOwnerKind::ForwardZone,
            &zone,
            Some(Ttl::new(300)?),
            json!({"preference": 30, "exchange": alias_host}),
        )?)
        .await;
    assert!(alias_target.is_err());

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_imports_extended_legacy_entities() -> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_imports_extended_legacy_entities").await else {
        return Ok(());
    };
    let storage = ctx.storage();
    let (batch, fixture) = build_extended_import_batch(&ctx, false)?;

    let summary = storage
        .imports()
        .create_import_batch(CreateImportBatch::new(batch, Some("tester".to_string())))
        .await?;

    let result = storage.imports().run_import_batch(summary.id()).await?;
    assert_eq!(result.status(), &ImportBatchStatus::Succeeded);

    let imported_ns = storage
        .nameservers()
        .get_nameserver_by_name(&DnsName::new(&fixture.nameserver)?)
        .await?;
    assert_eq!(imported_ns.ttl(), Some(Ttl::new(600)?));

    let imported_network = storage
        .networks()
        .get_network_by_cidr(&CidrValue::new(&fixture.cidr)?)
        .await?;
    assert_eq!(imported_network.vlan(), Some(VlanId::new(42).unwrap()));
    assert!(imported_network.dns_delegated());
    assert_eq!(imported_network.category(), "prod");
    assert_eq!(imported_network.location(), "dc1");
    assert!(imported_network.frozen());
    assert_eq!(imported_network.reserved(), ReservedCount::new(5).unwrap());

    let imported_host = storage
        .hosts()
        .get_host_by_name(&Hostname::new(&fixture.host)?)
        .await?;
    assert_eq!(imported_host.zone(), Some(&ZoneName::new(&fixture.zone)?));
    assert_eq!(imported_host.ttl(), Some(Ttl::new(1800)?));

    let ip_assignments = storage
        .hosts()
        .list_ip_addresses_for_host(&Hostname::new(&fixture.host)?, &PageRequest::default())
        .await?;
    assert_eq!(ip_assignments.items.len(), 1);
    assert_eq!(
        ip_assignments.items[0]
            .mac_address()
            .map(|value| value.as_str().to_ascii_lowercase()),
        Some("aa:bb:cc:dd:ee:ff".to_string())
    );

    let imported_contact = storage
        .host_contacts()
        .get_host_contact_by_email(&EmailAddressValue::new(&fixture.contact)?)
        .await?;
    assert_eq!(imported_contact.hosts(), &[Hostname::new(&fixture.host)?]);

    let imported_group = storage
        .host_groups()
        .get_host_group_by_name(&HostGroupName::new(&fixture.group)?)
        .await?;
    assert_eq!(imported_group.hosts(), &[Hostname::new(&fixture.host)?]);

    let bacnet = storage
        .bacnet()
        .get_bacnet_id(BacnetIdentifier::new(fixture.bacnet_id)?)
        .await?;
    assert_eq!(bacnet.host_name(), &Hostname::new(&fixture.host)?);

    let ptr = storage
        .ptr_overrides()
        .get_ptr_override_by_address(&IpAddressValue::new(&fixture.address)?)
        .await?;
    let expected_ptr = format!("ptr.{}", fixture.zone);
    assert_eq!(
        ptr.target_name().map(|value| value.as_str()),
        Some(expected_ptr.as_str())
    );

    let imported_policy = storage
        .network_policies()
        .get_network_policy_by_name(&NetworkPolicyName::new(&fixture.policy)?)
        .await?;
    assert_eq!(imported_policy.community_template_pattern(), Some("campus"));

    let imported_community = storage
        .communities()
        .find_community_by_names(
            &NetworkPolicyName::new(&fixture.policy)?,
            &CommunityName::new(&fixture.community)?,
        )
        .await?;
    assert_eq!(
        imported_community.network_cidr().as_str(),
        fixture.cidr.as_str()
    );

    let attachments = storage
        .attachments()
        .list_attachments_for_host(&Hostname::new(&fixture.host)?)
        .await?;
    assert_eq!(attachments.len(), 1);
    assert_eq!(
        attachments[0]
            .mac_address()
            .map(|value| value.as_str().to_ascii_lowercase()),
        Some("aa:bb:cc:dd:ee:ff".to_string())
    );

    let identifiers = storage
        .attachments()
        .list_attachment_dhcp_identifiers(attachments[0].id())
        .await?;
    assert_eq!(identifiers.len(), 1);
    assert_eq!(identifiers[0].value(), "01:aa:bb:cc:dd:ee:ff");

    let assignments = storage
        .attachment_community_assignments()
        .list_attachment_community_assignments(
            &PageRequest::default(),
            &AttachmentCommunityAssignmentFilter::from_query_params(
                std::collections::HashMap::from([("host".to_string(), fixture.host.clone())]),
            )?,
        )
        .await?;
    assert_eq!(assignments.items.len(), 1);
    assert_eq!(
        assignments.items[0].host_name(),
        &Hostname::new(&fixture.host)?
    );

    let imported_role = storage
        .host_policy()
        .get_role_by_name(&HostPolicyName::new(&fixture.role)?)
        .await?;
    assert!(imported_role.atoms().contains(&fixture.atom));
    assert!(imported_role.hosts().contains(&fixture.host));
    assert!(imported_role.labels().contains(&fixture.label));

    let delegations = storage
        .zones()
        .list_forward_zone_delegations(&ZoneName::new(&fixture.zone)?, &PageRequest::default())
        .await?;
    assert_eq!(delegations.items.len(), 1);
    assert_eq!(delegations.items[0].name().as_str(), fixture.delegation);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_import_rolls_back_extended_legacy_batch_on_late_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) =
        postgres_ctx("postgres_import_rolls_back_extended_legacy_batch_on_late_failure").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();
    let (batch, fixture) = build_extended_import_batch(&ctx, true)?;

    let summary = storage
        .imports()
        .create_import_batch(CreateImportBatch::new(batch, Some("tester".to_string())))
        .await?;

    let error = storage
        .imports()
        .run_import_batch(summary.id())
        .await
        .expect_err("late invalid item should fail the import");
    assert!(error.to_string().contains("role-label-invalid"));

    let imports = storage
        .imports()
        .list_import_batches(&PageRequest::default())
        .await?;
    let stored = imports
        .items
        .into_iter()
        .find(|item| item.id() == summary.id())
        .expect("stored import batch");
    assert_eq!(stored.status(), &ImportBatchStatus::Failed);
    assert!(stored.commit_summary().is_none());

    assert!(
        storage
            .nameservers()
            .get_nameserver_by_name(&DnsName::new(&fixture.nameserver)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .nameservers()
            .get_nameserver_by_name(&DnsName::new(&fixture.secondary_ns)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .networks()
            .get_network_by_cidr(&CidrValue::new(&fixture.cidr)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .zones()
            .get_forward_zone_by_name(&ZoneName::new(&fixture.zone)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .labels()
            .get_label_by_name(&LabelName::new(&fixture.label)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .hosts()
            .get_host_by_name(&Hostname::new(&fixture.host)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .host_contacts()
            .get_host_contact_by_email(&EmailAddressValue::new(&fixture.contact)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .host_groups()
            .get_host_group_by_name(&HostGroupName::new(&fixture.group)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .bacnet()
            .get_bacnet_id(BacnetIdentifier::new(fixture.bacnet_id)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .ptr_overrides()
            .get_ptr_override_by_address(&IpAddressValue::new(&fixture.address)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .network_policies()
            .get_network_policy_by_name(&NetworkPolicyName::new(&fixture.policy)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .communities()
            .find_community_by_names(
                &NetworkPolicyName::new(&fixture.policy)?,
                &CommunityName::new(&fixture.community)?,
            )
            .await
            .is_err()
    );
    assert!(
        storage
            .host_policy()
            .get_atom_by_name(&HostPolicyName::new(&fixture.atom)?)
            .await
            .is_err()
    );
    assert!(
        storage
            .host_policy()
            .get_role_by_name(&HostPolicyName::new(&fixture.role)?)
            .await
            .is_err()
    );

    let assignments = storage
        .attachment_community_assignments()
        .list_attachment_community_assignments(
            &PageRequest::default(),
            &AttachmentCommunityAssignmentFilter::from_query_params(
                std::collections::HashMap::from([("host".to_string(), fixture.host.clone())]),
            )?,
        )
        .await?;
    assert!(assignments.items.is_empty());

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_supports_unanchored_srv_and_rfc3597_raw_records()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_supports_unanchored_srv_and_rfc3597_raw_records").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    let zone = ctx.zone("srv-zone");
    let nameserver = ctx.nameserver("ns1", &zone);
    let unanchored_name = format!("_sip._tcp.{zone}");
    let raw_owner = ctx.host_in_zone("opaque", &zone);
    storage
        .nameservers()
        .create_nameserver(mreg_rust::domain::nameserver::CreateNameServer::new(
            DnsName::new(&nameserver)?,
            None,
        ))
        .await?;

    storage
        .zones()
        .create_forward_zone(CreateForwardZone::new(
            ZoneName::new(&zone)?,
            DnsName::new(&nameserver)?,
            vec![DnsName::new(&nameserver)?],
            EmailAddressValue::new(format!("hostmaster@{zone}"))?,
            SerialNumber::new(1)?,
            SoaSeconds::new(10800)?,
            SoaSeconds::new(3600)?,
            SoaSeconds::new(604800)?,
            Ttl::new(43_200)?,
            Ttl::new(43_200)?,
        ))
        .await?;

    let srv = storage
        .records()
        .create_record(CreateRecordInstance::new_unanchored(
            RecordTypeName::new("SRV")?,
            &unanchored_name,
            Some(Ttl::new(3600)?),
            json!({
                "priority": 10,
                "weight": 5,
                "port": 5060,
                "target": format!("sip1.{zone}")
            }),
        )?)
        .await?;
    assert_eq!(srv.owner_name(), unanchored_name);

    let namespace_seed = u16::from_str_radix(&ctx.namespace()[1..5], 16).unwrap_or(0);
    let mut raw_type = None;
    let mut raw_type_name = None;
    for offset in 0..512u16 {
        let raw_type_number = 60000 + ((namespace_seed.wrapping_add(offset)) % 5000);
        let candidate_name = format!("TYPE{raw_type_number}");
        match storage
            .records()
            .create_record_type(CreateRecordTypeDefinition::new(
                RecordTypeName::new(&candidate_name)?,
                Some(DnsTypeCode::new(raw_type_number as i32)?),
                RecordTypeSchema::new(
                    RecordOwnerKind::Host,
                    RecordCardinality::Multiple,
                    false,
                    Vec::new(),
                    json!({ "rfc3597": { "allow_raw_rdata": true } }),
                    None,
                )?,
                false,
            ))
            .await
        {
            Ok(created) => {
                raw_type = Some(created);
                raw_type_name = Some(candidate_name);
                break;
            }
            Err(AppError::Conflict(_)) => continue,
            Err(error) => return Err(Box::new(error) as Box<dyn std::error::Error>),
        }
    }
    let raw_type = raw_type.expect("should find an unused custom record type number");
    let raw_type_name = raw_type_name.expect("raw type name should match created type");
    assert_eq!(raw_type.name().as_str(), raw_type_name);

    let raw_record = storage
        .records()
        .create_record(CreateRecordInstance::new_raw(
            RecordTypeName::new(&raw_type_name)?,
            None,
            &raw_owner,
            None,
            Some(Ttl::new(900)?),
            RawRdataValue::from_presentation("\\# 4 deadbeef")?,
        )?)
        .await?;
    assert_eq!(
        raw_record.raw_rdata().map(|raw| raw.presentation()),
        Some("\\# 4 deadbeef".to_string())
    );

    let srv_rrset = storage.records().get_rrset(srv.rrset_id()).await?;
    assert_eq!(srv_rrset.owner_name().as_str(), unanchored_name);

    let raw_rrset = storage.records().get_rrset(raw_record.rrset_id()).await?;
    assert_eq!(raw_rrset.owner_name().as_str(), raw_owner);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_record_response_preserves_typed_and_opaque_wire_shapes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) =
        postgres_ctx("postgres_record_response_preserves_typed_and_opaque_wire_shapes").await
    else {
        return Ok(());
    };

    let host = ctx.host("typed-record");
    ctx.seed_host(&host).await;

    let (status, created_typed) = ctx
        .post_json(
            "/dns/records",
            json!({
                "type_name": "CNAME",
                "owner_kind": "host",
                "owner_name": host,
                "ttl": 300,
                "data": {
                    "target": "alias.example.org"
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created_typed["type_name"], "CNAME");
    assert_eq!(created_typed["data"]["target"], "alias.example.org");
    assert!(created_typed.get("kind").is_none());

    let typed_id = created_typed["id"].as_str().expect("typed record id");
    let fetched_typed = ctx.get_json(&format!("/dns/records/{typed_id}")).await;
    assert_eq!(fetched_typed["type_name"], "CNAME");
    assert_eq!(fetched_typed["data"]["target"], "alias.example.org");
    assert!(fetched_typed.get("kind").is_none());

    let raw_type_name = "TYPE65534";
    let (status, _) = ctx
        .post_json(
            "/dns/record-types",
            json!({
                "name": raw_type_name,
                "dns_type": 65534,
                "owner_kind": "host",
                "cardinality": "multiple",
                "fields": [],
                "behavior_flags": {
                    "rfc3597": { "allow_raw_rdata": true }
                }
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let raw_owner = ctx.host("opaque-record");
    let (status, created_opaque) = ctx
        .post_json(
            "/dns/records",
            json!({
                "type_name": raw_type_name,
                "owner_name": raw_owner,
                "ttl": 300,
                "raw_rdata": "\\# 6 cafe01020304"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created_opaque["type_name"], raw_type_name);
    assert!(created_opaque["data"].is_null());
    assert_eq!(created_opaque["raw_rdata"], "\\# 6 cafe01020304");
    assert!(created_opaque.get("kind").is_none());

    let opaque_id = created_opaque["id"].as_str().expect("opaque record id");
    let fetched_opaque = ctx.get_json(&format!("/dns/records/{opaque_id}")).await;
    assert_eq!(fetched_opaque["type_name"], raw_type_name);
    assert!(fetched_opaque["data"].is_null());
    assert_eq!(fetched_opaque["raw_rdata"], "\\# 6 cafe01020304");
    assert!(fetched_opaque.get("kind").is_none());

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_host_detail_query_budget_stays_batched() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(ctx) = postgres_ctx("postgres_host_detail_query_budget_stays_batched").await else {
        return Ok(());
    };

    let host = ctx.host("budget-host");
    let cidr_a = ctx.cidr(41);
    let cidr_b = ctx.cidr(42);
    let ip_a = ctx.ip_in_cidr(&cidr_a, 20);
    let ip_b = ctx.ip_in_cidr(&cidr_b, 21);
    let policy = ctx.name("budget-policy");
    let community = ctx.name("budget-community");
    let contact = format!("{}@example.test", ctx.name("budget-ops"));
    let group = ctx.name("budget-group");
    let atom = ctx.name("budget-atom");
    let role = ctx.name("budget-role");

    ctx.seed_network(&cidr_a).await;
    ctx.seed_network(&cidr_b).await;
    ctx.seed_host(&host).await;

    let (status, attachment_a) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({
                "network": cidr_a,
                "mac_address": "aa:bb:cc:dd:ee:01"
            }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_a_id = attachment_a["id"]
        .as_str()
        .expect("attachment id")
        .to_string();

    let (status, attachment_b) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({
                "network": cidr_b,
                "mac_address": "aa:bb:cc:dd:ee:02"
            }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_b_id = attachment_b["id"]
        .as_str()
        .expect("attachment id")
        .to_string();

    assert_eq!(
        ctx.post(
            &format!("/inventory/attachments/{attachment_a_id}/ip-addresses"),
            json!({ "address": ip_a }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            &format!("/inventory/attachments/{attachment_b_id}/ip-addresses"),
            json!({ "address": ip_b }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );

    for (attachment_id, value) in [
        (attachment_a_id.as_str(), "01:aa:bb:cc:dd:ee:01"),
        (attachment_b_id.as_str(), "01:aa:bb:cc:dd:ee:02"),
    ] {
        assert_eq!(
            ctx.post(
                &format!("/inventory/attachments/{attachment_id}/dhcp-identifiers"),
                json!({
                    "family": 4,
                    "kind": "client_id",
                    "value": value,
                    "priority": 10
                }),
            )
            .await,
            actix_web::http::StatusCode::CREATED
        );
    }

    assert_eq!(
        ctx.post(
            "/policy/network/policies",
            json!({ "name": policy, "description": "budget policy" }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/network/communities",
            json!({
                "policy_name": policy,
                "network": cidr_a,
                "name": community,
                "description": "budget community"
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/network/attachment-community-assignments",
            json!({
                "attachment_id": attachment_a_id,
                "policy_name": policy,
                "community_name": community
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );

    assert_eq!(
        ctx.post(
            "/inventory/host-contacts",
            json!({
                "email": contact,
                "display_name": "Budget Ops",
                "hosts": [host]
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/inventory/host-groups",
            json!({
                "name": group,
                "description": "budget group",
                "hosts": [host]
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/inventory/bacnet-ids",
            json!({ "bacnet_id": ctx.bacnet_id(2), "host_name": host }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/dns/records",
            json!({
                "type_name": "TXT",
                "owner_kind": "host",
                "owner_name": host,
                "ttl": 300,
                "data": { "value": "budget" }
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/host/atoms",
            json!({ "name": atom, "description": "budget atom" }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/host/roles",
            json!({ "name": role, "description": "budget role" }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            &format!("/policy/host/roles/{role}/hosts/{host}"),
            json!({}),
        )
        .await,
        actix_web::http::StatusCode::NO_CONTENT
    );
    assert_eq!(
        ctx.post(
            &format!("/policy/host/roles/{role}/atoms/{atom}"),
            json!({}),
        )
        .await,
        actix_web::http::StatusCode::NO_CONTENT
    );

    let _ = ctx.get_status("/system/version").await;

    let (_body, queries) = ctx
        .get_json_with_query_capture(
            &format!("/inventory/hosts/{host}"),
            "host-detail-query-budget",
        )
        .await;
    let effective_queries: usize = queries
        .query_counts()
        .iter()
        .filter(|(query, _)| {
            !query.starts_with("INSERT INTO \"export_templates\"")
                && !query.starts_with("INSERT INTO \"record_types\"")
                && !query.starts_with("SELECT 1 -- binds: []")
        })
        .map(|(_, count)| *count)
        .sum();

    assert!(
        effective_queries <= 20,
        "host detail query budget exceeded: {:?}",
        queries.query_counts()
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_dhcp_identifiers"),
        1
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_prefix_reservations"),
        1
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_community_assignments"),
        1
    );

    let (body, queries) = ctx
        .get_json_with_query_capture(
            &format!("/inventory/hosts?name={host}"),
            "host-list-detail-query-budget",
        )
        .await;
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["items"][0]["attachments"].as_array().map(Vec::len),
        Some(2)
    );

    let effective_queries: usize = queries
        .query_counts()
        .iter()
        .filter(|(query, _)| {
            !query.starts_with("INSERT INTO \"export_templates\"")
                && !query.starts_with("INSERT INTO \"record_types\"")
                && !query.starts_with("SELECT 1 -- binds: []")
        })
        .map(|(_, count)| *count)
        .sum();

    assert!(
        effective_queries <= 4,
        "host list detail query budget exceeded: {:?}",
        queries.query_counts()
    );
    assert_eq!(queries.queries_matching("FROM host_attachments"), 1);
    assert_eq!(
        queries.queries_matching("FROM attachment_dhcp_identifiers"),
        1
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_prefix_reservations"),
        1
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_community_assignments"),
        1
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_network_detail_query_budget_stays_batched()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_network_detail_query_budget_stays_batched").await else {
        return Ok(());
    };

    let cidr = ctx.cidr(43);
    let host_a = ctx.host("budget-net-a");
    let host_b = ctx.host("budget-net-b");

    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host_a).await;
    ctx.seed_host(&host_b).await;

    let (status, attachment_a) = ctx
        .post_json(
            &format!("/inventory/hosts/{host_a}/attachments"),
            json!({
                "network": cidr,
                "mac_address": "aa:bb:cc:dd:ee:11"
            }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_a_id = attachment_a["id"].as_str().expect("attachment id");

    let (status, attachment_b) = ctx
        .post_json(
            &format!("/inventory/hosts/{host_b}/attachments"),
            json!({
                "network": cidr,
                "mac_address": "aa:bb:cc:dd:ee:12"
            }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_b_id = attachment_b["id"].as_str().expect("attachment id");

    assert_eq!(
        ctx.post(
            &format!("/inventory/attachments/{attachment_a_id}/ip-addresses"),
            json!({ "address": ctx.ip_in_cidr(&cidr, 30) }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            &format!("/inventory/attachments/{attachment_b_id}/ip-addresses"),
            json!({ "address": ctx.ip_in_cidr(&cidr, 31) }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );

    for (attachment_id, value) in [
        (attachment_a_id, "01:aa:bb:cc:dd:ee:11"),
        (attachment_b_id, "01:aa:bb:cc:dd:ee:12"),
    ] {
        assert_eq!(
            ctx.post(
                &format!("/inventory/attachments/{attachment_id}/dhcp-identifiers"),
                json!({
                    "family": 4,
                    "kind": "client_id",
                    "value": value,
                    "priority": 10
                }),
            )
            .await,
            actix_web::http::StatusCode::CREATED
        );
    }

    let _ = ctx.get_status("/system/version").await;

    let (_body, queries) = ctx
        .get_json_with_query_capture(
            &format!("/inventory/networks/{cidr}"),
            "network-detail-query-budget",
        )
        .await;
    let effective_queries: usize = queries
        .query_counts()
        .iter()
        .filter(|(query, _)| {
            !query.starts_with("INSERT INTO \"export_templates\"")
                && !query.starts_with("INSERT INTO \"record_types\"")
                && !query.starts_with("SELECT 1 -- binds: []")
        })
        .map(|(_, count)| *count)
        .sum();

    assert!(
        effective_queries <= 18,
        "network detail query budget exceeded: {:?}",
        queries.query_counts()
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_dhcp_identifiers"),
        1
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_prefix_reservations"),
        1
    );
    assert_eq!(
        queries.queries_matching("FROM attachment_community_assignments"),
        1
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_revoked_token_persists_across_fresh_contexts()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_revoked_token_persists_across_fresh_contexts").await
    else {
        return Ok(());
    };

    let fingerprint = ctx.name("revoked-token");
    let principal_key = format!("mreg::local::{}", ctx.name("principal"));
    let expires_at = Utc::now() + Duration::hours(1);

    ctx.storage()
        .auth_sessions()
        .revoke_token(fingerprint.clone(), principal_key.clone(), expires_at)
        .await?;

    let fresh = postgres_ctx("postgres_revoked_token_persists_across_fresh_contexts-fresh")
        .await
        .expect("fresh postgres ctx");
    assert!(
        fresh
            .storage()
            .auth_sessions()
            .is_token_revoked(&fingerprint)
            .await?
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_logout_all_cutoff_persists_across_fresh_contexts()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_logout_all_cutoff_persists_across_fresh_contexts").await
    else {
        return Ok(());
    };

    let principal_key = format!("mreg::local::{}", ctx.name("principal"));
    // PostgreSQL TIMESTAMPTZ has microsecond precision; the storage layer
    // truncates nanoseconds before persisting. Pre-truncate here so the
    // round-trip comparison can be exact.
    let cutoff = DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .expect("cutoff must round-trip via micros");

    ctx.storage()
        .auth_sessions()
        .revoke_all_for_principal(principal_key.clone(), cutoff)
        .await?;

    let fresh = postgres_ctx("postgres_logout_all_cutoff_persists_across_fresh_contexts-fresh")
        .await
        .expect("fresh postgres ctx");
    let stored = fresh
        .storage()
        .auth_sessions()
        .principal_revoked_before(&principal_key)
        .await?;
    assert_eq!(stored, Some(cutoff));

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_label_sorting_persists_across_fresh_contexts()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_label_sorting_persists_across_fresh_contexts").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    let label_a = ctx.name("alpha");
    let label_b = ctx.name("beta");
    let label_c = ctx.name("gamma");
    for name in [&label_a, &label_b, &label_c] {
        let status = ctx
            .post(
                "/inventory/labels",
                json!({ "name": name, "description": format!("label {name}") }),
            )
            .await;
        assert_eq!(status, actix_web::http::StatusCode::CREATED);
    }

    let descending: Vec<String> = storage
        .labels()
        .list_labels(&PageRequest {
            after: None,
            limit: Some(u64::MAX),
            sort_by: Some("name".to_string()),
            sort_dir: Some(mreg_rust::domain::pagination::SortDirection::Desc),
        })
        .await?
        .items
        .into_iter()
        .map(|label| label.name().as_str().to_string())
        .filter(|name| name.contains(ctx.namespace()))
        .collect();
    assert_eq!(
        descending,
        vec![label_c.clone(), label_b.clone(), label_a.clone()]
    );

    let fresh = postgres_ctx("postgres_label_sorting_persists_across_fresh_contexts-fresh")
        .await
        .expect("fresh postgres context");
    let descending: Vec<String> = fresh
        .storage()
        .labels()
        .list_labels(&PageRequest {
            after: None,
            limit: Some(u64::MAX),
            sort_by: Some("name".to_string()),
            sort_dir: Some(mreg_rust::domain::pagination::SortDirection::Desc),
        })
        .await?
        .items
        .into_iter()
        .map(|label| label.name().as_str().to_string())
        .filter(|name| name.contains(ctx.namespace()))
        .collect();
    assert_eq!(descending, vec![label_c, label_b, label_a]);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_host_filter_and_sort_use_sql() -> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_host_filter_and_sort_use_sql").await else {
        return Ok(());
    };

    let zone = ctx.zone("filter-zone");
    let nameserver = ctx.nameserver("ns1", &zone);
    ctx.seed_zone(&zone, &nameserver).await;

    let host_a = ctx.host_in_zone("filter-a", &zone);
    let host_b = ctx.host_in_zone("filter-b", &zone);
    let host_other = ctx.host_in_zone("other", &zone);

    for (name, comment) in [
        (&host_a, "sql-cluster alpha"),
        (&host_b, "sql-cluster omega"),
        (&host_other, "something else"),
    ] {
        let status = ctx
            .post(
                "/inventory/hosts",
                json!({ "name": name, "zone": zone, "comment": comment }),
            )
            .await;
        assert_eq!(status, actix_web::http::StatusCode::CREATED);
    }

    let body = ctx
        .get_json(
            &format!(
                "/inventory/hosts?zone={zone}&comment__contains=sql-cluster&sort_by=comment&sort_dir=desc&limit=10"
            ),
        )
        .await;
    let names: Vec<&str> = body["items"]
        .as_array()
        .expect("host list items")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .filter(|name| name.contains(ctx.namespace()))
        .collect();
    assert_eq!(names, vec![host_b.as_str(), host_a.as_str()]);

    let fresh = postgres_ctx("postgres_host_filter_and_sort_use_sql-fresh")
        .await
        .expect("fresh postgres context");
    let body = fresh
        .get_json(
            &format!(
                "/inventory/hosts?zone={zone}&comment__contains=sql-cluster&sort_by=comment&sort_dir=desc&limit=10"
            ),
        )
        .await;
    let names: Vec<&str> = body["items"]
        .as_array()
        .expect("host list items")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .filter(|name| name.contains(ctx.namespace()))
        .collect();
    assert_eq!(names, vec![host_b.as_str(), host_a.as_str()]);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_network_filter_and_sort_use_sql() -> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_network_filter_and_sort_use_sql").await else {
        return Ok(());
    };

    let cidr_a = ctx.cidr(51);
    let cidr_b = ctx.cidr(52);
    let cidr_other = ctx.cidr(53);
    let prod_alpha = format!("{} prod alpha", ctx.namespace());
    let prod_omega = format!("{} prod omega", ctx.namespace());
    let guest_access = format!("{} guest access", ctx.namespace());
    for (cidr, description) in [
        (&cidr_a, prod_alpha.as_str()),
        (&cidr_b, prod_omega.as_str()),
        (&cidr_other, guest_access.as_str()),
    ] {
        let status = ctx
            .post(
                "/inventory/networks",
                json!({ "cidr": cidr, "description": description }),
            )
            .await;
        assert_eq!(status, actix_web::http::StatusCode::CREATED);
    }

    let body = ctx
        .get_json(
            &format!(
                "/inventory/networks?description__contains={}&sort_by=description&sort_dir=desc&limit=10",
                ctx.namespace()
            ),
        )
        .await;
    let cidrs: Vec<&str> = body["items"]
        .as_array()
        .expect("network list items")
        .iter()
        .filter_map(|item| item["cidr"].as_str())
        .filter(|cidr| *cidr == cidr_a || *cidr == cidr_b)
        .collect();
    assert_eq!(cidrs, vec![cidr_b.as_str(), cidr_a.as_str()]);

    let fresh = postgres_ctx("postgres_network_filter_and_sort_use_sql-fresh")
        .await
        .expect("fresh postgres context");
    let body = fresh
        .get_json(
            &format!(
                "/inventory/networks?description__contains={}&sort_by=description&sort_dir=desc&limit=10",
                ctx.namespace()
            ),
        )
        .await;
    let cidrs: Vec<&str> = body["items"]
        .as_array()
        .expect("network list items")
        .iter()
        .filter_map(|item| item["cidr"].as_str())
        .filter(|cidr| *cidr == cidr_a || *cidr == cidr_b)
        .collect();
    assert_eq!(cidrs, vec![cidr_b.as_str(), cidr_a.as_str()]);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_community_and_assignment_filters_use_sql()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_community_and_assignment_filters_use_sql").await else {
        return Ok(());
    };

    let policy = ctx.name("sql-policy");
    let cidr_a = ctx.cidr(61);
    let cidr_b = ctx.cidr(62);
    let host = ctx.host("sql-assigned");

    ctx.seed_network(&cidr_a).await;
    ctx.seed_network(&cidr_b).await;
    ctx.seed_host(&host).await;

    assert_eq!(
        ctx.post(
            "/policy/network/policies",
            json!({ "name": policy, "description": "sql policy" }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );

    let community_alpha = ctx.name("alpha-community");
    let community_beta = ctx.name("beta-community");
    for (network, name) in [(&cidr_a, &community_alpha), (&cidr_b, &community_beta)] {
        assert_eq!(
            ctx.post(
                "/policy/network/communities",
                json!({
                    "policy_name": policy,
                    "network": network,
                    "name": name,
                    "description": format!("community {name}")
                }),
            )
            .await,
            actix_web::http::StatusCode::CREATED
        );
    }

    let (status, attachment_a) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({ "network": cidr_a, "mac_address": "aa:bb:cc:dd:ee:61" }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_a = attachment_a["id"].as_str().expect("attachment id");

    let (status, attachment_b) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({ "network": cidr_b, "mac_address": "aa:bb:cc:dd:ee:62" }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_b = attachment_b["id"].as_str().expect("attachment id");

    for (attachment_id, community_name) in [
        (attachment_a, community_alpha.as_str()),
        (attachment_b, community_beta.as_str()),
    ] {
        assert_eq!(
            ctx.post(
                "/policy/network/attachment-community-assignments",
                json!({
                    "attachment_id": attachment_id,
                    "policy_name": policy,
                    "community_name": community_name
                }),
            )
            .await,
            actix_web::http::StatusCode::CREATED
        );
    }

    let body = ctx
        .get_json(&format!(
            "/policy/network/communities?policy_name={policy}&sort_by=name&sort_dir=desc&limit=10"
        ))
        .await;
    let community_names: Vec<&str> = body["items"]
        .as_array()
        .expect("community items")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .filter(|name| name.contains(ctx.namespace()))
        .collect();
    assert_eq!(
        community_names,
        vec![community_beta.as_str(), community_alpha.as_str()]
    );

    let body = ctx
        .get_json(&format!(
            "/policy/network/attachment-community-assignments?host={host}&sort_by=community_name&sort_dir=desc&limit=10"
        ))
        .await;
    let assignment_names: Vec<&str> = body["items"]
        .as_array()
        .expect("assignment items")
        .iter()
        .filter_map(|item| item["community_name"].as_str())
        .filter(|name| name.contains(ctx.namespace()))
        .collect();
    assert_eq!(
        assignment_names,
        vec![community_beta.as_str(), community_alpha.as_str()]
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_delete_host_cascades_attachment_graph() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(ctx) = postgres_ctx("postgres_delete_host_cascades_attachment_graph").await else {
        return Ok(());
    };
    let storage = ctx.storage();

    let host = ctx.host("cascade-host");
    let cidr = ctx.cidr(71);
    let v6_token = ctx.bacnet_id(71);
    let v6_cidr = format!(
        "2001:db8:{:x}:{:x}::/64",
        (v6_token >> 16) & 0xffff,
        v6_token & 0xffff
    );
    let v6_prefix = format!(
        "2001:db8:{:x}:{:x}::/120",
        (v6_token >> 16) & 0xffff,
        v6_token & 0xffff
    );
    let address = ctx.ip_in_cidr(&cidr, 21);
    let policy = ctx.name("cascade-policy");
    let community = ctx.name("cascade-community");

    ctx.seed_network(&cidr).await;
    ctx.seed_network(&v6_cidr).await;
    ctx.seed_host(&host).await;

    let (status, attachment) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({ "network": cidr, "mac_address": "aa:bb:cc:dd:ee:71" }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_id = attachment["id"]
        .as_str()
        .expect("attachment id")
        .to_string();
    let (status, v6_attachment) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({ "network": v6_cidr }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let v6_attachment_id = v6_attachment["id"]
        .as_str()
        .expect("attachment id")
        .to_string();

    assert_eq!(
        ctx.post(
            &format!("/inventory/attachments/{attachment_id}/ip-addresses"),
            json!({ "address": address }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            &format!("/inventory/attachments/{attachment_id}/dhcp-identifiers"),
            json!({
                "family": 4,
                "kind": "client_id",
                "value": "01:aa:bb:cc:dd:ee:71",
                "priority": 10
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            &format!("/inventory/attachments/{v6_attachment_id}/prefix-reservations"),
            json!({ "prefix": v6_prefix }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/network/policies",
            json!({ "name": policy, "description": "cascade policy" }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/network/communities",
            json!({
                "policy_name": policy,
                "network": cidr,
                "name": community,
                "description": "cascade community"
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/network/attachment-community-assignments",
            json!({
                "attachment_id": attachment_id,
                "policy_name": policy,
                "community_name": community
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );

    assert_eq!(
        ctx.delete(&format!("/inventory/hosts/{host}")).await,
        actix_web::http::StatusCode::NO_CONTENT
    );
    assert_eq!(
        ctx.get_status(&format!("/inventory/hosts/{host}")).await,
        actix_web::http::StatusCode::NOT_FOUND
    );
    assert_eq!(
        ctx.get_status(&format!("/inventory/attachments/{attachment_id}"))
            .await,
        actix_web::http::StatusCode::NOT_FOUND
    );
    assert_eq!(
        ctx.get_status(&format!("/inventory/attachments/{v6_attachment_id}"))
            .await,
        actix_web::http::StatusCode::NOT_FOUND
    );

    assert!(
        storage
            .attachments()
            .list_attachment_dhcp_identifiers_for_attachments(&[uuid::Uuid::parse_str(
                &attachment_id
            )?])
            .await?
            .is_empty()
    );
    assert!(
        storage
            .attachments()
            .list_attachment_prefix_reservations_for_attachments(&[uuid::Uuid::parse_str(
                &v6_attachment_id
            )?])
            .await?
            .is_empty()
    );
    let assignments = ctx
        .get_json(&format!(
            "/policy/network/attachment-community-assignments?host={host}"
        ))
        .await;
    assert!(
        assignments["items"]
            .as_array()
            .expect("assignment items")
            .is_empty()
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_network_delete_cascades_related_attachment_state()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_network_delete_cascades_related_attachment_state").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    let host = ctx.host("network-ref-host");
    let cidr = ctx.cidr(81);
    let encoded_cidr = cidr.replace("/", "%2F");
    let policy = ctx.name("network-delete-policy");
    let community = ctx.name("network-delete-community");

    ctx.seed_network(&cidr).await;
    ctx.seed_host(&host).await;

    let (status, attachment) = ctx
        .post_json(
            &format!("/inventory/hosts/{host}/attachments"),
            json!({ "network": cidr, "mac_address": "aa:bb:cc:dd:ee:81" }),
        )
        .await;
    assert_eq!(status, actix_web::http::StatusCode::CREATED);
    let attachment_id = attachment["id"]
        .as_str()
        .expect("attachment id")
        .to_string();

    assert_eq!(
        ctx.post(
            "/policy/network/policies",
            json!({ "name": policy, "description": "network delete policy" }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/network/communities",
            json!({
                "policy_name": policy,
                "network": cidr,
                "name": community,
                "description": "network delete community"
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );
    assert_eq!(
        ctx.post(
            "/policy/network/attachment-community-assignments",
            json!({
                "attachment_id": attachment_id,
                "policy_name": policy,
                "community_name": community
            }),
        )
        .await,
        actix_web::http::StatusCode::CREATED
    );

    assert_eq!(
        ctx.delete(&format!("/inventory/networks/{encoded_cidr}"))
            .await,
        actix_web::http::StatusCode::NO_CONTENT
    );
    assert_eq!(
        ctx.get_status(&format!("/inventory/networks/{encoded_cidr}"))
            .await,
        actix_web::http::StatusCode::NOT_FOUND
    );
    assert_eq!(
        ctx.get_status(&format!("/inventory/attachments/{attachment_id}"))
            .await,
        actix_web::http::StatusCode::NOT_FOUND
    );
    assert!(
        storage
            .attachment_community_assignments()
            .list_attachment_community_assignments(
                &PageRequest::all(),
                &AttachmentCommunityAssignmentFilter::from_query_params(
                    std::collections::HashMap::from([("host".to_string(), host.clone())]),
                )?,
            )
            .await?
            .items
            .is_empty()
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_builtin_bootstrap_is_idempotent_across_fresh_contexts()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) =
        postgres_ctx("postgres_builtin_bootstrap_is_idempotent_across_fresh_contexts").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();

    let record_types_first = storage
        .records()
        .list_record_types(&PageRequest::all())
        .await?;
    let export_templates_first = storage
        .exports()
        .list_export_templates(&PageRequest::all())
        .await?;

    let record_type_names_first = record_types_first
        .items
        .iter()
        .map(|item| item.name().as_str().to_string())
        .collect::<HashSet<_>>();
    let export_template_names_first = export_templates_first
        .items
        .iter()
        .map(|item| item.name().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(
        record_type_names_first.len(),
        record_types_first.items.len()
    );
    assert_eq!(
        export_template_names_first.len(),
        export_templates_first.items.len()
    );

    let fresh =
        postgres_ctx("postgres_builtin_bootstrap_is_idempotent_across_fresh_contexts-fresh")
            .await
            .expect("fresh postgres context");
    let fresh_storage = fresh.storage();
    let record_types_second = fresh_storage
        .records()
        .list_record_types(&PageRequest::all())
        .await?;
    let export_templates_second = fresh_storage
        .exports()
        .list_export_templates(&PageRequest::all())
        .await?;

    let record_type_names_second = record_types_second
        .items
        .iter()
        .map(|item| item.name().as_str().to_string())
        .collect::<HashSet<_>>();
    let export_template_names_second = export_templates_second
        .items
        .iter()
        .map(|item| item.name().to_string())
        .collect::<HashSet<_>>();

    assert_eq!(record_type_names_first, record_type_names_second);
    assert_eq!(export_template_names_first, export_template_names_second);
    assert_eq!(
        record_types_first.items.len(),
        record_types_second.items.len()
    );
    assert_eq!(
        export_templates_first.items.len(),
        export_templates_second.items.len()
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_auth_login_and_me_work_across_fresh_app_states()
-> Result<(), Box<dyn std::error::Error>> {
    let scope = "local-login";
    let login_username = format!("{scope}:admin");
    let principal_key = format!("mreg::{scope}::admin");
    let Some(state_a) = postgres_scoped_auth_state(scope, true).await else {
        eprintln!(
            "{}",
            common::postgres_skip_message(
                "postgres_auth_login_and_me_work_across_fresh_app_states"
            )
        );
        return Ok(());
    };

    let (status, body) = call_auth_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":login_username,"password":"secret"}))
            .to_request(),
        state_a.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"]
        .as_str()
        .expect("access token")
        .to_string();

    let (status, body) = call_auth_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        state_a,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["principal"]["id"], "admin");
    assert_eq!(body["principal"]["namespace"], json!(["mreg", scope]));
    assert_eq!(body["principal"]["key"], principal_key);

    let fresh = postgres_scoped_auth_state(scope, true)
        .await
        .expect("fresh postgres auth state");
    let (status, body) = call_auth_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        fresh,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["principal"]["id"], "admin");
    assert_eq!(body["principal"]["namespace"], json!(["mreg", scope]));
    assert_eq!(body["principal"]["key"], principal_key);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_auth_logout_revokes_token_across_fresh_app_states()
-> Result<(), Box<dyn std::error::Error>> {
    let scope = "local-logout";
    let login_username = format!("{scope}:admin");
    let Some(state_a) = postgres_scoped_auth_state(scope, true).await else {
        eprintln!(
            "{}",
            common::postgres_skip_message(
                "postgres_auth_logout_revokes_token_across_fresh_app_states"
            )
        );
        return Ok(());
    };

    let (status, body) = call_auth_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":login_username,"password":"secret"}))
            .to_request(),
        state_a.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"]
        .as_str()
        .expect("access token")
        .to_string();

    let (status, body) = call_auth_json(
        test::TestRequest::post()
            .uri("/auth/logout")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        state_a,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    let fresh = postgres_scoped_auth_state(scope, true)
        .await
        .expect("fresh postgres auth state");
    let (status, body) = call_auth_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        fresh,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_auth_logout_all_revokes_token_across_fresh_app_states()
-> Result<(), Box<dyn std::error::Error>> {
    let scope = "local-logout-all";
    let login_username = format!("{scope}:admin");
    let principal_key = format!("mreg::{scope}::admin");
    let Some(state_a) = postgres_scoped_auth_state(scope, true).await else {
        eprintln!(
            "{}",
            common::postgres_skip_message(
                "postgres_auth_logout_all_revokes_token_across_fresh_app_states"
            )
        );
        return Ok(());
    };

    let (status, body) = call_auth_json(
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"username":login_username,"password":"secret"}))
            .to_request(),
        state_a.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let access_token = body["access_token"]
        .as_str()
        .expect("access token")
        .to_string();

    let (status, body) = call_auth_json(
        test::TestRequest::post()
            .uri("/auth/logout-all")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .set_json(json!({ "principal_key": principal_key }))
            .to_request(),
        state_a,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    let fresh = postgres_scoped_auth_state(scope, true)
        .await
        .expect("fresh postgres auth state");
    let (status, body) = call_auth_json(
        test::TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {access_token}")))
            .to_request(),
        fresh,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn postgres_task_idempotency_key_race_allows_only_one_create()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(ctx) = postgres_ctx("postgres_task_idempotency_key_race_allows_only_one_create").await
    else {
        return Ok(());
    };
    let storage = ctx.storage();
    let task_kind = ctx.name("test-race");
    let idempotency_key = ctx.name("idempotency");

    let store_a = storage.clone();
    let kind_a = task_kind.clone();
    let key_a = idempotency_key.clone();
    let create_a = tokio::spawn(async move {
        store_a
            .tasks()
            .create_task(CreateTask::new(
                kind_a,
                Some("tester".to_string()),
                json!({"slot":"a"}),
                Some(key_a),
                3,
            )?)
            .await
    });

    let store_b = storage.clone();
    let kind_b = task_kind.clone();
    let key_b = idempotency_key.clone();
    let create_b = tokio::spawn(async move {
        store_b
            .tasks()
            .create_task(CreateTask::new(
                kind_b,
                Some("tester".to_string()),
                json!({"slot":"b"}),
                Some(key_b),
                3,
            )?)
            .await
    });

    let (result_a, result_b) = tokio::join!(create_a, create_b);
    let result_a = result_a.expect("join create a");
    let result_b = result_b.expect("join create b");

    let success_count = [result_a.as_ref(), result_b.as_ref()]
        .into_iter()
        .filter(|result| result.is_ok())
        .count();
    let conflict_count = [result_a.as_ref(), result_b.as_ref()]
        .into_iter()
        .filter(|result| matches!(result, Err(AppError::Conflict(_))))
        .count();

    assert_eq!(success_count, 1);
    assert_eq!(conflict_count, 1);

    let tasks = storage.tasks().list_tasks(&PageRequest::all()).await?;
    let matching: Vec<_> = tasks
        .items
        .into_iter()
        .filter(|task| task.kind() == task_kind)
        .collect();
    assert_eq!(matching.len(), 1);

    Ok(())
}
