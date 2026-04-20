#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

use mreg_rust::{
    AppState, BuildInfo,
    authn::{AuthnClient, LocalJwtIssuer},
    authz::{AttrValue, AuthorizerClient, Group, Principal},
    config::{AuthMode, Config, StorageBackendSetting},
    domain::{
        attachment::{
            CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, DhcpIdentifierFamily,
            DhcpIdentifierKind,
        },
        builtin_export_templates::built_in_export_templates,
        builtin_types::built_in_record_types,
        community::CreateCommunity,
        exports::ExportTemplate,
        filters::HostFilter,
        host::{AssignIpAddress, CreateHost, HostAuthContext},
        imports::{CreateImportBatch, ImportBatch, ImportItem, ImportKind, ImportOperation},
        label::CreateLabel,
        nameserver::CreateNameServer,
        network::CreateNetwork,
        network_policy::CreateNetworkPolicy,
        pagination::{PageRequest, SortDirection},
        resource_records::{
            CreateRecordInstance, RecordInstance, RecordOwnerKind, RecordTypeDefinition,
        },
        types::{
            CidrValue, CommunityName, DhcpPriority, DnsName, EmailAddressValue, Hostname,
            IpAddressValue, LabelName, MacAddressValue, NetworkPolicyName, RecordTypeName,
            ReservedCount, SerialNumber, SoaSeconds, Ttl, ZoneName,
        },
        zone::{CreateForwardZone, CreateReverseZone},
    },
    events::EventSinkClient,
    services::Services,
    storage::{DynStorage, ReadableStorage, build_storage},
};

pub const BENCH_JWT_KEY: &str = "this_is_exactly_32_bytes_long_!!";
pub const BENCH_JWT_ISSUER: &str = "mreg-bench";

pub fn runtime() -> Runtime {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime should build")
}

pub fn record_type(name: &str) -> RecordTypeDefinition {
    let definition = built_in_record_types()
        .expect("built-in record types should load")
        .into_iter()
        .find(|item| item.name().as_str() == name)
        .expect("requested built-in record type should exist");

    RecordTypeDefinition::restore(
        Uuid::nil(),
        definition.name().clone(),
        definition.dns_type(),
        definition.schema().clone(),
        definition.built_in(),
        Utc::now(),
        Utc::now(),
    )
}

pub fn host_listing_scenario(
    runtime: &Runtime,
    host_count: usize,
) -> (DynStorage, PageRequest, HostFilter) {
    let storage = memory_storage();

    runtime.block_on(async {
        storage
            .networks()
            .create_network(
                CreateNetwork::new(
                    CidrValue::new("10.10.0.0/20").expect("valid CIDR"),
                    "bench inventory network",
                    ReservedCount::new(1).expect("valid reserved count"),
                )
                .expect("valid network"),
            )
            .await
            .expect("network should be created");

        for index in 0..host_count {
            let host = Hostname::new(format!("host-{index:04}.bench.test"))
                .expect("valid benchmark hostname");
            storage
                .hosts()
                .create_host(
                    CreateHost::new(host.clone(), None, None, "bench host")
                        .expect("valid benchmark host"),
                )
                .await
                .expect("host should be created");

            let third_octet = (index / 250) as u8;
            let fourth_octet = (index % 250 + 2) as u8;
            let address = IpAddressValue::new(format!("10.10.{third_octet}.{fourth_octet}"))
                .expect("valid benchmark address");
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(host, Some(address), None, None)
                        .expect("valid address assignment"),
                )
                .await
                .expect("address assignment should succeed");
        }
    });

    let page = PageRequest {
        after: None,
        limit: Some(100),
        sort_by: Some("name".to_string()),
        sort_dir: Some(SortDirection::Asc),
    };
    let filter = HostFilter::from_query_params(HashMap::from([
        ("address__contains".to_string(), "10.10.3".to_string()),
        ("comment__contains".to_string(), "bench".to_string()),
    ]))
    .expect("benchmark filter should parse");

    (storage, page, filter)
}

pub fn auto_allocation_scenario(
    runtime: &Runtime,
    existing_allocations: usize,
) -> (DynStorage, Hostname, CidrValue) {
    parametrized_allocation_scenario(runtime, "10.20.0.0/24", existing_allocations)
}

/// Parametrized allocation scenario: creates a network of arbitrary CIDR and
/// pre-allocates `existing_allocations` IPs, then leaves a pending host ready
/// to claim the next free address. Caller is responsible for keeping
/// `existing_allocations` within the network's host range.
pub fn parametrized_allocation_scenario(
    runtime: &Runtime,
    cidr: &str,
    existing_allocations: usize,
) -> (DynStorage, Hostname, CidrValue) {
    let storage = memory_storage();
    let network = CidrValue::new(cidr).expect("valid CIDR");
    let pending_host = Hostname::new(format!(
        "pending-{:x}.bench.test",
        Uuid::new_v4().as_u128() & 0xffff
    ))
    .expect("valid benchmark hostname");

    let net_inner = network.as_inner();
    let mut hosts_iter = net_inner.hosts();
    // Skip the first reserved address.
    let _ = hosts_iter.next();

    runtime.block_on(async {
        storage
            .networks()
            .create_network(
                CreateNetwork::new(
                    network.clone(),
                    "parametrized allocation network",
                    ReservedCount::new(1).expect("valid reserved count"),
                )
                .expect("valid network"),
            )
            .await
            .expect("network should be created");

        for index in 0..existing_allocations {
            let host = Hostname::new(format!("existing-{index:05}.bench.test"))
                .expect("valid benchmark hostname");
            storage
                .hosts()
                .create_host(
                    CreateHost::new(host.clone(), None, None, "existing benchmark host")
                        .expect("valid benchmark host"),
                )
                .await
                .expect("host should be created");

            let ip = hosts_iter
                .next()
                .expect("network must contain enough host addresses");
            let address = IpAddressValue::new(ip.to_string()).expect("valid benchmark address");
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(host, Some(address), None, None)
                        .expect("valid address assignment"),
                )
                .await
                .expect("address assignment should succeed");
        }

        storage
            .hosts()
            .create_host(
                CreateHost::new(pending_host.clone(), None, None, "pending benchmark host")
                    .expect("valid benchmark host"),
            )
            .await
            .expect("pending host should be created");
    });

    (storage, pending_host, network)
}

