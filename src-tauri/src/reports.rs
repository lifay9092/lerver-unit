//! 测试报告（FR-R1~R6）
//!
//! - `report_page`：列表分页 + 过滤（名称模糊、时间范围、是否含失败）
//! - `report_detail`：汇总 + 明细数组
//! - `report_delete`：单条/批量删除（级联删 report_items）
//! - `report_export`：导出 xlsx（汇总 sheet + 明细 sheet）或 json（完整结构）到指定路径

use rusqlite::params_from_iter;
use rusqlite::types::ToSql;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use tauri::State;

use crate::error::{AppError, PageResult};
use crate::AppState;

/// 报告行（reports 表）
#[derive(Debug, Clone, Serialize)]
pub struct ReportRow {
    pub id: i64,
    pub name: String,
    pub total_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub env_name: String,
    pub base_url: String,
    pub started_at: String,
    pub duration_ms: i64,
    pub created_at: String,
}

/// 报告明细行（report_items 表）
#[derive(Debug, Clone, Serialize)]
pub struct ReportItemRow {
    pub id: i64,
    pub report_id: i64,
    pub case_id: Option<i64>,
    pub case_name: String,
    pub method: String,
    pub url: String,
    pub request_snapshot: String,
    pub response_status: Option<i64>,
    pub response_headers: String,
    pub response_body: String,
    pub assert_results: String,
    pub is_success: bool,
    pub error_msg: String,
    pub duration_ms: i64,
}

/// 报告详情（汇总 + 明细）
#[derive(Debug, Clone, Serialize)]
pub struct ReportDetail {
    pub report: ReportRow,
    pub items: Vec<ReportItemRow>,
}

/// 列表查询参数（snake_case，与前端一致）
#[derive(Debug, Default, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportQuery {
    pub keyword: Option<String>,
    /// true=仅含失败，false=仅全成功，None=全部
    pub has_fail: Option<bool>,
    /// started_at >= start（'YYYY-MM-DD HH:MM:SS'）
    pub start: Option<String>,
    /// started_at <= end
    pub end: Option<String>,
    pub page: i64,
    pub page_size: i64,
}

// ============ 命令 ============

/// 报告列表（FR-R3）
#[tauri::command]
pub fn report_page(
    query: ReportQuery,
    state: State<'_, AppState>,
) -> Result<PageResult<ReportRow>, AppError> {
    let guard = state.db.lock().unwrap();
    report_page_inner(&guard, query)
}

pub fn report_page_inner(
    conn: &rusqlite::Connection,
    query: ReportQuery,
) -> Result<PageResult<ReportRow>, AppError> {
    let page = query.page.max(1);
    let page_size = if query.page_size <= 0 { 20 } else { query.page_size.min(200) };

    let mut conds = Vec::new();
    let mut binds: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(kw) = &query.keyword {
        if !kw.trim().is_empty() {
            conds.push("name LIKE ?".to_string());
            binds.push(Box::new(format!("%{}%", kw)));
        }
    }
    if let Some(hf) = query.has_fail {
        if hf {
            conds.push("fail_count > 0".to_string());
        } else {
            conds.push("fail_count = 0".to_string());
        }
    }
    // 时间过滤：空字符串视为未传（防止 started_at <= '' 恒假导致查不到数据）
    if let Some(s) = &query.start {
        if !s.trim().is_empty() {
            conds.push("started_at >= ?".to_string());
            binds.push(Box::new(s.clone()));
        }
    }
    if let Some(e) = &query.end {
        if !e.trim().is_empty() {
            conds.push("started_at <= ?".to_string());
            binds.push(Box::new(e.clone()));
        }
    }

    let where_clause = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM reports{where_clause}"),
        params_from_iter(binds.iter().map(|b| b.as_ref())),
        |r| r.get(0),
    )?;

    let offset = (page - 1) * page_size;
    let sql = format!(
        "SELECT id, name, total_count, success_count, fail_count, env_name, base_url,
                started_at, duration_ms, created_at
         FROM reports{where_clause} ORDER BY id DESC LIMIT ? OFFSET ?"
    );
    let mut refs: Vec<&dyn ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    refs.push(&page_size);
    refs.push(&offset);

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(refs.iter().copied()), |r| {
        Ok(ReportRow {
            id: r.get(0)?,
            name: r.get(1)?,
            total_count: r.get(2)?,
            success_count: r.get(3)?,
            fail_count: r.get(4)?,
            env_name: r.get(5)?,
            base_url: r.get(6)?,
            started_at: r.get(7)?,
            duration_ms: r.get(8)?,
            created_at: r.get(9)?,
        })
    })?;
    let list: Vec<ReportRow> = rows.collect::<Result<_, _>>()?;

    Ok(PageResult {
        list,
        total,
        page,
        page_size,
    })
}

