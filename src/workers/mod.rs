pub const CLAIM_SQL: &str = r#"
select id
from tasks
where status = 'queued'
  and available_at <= now()
order by available_at asc, created_at asc
for update skip locked
limit 1
"#;