pub fn host_auth_context_scenario(
    runtime: &Runtime,
    assignment_count: usize,
    network_count: usize,
) -> (DynStorage, Hostname) {
    let storage = memory_storage();
    let host = Hostname::new(format!(
        "authz-{assignment_count:03}-{network_count:03}.bench.test"
    ))
    .expect("valid benchmark hostname");

    runtime.block_on(async {
        storage
            .hosts()
            .create_host(
                CreateHost::new(host.clone(), None, None, "authz benchmark host")
                    .expect("valid benchmark host"),
            )
            .await
            .expect("host should be created");

        for network_index in 0..network_count.max(1) {
            let cidr = CidrValue::new(format!("10.30.{network_index}.0/24"))
                .expect("valid benchmark CIDR");
            storage
                .networks()
                .create_network(
                    CreateNetwork::new(
                        cidr,
                        format!("authz bench network {network_index}"),
                        ReservedCount::new(1).expect("valid reserved count"),
                    )
                    .expect("valid benchmark network"),
                )
                .await
                .expect("network should be created");
        }

        for assignment_index in 0..assignment_count {
            let network_index = assignment_index % network_count.max(1);
            let address = IpAddressValue::new(format!(
                "10.30.{network_index}.{}",
                assignment_index / network_count.max(1) + 2
            ))
            .expect("valid benchmark address");
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(host.clone(), Some(address), None, None)
                        .expect("valid benchmark assignment"),
                )
                .await
                .expect("address assignment should succeed");
        }
    });

    (storage, host)
}

pub fn sample_host_auth_context(assignment_count: usize, network_count: usize) -> HostAuthContext {
    let host = mreg_rust::domain::host::Host::restore(
        Uuid::new_v4(),
        Hostname::new(format!(
            "sample-{assignment_count:03}-{network_count:03}.bench.test"
        ))
        .expect("valid benchmark hostname"),
        None,
        None,
        "sample benchmark host",
        Utc::now(),
        Utc::now(),
    )
    .expect("sample host should build");
    let addresses = (0..assignment_count)
        .map(|assignment_index| {
            let network_index = assignment_index % network_count.max(1);
            IpAddressValue::new(format!(
                "10.40.{network_index}.{}",
                assignment_index / network_count.max(1) + 2
            ))
            .expect("valid benchmark address")
        })
        .collect();
    let networks = (0..network_count)
        .map(|network_index| {
            CidrValue::new(format!("10.40.{network_index}.0/24")).expect("valid benchmark network")
        })
        .collect();

    HostAuthContext::new(host, addresses, networks)
}

pub fn host_auth_attrs(context: &HostAuthContext) -> BTreeMap<String, AttrValue> {
    let host = context.host();
    let mut attrs = BTreeMap::from([(
        "name".to_string(),
        AttrValue::String(host.name().as_str().to_string()),
    )]);
    if let Some(zone) = host.zone() {
        attrs.insert(
            "zone".to_string(),
            AttrValue::String(zone.as_str().to_string()),
        );
    }
    if !context.addresses().is_empty() {
        attrs.insert(
            "addresses".to_string(),
            AttrValue::Set(
                context
                    .addresses()
                    .iter()
                    .map(|address| AttrValue::String(address.as_str().to_string()))
                    .collect(),
            ),
        );
    }
    attrs.insert(
        "networks".to_string(),
        AttrValue::Set(
            context
                .networks()
                .iter()
                .map(|network| AttrValue::String(network.as_str().to_string()))
                .collect(),
        ),
    );
    attrs
}

pub fn memory_storage() -> DynStorage {
    build_storage(&Config {
        port: 0,
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: true,
        event_webhook_timeout_ms: 100,
        ..Config::default()
    })
    .expect("memory storage should initialize")
}

/// Build a default `AppState` backed by an empty in-memory storage. The state
/// uses the development authz bypass so handler-level auth checks succeed.
pub fn empty_app_state() -> AppState {
    let storage = memory_storage();
    let config = Config {
        port: 0,
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: true,
        event_webhook_timeout_ms: 100,
        ..Config::default()
    };
    let authn =
        AuthnClient::from_config(&config, storage.clone()).expect("authn client should build");
    let authz = AuthorizerClient::from_config(&config).expect("authz client should build");
    AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        reader: ReadableStorage::new(storage.clone()),
        services: Services::new(storage, EventSinkClient::noop()),
        authn,
        authz,
    }
}

/// Build an `AppState` reusing an existing storage (so seed data is visible
/// through the state's services).
pub fn app_state_for(storage: DynStorage) -> AppState {
    let config = Config {
        port: 0,
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: true,
        event_webhook_timeout_ms: 100,
        ..Config::default()
    };
    let authn =
        AuthnClient::from_config(&config, storage.clone()).expect("authn client should build");
    let authz = AuthorizerClient::from_config(&config).expect("authz client should build");
    AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        reader: ReadableStorage::new(storage.clone()),
        services: Services::new(storage, EventSinkClient::noop()),
        authn,
        authz,
    }
}

