#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};

use chrono::Utc;
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

use mreg_rust::{
    authz::AttrValue,
    config::{Config, StorageBackendSetting},
    domain::{
        builtin_types::built_in_record_types,
        filters::HostFilter,
        host::{AssignIpAddress, CreateHost, HostAuthContext},
        network::CreateNetwork,
        pagination::{PageRequest, SortDirection},
        resource_records::RecordTypeDefinition,
        types::{CidrValue, Hostname, IpAddressValue, ReservedCount},
    },
    storage::{DynStorage, build_storage},
};

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
    let storage = memory_storage();
    let network = CidrValue::new("10.20.0.0/24").expect("valid CIDR");
    let pending_host = Hostname::new("pending.bench.test").expect("valid benchmark hostname");

    runtime.block_on(async {
        storage
            .networks()
            .create_network(
                CreateNetwork::new(
                    network.clone(),
                    "dense allocation network",
                    ReservedCount::new(1).expect("valid reserved count"),
                )
                .expect("valid network"),
            )
            .await
            .expect("network should be created");

        for index in 0..existing_allocations {
            let host = Hostname::new(format!("existing-{index:04}.bench.test"))
                .expect("valid benchmark hostname");
            storage
                .hosts()
                .create_host(
                    CreateHost::new(host.clone(), None, None, "existing benchmark host")
                        .expect("valid benchmark host"),
                )
                .await
                .expect("host should be created");

            let address = IpAddressValue::new(format!("10.20.0.{}", index + 2))
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

fn memory_storage() -> DynStorage {
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
