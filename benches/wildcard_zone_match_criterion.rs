mod support;

use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::json;

use mreg_rust::domain::{
    resource_records::CreateRecordInstance,
    types::{DnsName, EmailAddressValue, RecordTypeName, SerialNumber, SoaSeconds, Ttl, ZoneName},
    zone::CreateForwardZone,
};

const ZONE_COUNT: u32 = 50;

fn wildcard_zone_match(c: &mut Criterion) {
    let runtime = support::runtime();
    let storage = support::memory_storage();

    for depth in 0..ZONE_COUNT {
        let ns_name = format!("ns1.z{depth:02}.bench.test");
        support::seed_nameserver(&runtime, &storage, &ns_name);
    }

    runtime.block_on(async {
        for depth in 0..ZONE_COUNT {
            let name = format!("z{depth:02}.bench.test");
            storage
                .zones()
                .create_forward_zone(CreateForwardZone::new(
                    ZoneName::new(&name).expect("zone name"),
                    DnsName::new(format!("ns1.{name}")).expect("ns"),
                    vec![DnsName::new(format!("ns1.{name}")).expect("ns")],
                    EmailAddressValue::new(format!("hostmaster@{name}")).expect("email"),
                    SerialNumber::new(2026042001).expect("serial"),
                    SoaSeconds::new(10800).expect("refresh"),
                    SoaSeconds::new(3600).expect("retry"),
                    SoaSeconds::new(604800).expect("expire"),
                    Ttl::new(3600).expect("soa ttl"),
                    Ttl::new(3600).expect("default ttl"),
                ))
                .await
                .expect("zone create");
        }
    });

    let counter = AtomicUsize::new(0);

    c.bench_function("wildcard_zone_match_50", |b| {
        b.iter(|| {
            let n = counter.fetch_add(1, Ordering::Relaxed);
            // Always target the deepest-nested zone (z49.bench.test) so the
            // longest-suffix match must compare against every seeded zone.
            let owner = format!("h{n:08}.z49.bench.test");
            let cmd = CreateRecordInstance::new_unanchored(
                RecordTypeName::new("A").expect("type"),
                owner,
                None,
                json!({"address": format!("10.0.0.{}", (n % 250) + 1)}),
            )
            .expect("record cmd");
            let result = runtime
                .block_on(storage.records().create_record(black_box(cmd)))
                .expect("record create succeeds");
            black_box(result);
        });
    });
}

criterion_group!(benches, wildcard_zone_match);
criterion_main!(benches);