pub fn benchmark_principal() -> Principal {
    Principal {
        id: "bench-user".to_string(),
        namespace: vec!["mreg".to_string(), "bench".to_string()],
        groups: vec![Group {
            id: "ops".to_string(),
            namespace: vec!["mreg".to_string(), "bench".to_string()],
        }],
    }
}

pub fn jwt_issuer() -> LocalJwtIssuer {
    LocalJwtIssuer::new(BENCH_JWT_KEY, BENCH_JWT_ISSUER, 3600)
}

pub fn signed_token() -> String {
    let issuer = jwt_issuer();
    let principal = benchmark_principal();
    let (token, _) = issuer
        .issue_access_token(&principal, "bench-user", "local", "local", None)
        .expect("issue benchmark token");
    token
}

/// Build a scoped `AuthnClient` configured with the benchmark JWT key, so
/// `authenticate_bearer` accepts tokens issued by `signed_token()`.
pub fn scoped_authn_client() -> AuthnClient {
    let storage = memory_storage();
    let config = Config {
        port: 0,
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: true,
        event_webhook_timeout_ms: 100,
        auth_mode: AuthMode::Scoped,
        auth_jwt_signing_key: Some(BENCH_JWT_KEY.to_string()),
        auth_jwt_issuer: BENCH_JWT_ISSUER.to_string(),
        ..Config::default()
    };
    AuthnClient::from_config(&config, storage).expect("scoped authn client should build")
}

pub fn export_template(name: &str) -> ExportTemplate {
    let entry = built_in_export_templates()
        .expect("built-in export templates should load")
        .into_iter()
        .find(|(template, _)| template.name() == name)
        .expect("requested built-in export template should exist");
    let (command, builtin) = entry;
    ExportTemplate::restore(
        Uuid::nil(),
        command.name(),
        command.description(),
        command.engine(),
        command.scope(),
        command.body(),
        command.metadata().clone(),
        builtin,
    )
    .expect("export template should restore")
}

/// Minimal forward zone fixture matching the `bind-forward-zone` template
/// expectations (zone metadata + a small set of records).
pub fn forward_zone_template_data() -> Value {
    let records: Vec<Value> = (0..20)
        .map(|i| {
            json!({
                "owner_name": format!("host-{i:02}.bench.test."),
                "type_name": "A",
                "ttl": 3600,
                "rendered": format!("10.0.0.{i}"),
            })
        })
        .collect();
    json!({
        "zone": {
            "name": "bench.test",
            "primary_ns": "ns1.bench.test.",
            "email": "hostmaster.bench.test.",
            "serial_no": 2026042001u64,
            "refresh": 10800,
            "retry": 3600,
            "expire": 604800,
            "soa_ttl": 3600,
            "default_ttl": 3600,
            "nameservers": ["ns1.bench.test.", "ns2.bench.test."],
        },
        "records": records,
    })
}

/// Reverse zone fixture mirroring `bind-reverse-zone` template input.
pub fn reverse_zone_template_data() -> Value {
    let records: Vec<Value> = (0..20)
        .map(|i| {
            json!({
                "owner_name": format!("{i}.0.0.10.in-addr.arpa."),
                "type_name": "PTR",
                "ttl": 3600,
                "rendered": format!("host-{i:02}.bench.test."),
            })
        })
        .collect();
    json!({
        "zone": {
            "name": "0.0.10.in-addr.arpa",
            "primary_ns": "ns1.bench.test.",
            "email": "hostmaster.bench.test.",
            "serial_no": 2026042001u64,
            "refresh": 10800,
            "retry": 3600,
            "expire": 604800,
            "soa_ttl": 3600,
            "default_ttl": 3600,
            "nameservers": ["ns1.bench.test.", "ns2.bench.test."],
        },
        "records": records,
    })
}

/// DHCP fixture matching the shape consumed by the built-in Kea/ISC
/// templates: `dhcp4_networks` and `dhcp6_networks` with attachment lists.
pub fn dhcp_template_data() -> Value {
    let mut dhcp4_networks = Vec::new();
    let mut dhcp6_networks = Vec::new();
    for net_idx in 0..2 {
        let mut dhcp4_attachments = Vec::new();
        let mut dhcp6_attachments = Vec::new();
        for host_idx in 0..5 {
            let host_name = format!("host-{net_idx}-{host_idx}.bench.test");
            let ipv4 = format!("10.{net_idx}.0.{}", host_idx + 2);
            let ipv6 = format!("2001:db8:{net_idx}::{}", host_idx + 2);
            let mac = format!("aa:bb:cc:0{net_idx}:00:{host_idx:02x}");
            dhcp4_attachments.push(json!({
                "host_name": host_name,
                "primary_ipv4_address": ipv4,
                "matchers": {
                    "ipv4": {"kind": "hw_address", "value": mac},
                },
            }));
            dhcp6_attachments.push(json!({
                "host_name": host_name,
                "primary_ipv6_address": ipv6,
                "matchers": {
                    "ipv6": {
                        "kind": "duid",
                        "value": format!("00:03:00:01:aa:bb:cc:0{net_idx}:00:{host_idx:02x}"),
                    },
                },
                "prefix_reservations": [
                    {"prefix": format!("2001:db8:{net_idx}:{host_idx:x}::/64")},
                ],
            }));
        }
        dhcp4_networks.push(json!({
            "cidr": format!("10.{net_idx}.0.0/24"),
            "description": format!("bench v4 net {net_idx}"),
            "dhcp4_attachments": dhcp4_attachments,
        }));
        dhcp6_networks.push(json!({
            "cidr": format!("2001:db8:{net_idx}::/64"),
            "description": format!("bench v6 net {net_idx}"),
            "dhcp6_attachments": dhcp6_attachments,
        }));
    }
    json!({
        "dhcp4_networks": dhcp4_networks,
        "dhcp6_networks": dhcp6_networks,
    })
}

