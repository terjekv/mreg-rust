use diesel::{
    PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{BigInt, Text},
};

use crate::errors::AppError;

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

/// Execute a dynamically-built COUNT query and return the result.
pub(in crate::storage::postgres) fn run_count_query(
    connection: &mut PgConnection,
    count_sql: &str,
    values: &[String],
) -> Result<u64, AppError> {
    let rows: Vec<CountRow> =
        run_dynamic_query(connection, count_sql, values).map_err(AppError::from)?;
    Ok(rows.first().map(|r| r.count.max(0) as u64).unwrap_or(0))
}

/// Generates match arms for binding N text parameters to a `sql_query`.
///
/// Diesel's `bind()` changes the query type at the type level with each call,
/// so parameters cannot be bound in a loop. This macro generates one match arm
/// per supported count from a compact invocation list, avoiding the need to
/// write out each arm by hand.
///
/// Each invocation line is `(count => idx0 idx1 ... idxN)`.
macro_rules! bind_text_params {
    (
        $q:expr, $v:expr, $c:expr, $max:literal;
        $( ( $n:literal => $($idx:literal)* ) )*
    ) => {
        match $v.len() {
            0 => sql_query($q).load($c),
            $(
                $n => sql_query($q)
                    $( .bind::<Text, _>(&$v[$idx]) )*
                    .load($c),
            )*
            n => Err(diesel::result::Error::QueryBuilderError(
                format!("too many filter bind parameters ({n}), max supported is {}", $max).into(),
            )),
        }
    };
}

/// Execute a dynamically-built SQL query with string bind values.
/// Returns the loaded rows for any `QueryableByName` type.
pub(in crate::storage::postgres) fn run_dynamic_query<
    T: QueryableByName<diesel::pg::Pg> + 'static,
>(
    connection: &mut PgConnection,
    query_str: &str,
    values: &[String],
) -> Result<Vec<T>, diesel::result::Error> {
    bind_text_params!(query_str, values, connection, 12;
        ( 1  => 0)
        ( 2  => 0 1)
        ( 3  => 0 1 2)
        ( 4  => 0 1 2 3)
        ( 5  => 0 1 2 3 4)
        ( 6  => 0 1 2 3 4 5)
        ( 7  => 0 1 2 3 4 5 6)
        ( 8  => 0 1 2 3 4 5 6 7)
        ( 9  => 0 1 2 3 4 5 6 7 8)
        (10  => 0 1 2 3 4 5 6 7 8 9)
        (11  => 0 1 2 3 4 5 6 7 8 9 10)
        (12  => 0 1 2 3 4 5 6 7 8 9 10 11)
    )
}
