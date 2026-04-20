mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mreg_rust::domain::{
    types::{DnsName, EmailAddressValue, SerialNumber, SoaSeconds, Ttl, ZoneName},
    zone::CreateForwardZone,
};

fn zone_serial_bump(c: &mut Criterion) {
    let runtime = support::runtime();
    let storage = support::memory_storage();

    support::seed_nameserver(&runtime, &storage, "ns1.serialbump.test");

    let zone = runtime.block_on(async {
        storage
            .zones()
            .create_forward_zone(CreateForwardZone::new(
                ZoneName::new("serialbump.test").expect("zone"),
                DnsName::new("ns1.serialbump.test").expect("ns"),
                vec![DnsName::new("ns1.serialbump.test").expect("ns")],
                EmailAddressValue::new("hostmaster@serialbump.test").expect("email"),
                SerialNumber::new(2026042001).expect("serial"),
                SoaSeconds::new(10800).expect("refresh"),
                SoaSeconds::new(3600).expect("retry"),
                SoaSeconds::new(604800).expect("expire"),
                Ttl::new(3600).expect("soa ttl"),
                Ttl::new(3600).expect("default ttl"),
            ))
            .await
            .expect("zone create")
    });
    let zone_id = zone.id();

    c.bench_function("zone_serial_bump", |b| {
        b.iter(|| {
            let bumped = runtime
                .block_on(storage.zones().bump_forward_zone_serial(black_box(zone_id)))
                .expect("serial bump succeeds");
            black_box(bumped);
        });
    });
}

criterion_group!(benches, zone_serial_bump);
criterion_main!(benches);