/// Sample record instances covering the four record-kind projection variants:
/// built-in typed, opaque custom, raw-RDATA fallback, and malformed payload.
pub fn record_kind_samples() -> Vec<RecordInstance> {
    let now = Utc::now();
    let owner = DnsName::new("kind.bench.test").expect("valid owner");

    let typed = RecordInstance::restore(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        RecordTypeName::new("A").expect("type name"),
        Some(RecordOwnerKind::Host),
        None,
        owner.clone(),
        None,
        Some(Ttl::new(3600).expect("ttl")),
        json!({"address": "10.10.0.10"}),
        None,
        Some("10.10.0.10".to_string()),
        now,
        now,
    );
    let opaque = RecordInstance::restore(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        RecordTypeName::new("CUSTOM").expect("type name"),
        None,
        None,
        owner.clone(),
        None,
        None,
        json!({"value": "opaque"}),
        None,
        None,
        now,
        now,
    );
    let raw_rdata = RecordInstance::restore(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        RecordTypeName::new("TXT").expect("type name"),
        Some(RecordOwnerKind::Host),
        None,
        owner.clone(),
        None,
        None,
        Value::Null,
        None,
        Some("\"raw rdata bench value\"".to_string()),
        now,
        now,
    );
    let malformed = RecordInstance::restore(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        RecordTypeName::new("A").expect("type name"),
        Some(RecordOwnerKind::Host),
        None,
        owner,
        None,
        None,
        json!({"address": "not-an-ip-address"}),
        None,
        None,
        now,
        now,
    );

    vec![typed, opaque, raw_rdata, malformed]
}

/// Build storage seeded with `count` A records under a single zone, plus a
/// page request that pulls 100 records ordered by id.
pub fn record_listing_storage(runtime: &Runtime, count: usize) -> (DynStorage, PageRequest) {
    let storage = memory_storage();

    runtime.block_on(async {
        for index in 0..count {
            let owner = format!("rec-{index:04}.bench.test");
            let cmd = CreateRecordInstance::new_unanchored(
                RecordTypeName::new("A").expect("type name"),
                owner,
                None,
                json!({"address": format!("10.50.{}.{}", (index / 250) % 250, (index % 250) + 1)}),
            )
            .expect("valid record");
            storage
                .records()
                .create_record(cmd)
                .await
                .expect("record should be created");
        }
    });

    let page = PageRequest {
        after: None,
        limit: Some(100),
        sort_by: None,
        sort_dir: Some(SortDirection::Asc),
    };
    (storage, page)
}

/// Storage seeded with a host that owns `ip_count` A records on a single
/// network, ready for cascade-delete benches.
pub fn ptr_cascade_storage(runtime: &Runtime, ip_count: usize) -> (DynStorage, Hostname) {
    let storage = memory_storage();
    let host = Hostname::new("cascade.bench.test").expect("hostname");

    runtime.block_on(async {
        storage
            .networks()
            .create_network(
                CreateNetwork::new(
                    CidrValue::new("10.60.0.0/16").expect("CIDR"),
                    "cascade bench network",
                    ReservedCount::new(1).expect("reserved count"),
                )
                .expect("network"),
            )
            .await
            .expect("network create");

        storage
            .hosts()
            .create_host(CreateHost::new(host.clone(), None, None, "cascade host").expect("host"))
            .await
            .expect("host create");

        for index in 0..ip_count {
            let address = IpAddressValue::new(format!(
                "10.60.{}.{}",
                (index / 250) % 250,
                (index % 250) + 1
            ))
            .expect("address");
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(host.clone(), Some(address), None, None)
                        .expect("assignment"),
                )
                .await
                .expect("assignment ok");
        }
    });

    (storage, host)
}