/// 报告详情（FR-R1/R2/R5）
#[tauri::command]
pub fn report_detail(id: i64, state: State<'_, AppState>) -> Result<ReportDetail, AppError> {
    let guard = state.db.lock().unwrap();
    report_detail_inner(&guard, id)
}

pub fn report_detail_inner(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<ReportDetail, AppError> {
    let report = query_report(conn, id)?;
    let items = query_items(conn, id)?;
    Ok(ReportDetail { report, items })
}

/// 删除报告（单条/批量，级联明细，FR-R6）
#[tauri::command]
pub fn report_delete(ids: Vec<i64>, state: State<'_, AppState>) -> Result<u64, AppError> {
    let guard = state.db.lock().unwrap();
    report_delete_inner(&guard, ids)
}

pub fn report_delete_inner(
    conn: &rusqlite::Connection,
    ids: Vec<i64>,
) -> Result<u64, AppError> {
    if ids.is_empty() {
        return Ok(0);
    }

    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let ph = placeholders.join(",");

    conn.execute(
        &format!("DELETE FROM report_items WHERE report_id IN ({ph})"),
        params_from_iter(ids.iter().map(|i| i as &dyn ToSql)),
    )?;
    let n = conn.execute(
        &format!("DELETE FROM reports WHERE id IN ({ph})"),
        params_from_iter(ids.iter().map(|i| i as &dyn ToSql)),
    )?;
    Ok(n as u64)
}

/// 导出报告（FR-R4）：xlsx / json，写入 file_path 返回该路径
#[tauri::command]
pub fn report_export(
    id: i64,
    format: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let guard = state.db.lock().unwrap();
    report_export_inner(&guard, id, &format, &file_path)
}

pub fn report_export_inner(
    conn: &rusqlite::Connection,
    id: i64,
    format: &str,
    file_path: &str,
) -> Result<String, AppError> {
    let detail = report_detail_inner(conn, id)?;
    let path = Path::new(file_path);
    match format {
        "xlsx" => export_xlsx(&detail, path).map(|_| file_path.to_string()),
        "json" => {
            let s = serde_json::to_string_pretty(&detail)?;
            std::fs::write(path, s)?;
            Ok(file_path.to_string())
        }
        other => Err(AppError::new(format!(
            "不支持的导出格式：{other}（仅支持 xlsx / json）"
        ))),
    }
}

// ============ 辅助 ============

fn query_report(conn: &rusqlite::Connection, id: i64) -> Result<ReportRow, AppError> {
    let r = conn.query_row(
        "SELECT id, name, total_count, success_count, fail_count, env_name, base_url,
                started_at, duration_ms, created_at
         FROM reports WHERE id = ?1",
        [id],
        |r| {
            Ok(ReportRow {
                id: r.get(0)?,
                name: r.get(1)?,
                total_count: r.get(2)?,
                success_count: r.get(3)?,
                fail_count: r.get(4)?,
                env_name: r.get(5)?,
                base_url: r.get(6)?,
                started_at: r.get(7)?,
                duration_ms: r.get(8)?,
                created_at: r.get(9)?,
            })
        },
    )?;
    Ok(r)
}

fn query_items(conn: &rusqlite::Connection, report_id: i64) -> Result<Vec<ReportItemRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, report_id, case_id, case_name, method, url, request_snapshot,
                response_status, response_headers, response_body, assert_results,
                is_success, error_msg, duration_ms
         FROM report_items WHERE report_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([report_id], |r| {
        Ok(ReportItemRow {
            id: r.get(0)?,
            report_id: r.get(1)?,
            case_id: r.get(2)?,
            case_name: r.get(3)?,
            method: r.get(4)?,
            url: r.get(5)?,
            request_snapshot: r.get(6)?,
            response_status: r.get(7)?,
            response_headers: r.get(8)?,
            response_body: r.get(9)?,
            assert_results: r.get(10)?,
            is_success: r.get(11)?,
            error_msg: r.get(12)?,
            duration_ms: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// xlsx 导出：汇总 sheet + 明细 sheet
fn export_xlsx(detail: &ReportDetail, path: &Path) -> Result<(), AppError> {
    use rust_xlsxwriter::{Format, Workbook};

    let mut workbook = Workbook::new();

    let header_fmt = Format::new().set_bold();

    // ---- 汇总 sheet ----
    let summary = workbook.add_worksheet();
    summary.set_name("汇总")?;
    let s_headers = [
        "报告ID",
        "报告名称",
        "执行总数",
        "成功数",
        "失败数",
        "通过率",
        "执行环境",
        "baseUrl",
        "开始时间",
        "总耗时(ms)",
    ];
    for (c, h) in s_headers.iter().enumerate() {
        summary.write_string_with_format(0, c as u16, *h, &header_fmt)?;
    }
    let rate = if detail.report.total_count > 0 {
        format!(
            "{:.1}%",
            detail.report.success_count as f64 / detail.report.total_count as f64 * 100.0
        )
    } else {
        "-".to_string()
    };
    let s_vals = [
        detail.report.id.to_string(),
        detail.report.name.clone(),
        detail.report.total_count.to_string(),
        detail.report.success_count.to_string(),
        detail.report.fail_count.to_string(),
        rate,
        detail.report.env_name.clone(),
        detail.report.base_url.clone(),
        detail.report.started_at.clone(),
        detail.report.duration_ms.to_string(),
    ];
    for (c, v) in s_vals.iter().enumerate() {
        summary.write_string(1, c as u16, v)?;
    }
    summary.set_column_width(0, 8)?;
    summary.set_column_width(1, 28)?;
    summary.set_column_width(6, 14)?;
    summary.set_column_width(7, 30)?;

    // ---- 明细 sheet ----（汇总写完后再借用 workbook，避免同时两次可变借用）
    let detail_ws = workbook.add_worksheet();
    detail_ws.set_name("明细")?;
    let d_headers = [
        "序号",
        "用例名称",
        "方法",
        "请求地址",
        "状态码",
        "结果",
        "耗时(ms)",
        "错误信息",
        "校验结果",
        "请求快照(JSON)",
        "响应头(JSON)",
        "响应体",
    ];
    for (c, h) in d_headers.iter().enumerate() {
        detail_ws.write_string_with_format(0, c as u16, *h, &header_fmt)?;
    }
    for (i, it) in detail.items.iter().enumerate() {
        let row = (i + 1) as u32;
        let verdict = if it.is_success {
            "通过".to_string()
        } else if it.response_status.is_some() {
            "失败".to_string()
        } else {
            "错误".to_string()
        };
        let assert_summary = summarize_assert(&it.assert_results);
        let status = it
            .response_status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());
        let vals = [
            (i + 1).to_string(),
            it.case_name.clone(),
            it.method.clone(),
            it.url.clone(),
            status,
            verdict,
            it.duration_ms.to_string(),
            it.error_msg.clone(),
            assert_summary,
            it.request_snapshot.clone(),
            it.response_headers.clone(),
            it.response_body.clone(),
        ];
        for (c, v) in vals.iter().enumerate() {
            detail_ws.write_string(row, c as u16, v)?;
        }
    }
    detail_ws.set_column_width(0, 6)?;
    detail_ws.set_column_width(1, 24)?;
    detail_ws.set_column_width(3, 36)?;
    detail_ws.set_column_width(7, 30)?;
    detail_ws.set_column_width(8, 40)?;
    detail_ws.set_column_width(9, 40)?;
    detail_ws.set_column_width(10, 30)?;
    detail_ws.set_column_width(11, 50)?;

    workbook.save(path)?;
    Ok(())
}

/// 把 assert_results JSON 数组压成多行文本摘要
fn summarize_assert(raw: &str) -> String {
    let v: Value = match serde_json::from_str(raw) {
        Ok(x) => x,
        Err(_) => return String::new(),
    };
    let arr = match v.as_array() {
        Some(a) => a,
        None => return String::new(),
    };
    arr.iter()
        .map(|item| {
            let rule = item.get("rule").and_then(|x| x.as_str()).unwrap_or("");
            let verdict = item.get("verdict").and_then(|x| x.as_str()).unwrap_or("");
            let msg = item.get("message").and_then(|x| x.as_str()).unwrap_or("");
            format!("[{verdict}] {rule}：{msg}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;

    fn setup() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO reports (id, name, total_count, success_count, fail_count, env_name, base_url, duration_ms)
             VALUES (1, 'R1', 3, 2, 1, 'dev', 'http://d', 1200),
                    (2, 'R2', 1, 1, 0, 'test', 'http://t', 300)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO report_items (report_id, case_id, case_name, method, url, response_status, assert_results, is_success, error_msg, duration_ms)
             VALUES (1, 1, 'c1', 'GET', 'http://d/a', 200, '[]', 1, '', 100),
                    (1, 2, 'c2', 'POST', 'http://d/b', 500, '[{\"rule\":\"status == 200\",\"verdict\":\"fail\",\"message\":\"x\"}]', 0, 'status fail', 200),
                    (1, 3, 'c3', 'GET', 'http://d/c', NULL, '[]', 0, 'timeout', 900),
                    (2, 4, 'c4', 'GET', 'http://t/x', 200, '[]', 1, '', 300)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn page_filters_work() {
        let conn = setup();
        // 全部
        let p = report_page_inner(&conn, ReportQuery::default()).unwrap();
        assert_eq!(p.total, 2);
        // 空字符串时间过滤参数应视为未传（回归：曾因 started_at <= '' 恒假查不到数据）
        let p = report_page_inner(
            &conn,
            ReportQuery {
                start: Some(String::new()),
                end: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.total, 2);
        // 仅含失败
        let p = report_page_inner(
            &conn,
            ReportQuery {
                has_fail: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.total, 1);
        assert_eq!(p.list[0].name, "R1");
        // 仅全成功
        let p = report_page_inner(
            &conn,
            ReportQuery {
                has_fail: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.total, 1);
        assert_eq!(p.list[0].name, "R2");
        // 名称模糊
        let p = report_page_inner(
            &conn,
            ReportQuery {
                keyword: Some("R2".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.total, 1);
    }

    #[test]
    fn detail_returns_items_in_order() {
        let conn = setup();
        let d = report_detail_inner(&conn, 1).unwrap();
        assert_eq!(d.items.len(), 3);
        assert_eq!(d.items[0].case_name, "c1");
        assert_eq!(d.report.fail_count, 1);
    }

    #[test]
    fn delete_cascades_items() {
        let conn = setup();
        let n = report_delete_inner(&conn, vec![1]).unwrap();
        assert_eq!(n, 1);
        // 明细应被级联删除
        let remain: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM report_items WHERE report_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remain, 0);
        // R2 仍在
        let n2: i64 = conn
            .query_row("SELECT COUNT(*) FROM reports", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n2, 1);
    }

    #[test]
    fn export_json_roundtrip() {
        let conn = setup();
        let dir = std::env::temp_dir();
        let path = dir.join("m6_test_report.json");
        let ret = report_export_inner(&conn, 1, "json", &path.to_string_lossy()).unwrap();
        assert_eq!(ret, path.to_string_lossy());
        let txt = std::fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&txt).unwrap();
        assert_eq!(v["report"]["name"], "R1");
        assert_eq!(v["items"].as_array().unwrap().len(), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_xlsx_writes_two_sheets() {
        let conn = setup();
        let dir = std::env::temp_dir();
        let path = dir.join("m6_test_report.xlsx");
        let ret = report_export_inner(&conn, 1, "xlsx", &path.to_string_lossy()).unwrap();
        assert_eq!(ret, path.to_string_lossy());
        let _ = std::fs::remove_file(&path);
    }
}
