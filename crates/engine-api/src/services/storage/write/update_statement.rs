use super::*;

pub(super) fn inserted(
    version: &str,
    write: i16,
    now: chrono::DateTime<chrono::Utc>,
) -> WriteResult {
    WriteResult {
        write,
        version: version.to_owned(),
        create_time: now,
        update_time: now,
        applied: true,
    }
}

pub(super) fn rewritten(
    version: &str,
    write: i16,
    row: &StorageCurrentRow,
    now: chrono::DateTime<chrono::Utc>,
) -> WriteResult {
    WriteResult {
        write,
        version: version.to_owned(),
        create_time: row.create_time,
        update_time: now,
        applied: true,
    }
}

pub(super) fn unchanged(row: &StorageCurrentRow) -> WriteResult {
    WriteResult {
        write: row.write,
        version: row.version.clone(),
        create_time: row.create_time,
        update_time: row.update_time,
        applied: false,
    }
}
