use rusqlite::Connection;

use crate::error::AppError;

/// 建表迁移：五张核心表 + 索引（幂等，可重复执行）
const MIGRATIONS: &[&str] = &[
    // 分组表
    "CREATE TABLE IF NOT EXISTS groups (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        name        TEXT NOT NULL,
        parent_id   INTEGER,
        sort_order  INTEGER NOT NULL DEFAULT 0,
        remark      TEXT NOT NULL DEFAULT '',
        created_at  TEXT NOT NULL DEFAULT (datetime('now','localtime')),
        updated_at  TEXT NOT NULL DEFAULT (datetime('now','localtime'))
    );",
    "CREATE INDEX IF NOT EXISTS idx_groups_parent ON groups(parent_id);",
    // 导入模板表
    "CREATE TABLE IF NOT EXISTS templates (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        name        TEXT NOT NULL,
        type        TEXT NOT NULL CHECK (type IN ('xlsx','json')),
        mappings    TEXT NOT NULL DEFAULT '{}',
        is_builtin  INTEGER NOT NULL DEFAULT 0,
        created_at  TEXT NOT NULL DEFAULT (datetime('now','localtime')),
        updated_at  TEXT NOT NULL DEFAULT (datetime('now','localtime'))
    );",
    // 测试用例表
    "CREATE TABLE IF NOT EXISTS test_cases (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        group_id      INTEGER,
        case_name     TEXT NOT NULL,
        method        TEXT NOT NULL CHECK (method IN ('GET','POST','PUT','DELETE','PATCH')),
        content_type  TEXT NOT NULL DEFAULT 'none',
        url           TEXT NOT NULL,
        headers       TEXT NOT NULL DEFAULT '{}',
        request_body  TEXT NOT NULL DEFAULT '',
        query_params  TEXT NOT NULL DEFAULT '{}',
        assert_expr   TEXT NOT NULL DEFAULT '',
        timeout_secs  INTEGER NOT NULL DEFAULT 30,
        remark        TEXT NOT NULL DEFAULT '',
        created_at    TEXT NOT NULL DEFAULT (datetime('now','localtime')),
        updated_at    TEXT NOT NULL DEFAULT (datetime('now','localtime'))
    );",
    "CREATE INDEX IF NOT EXISTS idx_cases_group ON test_cases(group_id);",
    // 测试报告表
    "CREATE TABLE IF NOT EXISTS reports (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        name          TEXT NOT NULL,
        total_count   INTEGER NOT NULL DEFAULT 0,
        success_count INTEGER NOT NULL DEFAULT 0,
        fail_count    INTEGER NOT NULL DEFAULT 0,
        env_name      TEXT NOT NULL DEFAULT '',
        base_url      TEXT NOT NULL DEFAULT '',
        started_at    TEXT NOT NULL DEFAULT (datetime('now','localtime')),
        duration_ms   INTEGER NOT NULL DEFAULT 0,
        created_at    TEXT NOT NULL DEFAULT (datetime('now','localtime'))
    );",
    "CREATE INDEX IF NOT EXISTS idx_reports_created ON reports(created_at);",
    // 报告明细表
    "CREATE TABLE IF NOT EXISTS report_items (
        id                INTEGER PRIMARY KEY AUTOINCREMENT,
        report_id         INTEGER NOT NULL,
        case_id           INTEGER,
        case_name         TEXT NOT NULL,
        method            TEXT NOT NULL,
        url               TEXT NOT NULL,
        request_snapshot  TEXT NOT NULL DEFAULT '{}',
        response_status   INTEGER,
        response_headers  TEXT NOT NULL DEFAULT '{}',
        response_body     TEXT NOT NULL DEFAULT '',
        assert_results    TEXT NOT NULL DEFAULT '[]',
        is_success        INTEGER NOT NULL DEFAULT 0,
        error_msg         TEXT NOT NULL DEFAULT '',
        duration_ms       INTEGER NOT NULL DEFAULT 0
    );",
    "CREATE INDEX IF NOT EXISTS idx_report_items_report ON report_items(report_id);",
];

pub fn run(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    for sql in MIGRATIONS {
        conn.execute_batch(sql)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        run(&conn).unwrap(); // 重复执行不报错
    }
}
