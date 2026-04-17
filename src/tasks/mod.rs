use crate::domain::tasks::TaskStatus;

pub const TASK_STATUSES: &[TaskStatus] = &[
    TaskStatus::Queued,
    TaskStatus::Running,
    TaskStatus::Succeeded,
    TaskStatus::Failed,
    TaskStatus::Cancelled,
];
