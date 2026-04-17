pub mod models;
pub mod schema;

use diesel::{
    Connection, PgConnection,
    connection::InstrumentationEvent,
    r2d2::{ConnectionManager, Pool},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    sync::{Mutex, OnceLock},
};

use crate::{config::Config, errors::AppError};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub type PgPool = Pool<ConnectionManager<PgConnection>>;

tokio::task_local! {
    static ACTIVE_QUERY_CAPTURE_ID: String;
}

#[derive(Clone, Debug, Default)]
pub struct QueryCaptureSnapshot {
    total_queries: usize,
    query_counts: BTreeMap<String, usize>,
}

impl QueryCaptureSnapshot {
    pub fn total_queries(&self) -> usize {
        self.total_queries
    }

    pub fn query_counts(&self) -> &BTreeMap<String, usize> {
        &self.query_counts
    }

    pub fn queries_matching(&self, needle: &str) -> usize {
        self.query_counts
            .iter()
            .filter(|(query, _)| query.contains(needle))
            .map(|(_, count)| *count)
            .sum()
    }
}

#[derive(Default)]
struct QueryCaptureRegistry {
    captures: HashMap<String, QueryCaptureSnapshot>,
}

fn query_capture_registry() -> &'static Mutex<QueryCaptureRegistry> {
    static REGISTRY: OnceLock<Mutex<QueryCaptureRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(QueryCaptureRegistry::default()))
}

fn active_query_capture_id() -> Option<String> {
    ACTIVE_QUERY_CAPTURE_ID
        .try_with(|capture_id| capture_id.clone())
        .ok()
}

fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn record_query(capture_id: &str, query: &str) {
    let query = normalize_query(query);
    let mut registry = query_capture_registry()
        .lock()
        .expect("query capture registry should not be poisoned");
    let snapshot = registry.captures.entry(capture_id.to_string()).or_default();
    snapshot.total_queries += 1;
    *snapshot.query_counts.entry(query).or_insert(0) += 1;
}

struct QueryCaptureInstrumentation {
    capture_id: Option<String>,
}

impl QueryCaptureInstrumentation {
    fn new(capture_id: Option<String>) -> Self {
        Self { capture_id }
    }
}

impl diesel::connection::Instrumentation for QueryCaptureInstrumentation {
    fn on_connection_event(&mut self, event: InstrumentationEvent<'_>) {
        let Some(capture_id) = &self.capture_id else {
            return;
        };
        if let InstrumentationEvent::StartQuery { query, .. } = event {
            record_query(capture_id, &query.to_string());
        }
    }
}

pub async fn with_query_capture<T>(
    capture_id: impl Into<String>,
    future: impl Future<Output = T>,
) -> T {
    let capture_id = capture_id.into();
    clear_query_capture(&capture_id);
    ACTIVE_QUERY_CAPTURE_ID.scope(capture_id, future).await
}

pub fn clear_query_capture(capture_id: &str) {
    let mut registry = query_capture_registry()
        .lock()
        .expect("query capture registry should not be poisoned");
    registry.captures.remove(capture_id);
}

pub fn take_query_capture(capture_id: &str) -> QueryCaptureSnapshot {
    let mut registry = query_capture_registry()
        .lock()
        .expect("query capture registry should not be poisoned");
    registry.captures.remove(capture_id).unwrap_or_default()
}

#[derive(Clone)]
pub struct Database {
    pool: Option<PgPool>,
}

impl Database {
    pub fn connect(config: &Config) -> Result<Self, AppError> {
        let Some(database_url) = &config.database_url else {
            return Ok(Self { pool: None });
        };

        let manager = ConnectionManager::<PgConnection>::new(database_url);
        let pool = Pool::builder().build(manager).map_err(AppError::internal)?;

        if config.run_migrations {
            let mut connection = pool.get().map_err(AppError::internal)?;
            connection
                .run_pending_migrations(MIGRATIONS)
                .map_err(AppError::internal)?;
        }

        Ok(Self { pool: Some(pool) })
    }

    pub fn is_configured(&self) -> bool {
        self.pool.is_some()
    }

    pub fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    pub async fn run<T, F>(&self, operation: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut PgConnection) -> Result<T, AppError> + Send + 'static,
    {
        let Some(pool) = self.pool.clone() else {
            return Err(AppError::unavailable(
                "database-backed storage is not configured",
            ));
        };
        let capture_id = active_query_capture_id();

        tokio::task::spawn_blocking(move || {
            let mut connection = pool.get().map_err(AppError::internal)?;
            connection.set_instrumentation(QueryCaptureInstrumentation::new(capture_id));
            operation(&mut connection)
        })
        .await
        .map_err(AppError::internal)?
    }
}