/// Storage seeded with a host that has multiple attachments, each with a
/// DHCP identifier, prefix reservation, and community assignment. Returns the
/// host name and the full attachment graph stays in storage ready for delete.
pub fn attachment_graph_storage(runtime: &Runtime) -> (DynStorage, Hostname) {
    let storage = memory_storage();
    let host = Hostname::new("attach.bench.test").expect("hostname");

    runtime.block_on(async {
        // Policy + community used by attachment community assignments.
        let policy = storage
            .network_policies()
            .create_network_policy(
                CreateNetworkPolicy::new(
                    NetworkPolicyName::new("bench-policy").expect("policy name"),
                    "bench attachment graph policy",
                    None,
                )
                .expect("policy command"),
            )
            .await
            .expect("policy create");
        let _ = policy;

        // Two networks, one v4 and one v6 (so prefix reservations are valid).
        let v4 = CidrValue::new("10.70.0.0/24").expect("v4 cidr");
        let v6 = CidrValue::new("2001:db8:70::/48").expect("v6 cidr");
        for cidr in [v4.clone(), v6.clone()] {
            storage
                .networks()
                .create_network(
                    CreateNetwork::new(
                        cidr,
                        "attachment graph bench network",
                        ReservedCount::new(0).expect("reserved count"),
                    )
                    .expect("network"),
                )
                .await
                .expect("network create");
        }

        for (community_cidr, name) in [
            (v4.clone(), "bench-community-v4"),
            (v6.clone(), "bench-community-v6"),
        ] {
            storage
                .communities()
                .create_community(
                    CreateCommunity::new(
                        NetworkPolicyName::new("bench-policy").expect("policy name"),
                        community_cidr,
                        CommunityName::new(name).expect("community"),
                        "attachment bench community",
                    )
                    .expect("community command"),
                )
                .await
                .expect("community create");
        }

        storage
            .hosts()
            .create_host(
                CreateHost::new(host.clone(), None, None, "attachment graph host").expect("host"),
            )
            .await
            .expect("host create");

        for net_index in 0..4u32 {
            // Alternate between v4 and v6 networks so both DHCP families and
            // prefix reservations are exercised.
            let on_v6 = net_index.is_multiple_of(2);
            let network = if on_v6 { v6.clone() } else { v4.clone() };

            let mac =
                MacAddressValue::new(format!("aa:bb:cc:dd:ee:{:02x}", net_index)).expect("mac");

            let attachment = storage
                .attachments()
                .create_attachment(CreateHostAttachment::new(
                    host.clone(),
                    network.clone(),
                    Some(mac),
                    Some(format!("bench attachment {net_index}")),
                ))
                .await
                .expect("attachment create");

            if on_v6 {
                storage
                    .attachments()
                    .create_attachment_dhcp_identifier(
                        CreateAttachmentDhcpIdentifier::new(
                            attachment.id(),
                            DhcpIdentifierFamily::V6,
                            DhcpIdentifierKind::DuidLl,
                            format!("00:03:00:01:aa:bb:cc:dd:ee:{:02x}", net_index),
                            DhcpPriority::new(0),
                        )
                        .expect("dhcp identifier"),
                    )
                    .await
                    .expect("dhcp identifier create");
                storage
                    .attachments()
                    .create_attachment_prefix_reservation(
                        CreateAttachmentPrefixReservation::new(
                            attachment.id(),
                            CidrValue::new(format!("2001:db8:70:{net_index:x}::/64"))
                                .expect("prefix"),
                        )
                        .expect("prefix command"),
                    )
                    .await
                    .expect("prefix reservation create");
            } else {
                storage
                    .attachments()
                    .create_attachment_dhcp_identifier(
                        CreateAttachmentDhcpIdentifier::new(
                            attachment.id(),
                            DhcpIdentifierFamily::V4,
                            DhcpIdentifierKind::ClientId,
                            format!("01:aa:bb:cc:dd:ee:{:02x}", net_index),
                            DhcpPriority::new(0),
                        )
                        .expect("dhcp identifier"),
                    )
                    .await
                    .expect("dhcp identifier create");
            }

            let community_name = if on_v6 {
                "bench-community-v6"
            } else {
                "bench-community-v4"
            };
            storage
                .attachment_community_assignments()
                .create_attachment_community_assignment(CreateAttachmentCommunityAssignment::new(
                    attachment.id(),
                    NetworkPolicyName::new("bench-policy").expect("policy"),
                    CommunityName::new(community_name).expect("community"),
                ))
                .await
                .expect("community assignment create");
        }
    });

    (storage, host)
}

