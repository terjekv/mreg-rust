mod common;

use diesel::{Connection, PgConnection, RunQueryDsl, sql_query};
use futures::future::try_join_all;
use mreg_rust::{
    config::{Config, StorageBackendSetting},
    domain::{pagination::PageRequest, resource_records::built_in_record_types},
    storage::build_storage,
};

// A separate test binary keeps the unseeded schema and refresh independent of
// the shared, already initialized fixtures in the other PostgreSQL suites.
#[tokio::test]
async fn concurrent_builtin_initialization_and_refresh_preserve_definitions() {
    let Some(database_url) = common::postgres_test_database_url().expect("test schema") else {
        eprintln!("{}", common::postgres_skip_message("concurrent bootstrap"));
        return;
    };
    let storage = build_storage(&Config {
        database_url: Some(database_url.clone()),
        storage_backend: StorageBackendSetting::Postgres,
        run_migrations: true,
        ..Config::default()
    })
    .expect("unseeded PostgreSQL storage");
    let expected = built_in_record_types().expect("built-in definitions");
    let mut snapshots = Vec::new();
    let page = PageRequest::all();
    for refresh in [false, true] {
        if refresh {
            let mut connection = PgConnection::establish(&database_url).expect("test connection");
            sql_query("UPDATE record_types SET rendering_schema = '{}'::jsonb WHERE name = 'A'")
                .execute(&mut connection)
                .expect("simulate outdated definition");
        }
        let pages = try_join_all((0..8).map(|_| storage.records().list_record_types(&page)))
            .await
            .expect("concurrent bootstrap succeeds");
        snapshots.extend(pages.into_iter().map(|page| {
            page.items.len() == expected.len()
                && expected.iter().all(|definition| {
                    page.items.iter().any(|stored| {
                        stored.name() == definition.name()
                            && stored.dns_type() == definition.dns_type()
                            && stored.schema() == definition.schema()
                            && stored.built_in()
                    })
                })
        }));
    }
    assert_eq!(snapshots, vec![true; 16]);
}
