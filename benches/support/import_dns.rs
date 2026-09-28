use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion};
use mreg_rust::domain::{
    imports::{CreateImportBatch, ImportBatch, ImportItem, ImportKind, ImportOperation},
    types::{IpAddressValue, ip_to_ptr_name},
};
use serde_json::json;

use super::{IpImportScenario, memory_storage, record_listing_storage, runtime};

#[derive(Clone, Copy)]
enum DnsZones {
    Forward,
    Reverse,
    Both,
    AfterAssignments,
}

impl DnsZones {
    fn name(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Reverse => "reverse",
            Self::Both => "both",
            Self::AfterAssignments => "backfill",
        }
    }

    fn command(self, scenario: IpImportScenario, count: usize) -> CreateImportBatch {
        let inventory = scenario.command(count);
        let mut items = inventory.batch().items().to_vec();
        // Keep these hosts distinct from the populated fixture's existing owners.
        for item in &mut items {
            if item.kind() == &ImportKind::Host {
                let mut attributes = item.attributes().clone();
                let name = attributes["name"].as_str().expect("host name");
                attributes["name"] = json!(name.replace(".bench.test", ".import.bench.test"));
                *item = ImportItem::new(
                    item.reference(),
                    ImportKind::Host,
                    ImportOperation::Create,
                    attributes,
                )
                .expect("host item");
            }
        }
        let nameserver = ImportItem::new(
            "ns",
            ImportKind::Nameserver,
            ImportOperation::Create,
            json!({"name":"ns.import.bench.test"}),
        )
        .expect("nameserver item");
        let mut zones = Vec::new();
        if !matches!(self, Self::Reverse) {
            zones.push(Self::zone(
                "forward",
                ImportKind::ForwardZone,
                "import.bench.test",
            ));
        }
        if !matches!(self, Self::Forward) {
            for (reference, address, host_labels) in [
                ("reverse-v4", "10.111.0.1", 1),
                ("reverse-v6", "2001:db8:111::1", 16),
            ] {
                let owner = ip_to_ptr_name(&IpAddressValue::new(address).expect("address"));
                let zone = owner
                    .split('.')
                    .skip(host_labels)
                    .collect::<Vec<_>>()
                    .join(".");
                zones.push(Self::zone(reference, ImportKind::ReverseZone, &zone));
            }
        }
        items.insert(0, nameserver);
        if matches!(self, Self::AfterAssignments) {
            items.extend(zones);
        } else {
            items.splice(1..1, zones);
        }
        CreateImportBatch::new(ImportBatch::new(items).expect("DNS import batch"), None)
    }

    fn zone(reference: &str, kind: ImportKind, name: &str) -> ImportItem {
        ImportItem::new(
            reference,
            kind,
            ImportOperation::Create,
            json!({
                "name":name, "primary_ns":"ns.import.bench.test",
                "nameservers":["ns"], "email":"hostmaster@bench.test"
            }),
        )
        .expect("zone item")
    }
}

pub fn bench(c: &mut Criterion, name: &str, scenarios: &[IpImportScenario]) {
    let runtime = runtime();
    let mut group = c.benchmark_group(name);
    for &scenario in scenarios {
        for (zones, count, existing_records) in [
            (DnsZones::Forward, 32, 0),
            (DnsZones::Reverse, 32, 0),
            (DnsZones::Both, 32, 0),
            (DnsZones::AfterAssignments, 32, 0),
            (DnsZones::Both, 128, 0),
            (DnsZones::AfterAssignments, 128, 0),
            (DnsZones::Both, 32, 512),
            (DnsZones::AfterAssignments, 32, 512),
        ] {
            let command = zones.command(scenario, count);
            group.bench_function(
                BenchmarkId::new(
                    format!(
                        "{}/{}/existing_{existing_records}",
                        scenario.name(),
                        zones.name()
                    ),
                    count,
                ),
                |b| {
                    b.iter_batched_ref(
                        || {
                            let storage = if existing_records == 0 {
                                memory_storage()
                            } else {
                                record_listing_storage(&runtime, existing_records).0
                            };
                            let batch = runtime
                                .block_on(storage.imports().create_import_batch(command.clone()))
                                .expect("stage DNS import");
                            (storage, batch.id())
                        },
                        |(storage, id)| {
                            black_box(
                                runtime
                                    .block_on(storage.imports().run_import_batch(black_box(*id)))
                                    .expect("DNS import runs"),
                            )
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    group.finish();
}