/// Seed two host detail fixtures: a small host (1 attachment, 2 IPs) and a
/// large host (4 attachments with mixed identifiers, prefixes, communities).
pub fn host_detail_fixtures(runtime: &Runtime) -> (DynStorage, Hostname, Hostname) {
    let storage = memory_storage();
    let small = Hostname::new("small.bench.test").expect("hostname");
    let large = Hostname::new("large.bench.test").expect("hostname");

    runtime.block_on(async {
        storage
            .network_policies()
            .create_network_policy(
                CreateNetworkPolicy::new(
                    NetworkPolicyName::new("detail-policy").expect("policy name"),
                    "detail bench policy",
                    None,
                )
                .expect("policy command"),
            )
            .await
            .expect("policy create");

        let v4 = CidrValue::new("10.80.0.0/24").expect("v4 cidr");
        let v6 = CidrValue::new("2001:db8:80::/112").expect("v6 cidr");

        for cidr in [v4.clone(), v6.clone()] {
            storage
                .networks()
                .create_network(
                    CreateNetwork::new(
                        cidr,
                        "detail bench network",
                        ReservedCount::new(0).expect("reserved"),
                    )
                    .expect("network"),
                )
                .await
                .expect("network create");
        }

        for (community_cidr, name) in [
            (v4.clone(), "detail-community-v4"),
            (v6.clone(), "detail-community-v6"),
        ] {
            storage
                .communities()
                .create_community(
                    CreateCommunity::new(
                        NetworkPolicyName::new("detail-policy").expect("policy"),
                        community_cidr,
                        CommunityName::new(name).expect("community"),
                        "detail bench community",
                    )
                    .expect("community command"),
                )
                .await
                .expect("community create");
        }

        // Small host. Both IPs are wired onto the same attachment by passing
        // the same MAC through `assign_ip_address`; the storage layer keys
        // attachments by (host, network, mac).
        storage
            .hosts()
            .create_host(
                CreateHost::new(small.clone(), None, None, "small bench host").expect("host"),
            )
            .await
            .expect("host create");
        let small_mac = MacAddressValue::new("aa:bb:cc:dd:ee:01").expect("mac");
        let small_attach = storage
            .attachments()
            .create_attachment(CreateHostAttachment::new(
                small.clone(),
                v4.clone(),
                Some(small_mac.clone()),
                Some("small attachment".to_string()),
            ))
            .await
            .expect("small attachment");
        storage
            .hosts()
            .assign_ip_address(
                AssignIpAddress::new(
                    small.clone(),
                    Some(IpAddressValue::new("10.80.0.10").expect("ip")),
                    None,
                    Some(small_mac.clone()),
                )
                .expect("assign"),
            )
            .await
            .expect("assign ok");
        storage
            .hosts()
            .assign_ip_address(
                AssignIpAddress::new(
                    small.clone(),
                    Some(IpAddressValue::new("10.80.0.11").expect("ip")),
                    None,
                    Some(small_mac),
                )
                .expect("assign"),
            )
            .await
            .expect("assign ok");
        let _ = small_attach;

        // Large host.
        storage
            .hosts()
            .create_host(
                CreateHost::new(large.clone(), None, None, "large bench host").expect("host"),
            )
            .await
            .expect("host create");
        for index in 0..4u32 {
            let on_v6 = index.is_multiple_of(2);
            let network = if on_v6 { v6.clone() } else { v4.clone() };
            let mac = MacAddressValue::new(format!("aa:bb:cc:ee:ee:{:02x}", index)).expect("mac");
            let attachment = storage
                .attachments()
                .create_attachment(CreateHostAttachment::new(
                    large.clone(),
                    network.clone(),
                    Some(mac.clone()),
                    Some(format!("large attachment {index}")),
                ))
                .await
                .expect("attachment create");

            // Assign an IP to the same attachment by threading the MAC through.
            let address = if on_v6 {
                IpAddressValue::new(format!("2001:db8:80::{}", index + 10)).expect("v6 ip")
            } else {
                IpAddressValue::new(format!("10.80.0.{}", index + 20)).expect("v4 ip")
            };
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(large.clone(), Some(address), None, Some(mac))
                        .expect("assign"),
                )
                .await
                .expect("large assign ok");

            if on_v6 {
                storage
                    .attachments()
                    .create_attachment_dhcp_identifier(
                        CreateAttachmentDhcpIdentifier::new(
                            attachment.id(),
                            DhcpIdentifierFamily::V6,
                            DhcpIdentifierKind::DuidLl,
                            format!("00:03:00:01:aa:bb:cc:ee:ee:{:02x}", index),
                            DhcpPriority::new(0),
                        )
                        .expect("dhcp identifier"),
                    )
                    .await
                    .expect("dhcp create");
                // /120 delegated prefixes inside the /112 parent network so
                // the attachment graph remains internally consistent.
                let suffix = index * 0x100;
                storage
                    .attachments()
                    .create_attachment_prefix_reservation(
                        CreateAttachmentPrefixReservation::new(
                            attachment.id(),
                            CidrValue::new(format!("2001:db8:80::{suffix:x}/120")).expect("prefix"),
                        )
                        .expect("prefix command"),
                    )
                    .await
                    .expect("prefix create");
            } else {
                storage
                    .attachments()
                    .create_attachment_dhcp_identifier(
                        CreateAttachmentDhcpIdentifier::new(
                            attachment.id(),
                            DhcpIdentifierFamily::V4,
                            DhcpIdentifierKind::ClientId,
                            format!("01:aa:bb:cc:ee:ee:{:02x}", index),
                            DhcpPriority::new(0),
                        )
                        .expect("dhcp identifier"),
                    )
                    .await
                    .expect("dhcp create");
            }

            let community_name = if on_v6 {
                "detail-community-v6"
            } else {
                "detail-community-v4"
            };
            storage
                .attachment_community_assignments()
                .create_attachment_community_assignment(CreateAttachmentCommunityAssignment::new(
                    attachment.id(),
                    NetworkPolicyName::new("detail-policy").expect("policy"),
                    CommunityName::new(community_name).expect("community"),
                ))
                .await
                .expect("community assignment");
        }
    });

    (storage, small, large)
}

/// Network detail fixture: a /24 with several attached hosts and policy state.
pub fn network_detail_fixture(runtime: &Runtime) -> (DynStorage, CidrValue) {
    let storage = memory_storage();
    let cidr = CidrValue::new("10.90.0.0/24").expect("cidr");

    runtime.block_on(async {
        storage
            .network_policies()
            .create_network_policy(
                CreateNetworkPolicy::new(
                    NetworkPolicyName::new("net-policy").expect("policy"),
                    "network detail bench",
                    None,
                )
                .expect("policy"),
            )
            .await
            .expect("policy create");

        storage
            .networks()
            .create_network(
                CreateNetwork::new(
                    cidr.clone(),
                    "network detail bench",
                    ReservedCount::new(0).expect("reserved"),
                )
                .expect("network"),
            )
            .await
            .expect("network create");

        storage
            .communities()
            .create_community(
                CreateCommunity::new(
                    NetworkPolicyName::new("net-policy").expect("policy"),
                    cidr.clone(),
                    CommunityName::new("net-community").expect("community"),
                    "net detail community",
                )
                .expect("community"),
            )
            .await
            .expect("community create");

        for index in 0..12u32 {
            let host = Hostname::new(format!("net-{index:02}.bench.test")).expect("hostname");
            storage
                .hosts()
                .create_host(
                    CreateHost::new(host.clone(), None, None, "network bench host").expect("host"),
                )
                .await
                .expect("host create");
            let mac = MacAddressValue::new(format!("aa:bb:cc:90:00:{:02x}", index)).expect("mac");
            let attachment = storage
                .attachments()
                .create_attachment(CreateHostAttachment::new(
                    host.clone(),
                    cidr.clone(),
                    Some(mac.clone()),
                    Some(format!("net attachment {index}")),
                ))
                .await
                .expect("attachment create");
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(
                        host,
                        Some(IpAddressValue::new(format!("10.90.0.{}", index + 10)).expect("ip")),
                        None,
                        Some(mac),
                    )
                    .expect("assign"),
                )
                .await
                .expect("assign ok");
            storage
                .attachments()
                .create_attachment_dhcp_identifier(
                    CreateAttachmentDhcpIdentifier::new(
                        attachment.id(),
                        DhcpIdentifierFamily::V4,
                        DhcpIdentifierKind::ClientId,
                        format!("01:aa:bb:cc:90:00:{:02x}", index),
                        DhcpPriority::new(0),
                    )
                    .expect("dhcp"),
                )
                .await
                .expect("dhcp create");
            storage
                .attachment_community_assignments()
                .create_attachment_community_assignment(CreateAttachmentCommunityAssignment::new(
                    attachment.id(),
                    NetworkPolicyName::new("net-policy").expect("policy"),
                    CommunityName::new("net-community").expect("community"),
                ))
                .await
                .expect("community assignment");
        }
    });

    (storage, cidr)
}

/// Seed forward + reverse zones with a small set of records, ready for
/// `run_export` benches.
pub fn zone_export_storage(runtime: &Runtime) -> (DynStorage, ZoneName, ZoneName) {
    let storage = memory_storage();
    let forward = ZoneName::new("export.test").expect("zone");
    let reverse = ZoneName::new("0.0.10.in-addr.arpa").expect("reverse zone");

    runtime.block_on(async {
        storage
            .nameservers()
            .create_nameserver(CreateNameServer::new(
                DnsName::new("ns1.export.test").expect("ns name"),
                None,
            ))
            .await
            .expect("nameserver create");
        storage
            .zones()
            .create_forward_zone(CreateForwardZone::new(
                forward.clone(),
                DnsName::new("ns1.export.test").expect("ns"),
                vec![DnsName::new("ns1.export.test").expect("ns")],
                EmailAddressValue::new("hostmaster@export.test").expect("email"),
                SerialNumber::new(2026042001).expect("serial"),
                SoaSeconds::new(10800).expect("refresh"),
                SoaSeconds::new(3600).expect("retry"),
                SoaSeconds::new(604800).expect("expire"),
                Ttl::new(3600).expect("soa ttl"),
                Ttl::new(3600).expect("default ttl"),
            ))
            .await
            .expect("forward zone create");
        storage
            .zones()
            .create_reverse_zone(CreateReverseZone::new(
                reverse.clone(),
                Some(CidrValue::new("10.0.0.0/24").expect("cidr")),
                DnsName::new("ns1.export.test").expect("ns"),
                vec![DnsName::new("ns1.export.test").expect("ns")],
                EmailAddressValue::new("hostmaster@export.test").expect("email"),
                SerialNumber::new(2026042001).expect("serial"),
                SoaSeconds::new(10800).expect("refresh"),
                SoaSeconds::new(3600).expect("retry"),
                SoaSeconds::new(604800).expect("expire"),
                Ttl::new(3600).expect("soa ttl"),
                Ttl::new(3600).expect("default ttl"),
            ))
            .await
            .expect("reverse zone create");

        for index in 0..20u32 {
            let owner = format!("zhost-{index:02}.export.test");
            let cmd = CreateRecordInstance::new_unanchored(
                RecordTypeName::new("A").expect("type"),
                owner,
                None,
                json!({"address": format!("10.0.0.{}", index + 10)}),
            )
            .expect("record");
            storage
                .records()
                .create_record(cmd)
                .await
                .expect("record create");
        }
    });

    (storage, forward, reverse)
}

/// Seed a DHCP-export-ready storage with networks, hosts, and attachments
/// across both v4 and v6 families.
pub fn dhcp_export_storage(runtime: &Runtime) -> DynStorage {
    let storage = memory_storage();

    runtime.block_on(async {
        storage
            .network_policies()
            .create_network_policy(
                CreateNetworkPolicy::new(
                    NetworkPolicyName::new("dhcp-policy").expect("policy"),
                    "dhcp bench",
                    None,
                )
                .expect("policy"),
            )
            .await
            .expect("policy create");

        let v4 = CidrValue::new("10.100.0.0/24").expect("v4");
        let v6 = CidrValue::new("2001:db8:100::/112").expect("v6");
        for cidr in [v4.clone(), v6.clone()] {
            storage
                .networks()
                .create_network(
                    CreateNetwork::new(
                        cidr,
                        "dhcp bench network",
                        ReservedCount::new(0).expect("reserved"),
                    )
                    .expect("network"),
                )
                .await
                .expect("network create");
        }

        storage
            .communities()
            .create_community(
                CreateCommunity::new(
                    NetworkPolicyName::new("dhcp-policy").expect("policy"),
                    v6.clone(),
                    CommunityName::new("dhcp-community").expect("community"),
                    "dhcp bench community",
                )
                .expect("community"),
            )
            .await
            .expect("community create");

        for index in 0..6u32 {
            let host = Hostname::new(format!("dhcp-{index:02}.bench.test")).expect("hostname");
            storage
                .hosts()
                .create_host(
                    CreateHost::new(host.clone(), None, None, "dhcp bench host").expect("host"),
                )
                .await
                .expect("host create");
            let mac = MacAddressValue::new(format!("aa:bb:cc:00:dc:{:02x}", index)).expect("mac");
            let attachment_v4 = storage
                .attachments()
                .create_attachment(CreateHostAttachment::new(
                    host.clone(),
                    v4.clone(),
                    Some(mac.clone()),
                    Some(format!("dhcp v4 attachment {index}")),
                ))
                .await
                .expect("attachment v4");
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(
                        host.clone(),
                        Some(IpAddressValue::new(format!("10.100.0.{}", index + 10)).expect("ip")),
                        None,
                        Some(mac.clone()),
                    )
                    .expect("assign"),
                )
                .await
                .expect("assign v4");
            storage
                .attachments()
                .create_attachment_dhcp_identifier(
                    CreateAttachmentDhcpIdentifier::new(
                        attachment_v4.id(),
                        DhcpIdentifierFamily::V4,
                        DhcpIdentifierKind::ClientId,
                        format!("01:aa:bb:cc:00:dc:{:02x}", index),
                        DhcpPriority::new(0),
                    )
                    .expect("dhcp"),
                )
                .await
                .expect("dhcp v4 create");

            let attachment_v6 = storage
                .attachments()
                .create_attachment(CreateHostAttachment::new(
                    host.clone(),
                    v6.clone(),
                    Some(mac.clone()),
                    Some(format!("dhcp v6 attachment {index}")),
                ))
                .await
                .expect("attachment v6");
            storage
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(
                        host,
                        Some(
                            IpAddressValue::new(format!("2001:db8:100::{}", index + 10))
                                .expect("ip"),
                        ),
                        None,
                        Some(mac.clone()),
                    )
                    .expect("assign v6"),
                )
                .await
                .expect("assign v6 ok");
            storage
                .attachments()
                .create_attachment_dhcp_identifier(
                    CreateAttachmentDhcpIdentifier::new(
                        attachment_v6.id(),
                        DhcpIdentifierFamily::V6,
                        DhcpIdentifierKind::DuidLl,
                        format!("00:03:00:01:aa:bb:cc:00:dc:{:02x}", index),
                        DhcpPriority::new(0),
                    )
                    .expect("dhcp v6"),
                )
                .await
                .expect("dhcp v6 create");
            // /120 delegated prefixes inside the /112 parent network so the
            // attachment graph remains internally consistent.
            let v6_prefix_suffix = index * 0x100;
            storage
                .attachments()
                .create_attachment_prefix_reservation(
                    CreateAttachmentPrefixReservation::new(
                        attachment_v6.id(),
                        CidrValue::new(format!("2001:db8:100::{v6_prefix_suffix:x}/120"))
                            .expect("prefix"),
                    )
                    .expect("prefix"),
                )
                .await
                .expect("prefix create");
            storage
                .attachment_community_assignments()
                .create_attachment_community_assignment(CreateAttachmentCommunityAssignment::new(
                    attachment_v6.id(),
                    NetworkPolicyName::new("dhcp-policy").expect("policy"),
                    CommunityName::new("dhcp-community").expect("community"),
                ))
                .await
                .expect("community assignment");
        }
    });

    storage
}

/// Construct a representative import batch covering scalars, `_ref`-resolved
/// references, and mixed typed attributes. The community references the
/// preceding network and policy items via `{key}_ref` placeholders, so the
/// resolver and ref map are exercised end-to-end.
pub fn import_batch_payload() -> Value {
    let mut items = Vec::new();
    for label_idx in 0..4 {
        items.push(json!({
            "ref": format!("label-{label_idx}"),
            "kind": "label",
            "operation": "create",
            "attributes": {
                "name": format!("bench-label-{label_idx}"),
                "description": format!("bench label {label_idx}"),
            },
        }));
    }
    items.push(json!({
        "ref": "policy",
        "kind": "network_policy",
        "operation": "create",
        "attributes": {
            "name": "import-policy",
            "description": "imported policy",
        },
    }));
    for net_idx in 0..2 {
        items.push(json!({
            "ref": format!("network-{net_idx}"),
            "kind": "network",
            "operation": "create",
            "attributes": {
                "cidr": format!("10.110.{net_idx}.0/24"),
                "description": format!("imported network {net_idx}"),
                "reserved": 1,
            },
        }));
    }
    items.push(json!({
        "ref": "community",
        "kind": "community",
        "operation": "create",
        "attributes": {
            "policy_name_ref": "policy",
            "network_ref": "network-0",
            "name": "import-community",
            "description": "imported community",
        },
    }));
    for host_idx in 0..6 {
        items.push(json!({
            "ref": format!("host-{host_idx}"),
            "kind": "host",
            "operation": "create",
            "attributes": {
                "name": format!("imp-{host_idx:02}.bench.test"),
                "comment": "imported host",
            },
        }));
    }
    json!({"items": items})
}

/// Build a fully-resolved ImportBatch domain object from the canonical bench
/// payload. Useful for benches that want to skip the JSON parse cost.
pub fn import_batch() -> ImportBatch {
    let payload = import_batch_payload();
    let raw = payload
        .get("items")
        .and_then(Value::as_array)
        .expect("items array");
    let items: Vec<ImportItem> = raw
        .iter()
        .map(|item| {
            let reference = item
                .get("ref")
                .and_then(Value::as_str)
                .expect("ref")
                .to_string();
            let kind: ImportKind = serde_json::from_value(item.get("kind").cloned().expect("kind"))
                .expect("kind enum");
            let operation: ImportOperation =
                serde_json::from_value(item.get("operation").cloned().expect("operation"))
                    .expect("operation enum");
            let attributes = item.get("attributes").cloned().unwrap_or(Value::Null);
            ImportItem::new(reference, kind, operation, attributes).expect("item")
        })
        .collect();
    ImportBatch::new(items).expect("batch")
}

pub fn import_batch_command() -> CreateImportBatch {
    CreateImportBatch::new(import_batch(), Some("bench-user".to_string()))
}

/// Pre-create a nameserver so that downstream zone-create calls satisfy the
/// nameserver-existence check enforced by the memory backend.
pub fn seed_nameserver(runtime: &Runtime, storage: &DynStorage, name: &str) {
    runtime.block_on(async {
        storage
            .nameservers()
            .create_nameserver(CreateNameServer::new(
                DnsName::new(name).expect("nameserver name"),
                None,
            ))
            .await
            .expect("nameserver create");
    });
}

/// Pre-create a label so that the storage benches don't pay seed cost.
pub fn seed_label(runtime: &Runtime, storage: &DynStorage, name: &str) {
    runtime.block_on(async {
        storage
            .labels()
            .create_label(
                CreateLabel::new(LabelName::new(name).expect("label"), "bench label")
                    .expect("label command"),
            )
            .await
            .expect("label create");
    });
}
