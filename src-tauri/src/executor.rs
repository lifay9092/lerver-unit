//! 执行引擎（FR-C5~C8、C10、E2/E6）
//!
//! - `run_cases`：按当前激活环境解析 URL，串行执行一组用例，逐条写报告明细，
//!   并通过 Tauri 事件 `run-progress` 向前端实时推送进度。
//! - `try_run`：单条试运行，不落库，供用例编辑抽屉「发送试运行」调试用。
//! - 报告记录执行时的环境快照（env_name + base_url，FR-E6）。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::assert_expr::{eval, parse, AssertItem};
use crate::cases::{fetch_case, CaseInput, CaseRow};
use crate::env_config;
use crate::error::AppError;
use crate::AppState;

/// 实时进度事件负载（前端 listen("run-progress")）
#[derive(Debug, Clone, Serialize)]
pub struct RunProgress {
    pub report_id: i64,
    pub total: usize,
    pub current: usize,
    pub case_id: i64,
    pub case_name: String,
    /// running（请求中）/ done（已完成）
    pub phase: String,
    /// 完成时的判定：pass / fail / error
    pub verdict: String,
    pub detail: String,
    pub duration_ms: u64,
}

/// 试运行结果（不落库）
#[derive(Debug, Clone, Serialize)]
pub struct TryRunResult {
    pub resolved_url: String,
    pub status: Option<u16>,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub duration_ms: u64,
    pub error_msg: String,
    pub assert_results: Vec<AssertItem>,
}

/// 单条用例执行落库快照
#[derive(Debug, Default)]
struct ItemSnapshot {
    case_id: Option<i64>,
    case_name: String,
    method: String,
    url: String,
    request_snapshot: String,
    response_status: Option<u16>,
    response_headers: String,
    response_body: String,
    assert_results: String,
    is_success: bool,
    error_msg: String,
    duration_ms: u64,
}

// ============ 命令 ============

/// 批量执行（FR-C6/C7）：串行执行，逐条写 report_items，实时推送进度
#[tauri::command]
pub async fn run_cases(
    case_ids: Vec<i64>,
    report_name: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<i64, AppError> {
    // 执行前重新读取环境配置（FR-E4），用当前激活环境
    let env = env_config::load_or_create(&state.data_dir)?;
    let base_url = env.active_base_url().to_string();
    let env_name = env.active_name().to_string();

    let total = case_ids.len();
    let (report_id, _name) = {
        let conn = state.db.lock().unwrap();
        let name = report_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_report_name(&conn));
        conn.execute(
            "INSERT INTO reports (name, total_count, success_count, fail_count, env_name, base_url, duration_ms)
             VALUES (?1, ?2, 0, 0, ?3, ?4, 0)",
            params![name, total as i64, env_name, base_url],
        )?;
        (conn.last_insert_rowid(), name)
    };

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| AppError::new(format!("HTTP 客户端创建失败：{e}")))?;

    let start = Instant::now();
    let mut success: u64 = 0;
    let mut fail: u64 = 0;

    for (idx, &cid) in case_ids.iter().enumerate() {
        // 取用例
        let case = match {
            let conn = state.db.lock().unwrap();
            fetch_case(&conn, cid)?
        } {
            Some(mut c) => {
                // 合并当前激活环境的通用请求头（用例级覆盖环境级，FR-E7）
                c.headers = match env.active_env() {
                    Some(e) => e.merged_headers(&c.headers),
                    None => c.headers.clone(),
                };
                c
            }
            None => {
                let it = ItemSnapshot {
                    case_id: Some(cid),
                    case_name: format!("用例 #{cid}"),
                    is_success: false,
                    error_msg: "用例不存在或已被删除".into(),
                    ..Default::default()
                };
                {
                    let conn = state.db.lock().unwrap();
                    insert_report_item(&conn, report_id, &it)?;
                }
                fail += 1;
                emit_progress(
                    &app,
                    &RunProgress {
                        report_id,
                        total,
                        current: idx + 1,
                        case_id: cid,
                        case_name: it.case_name.clone(),
                        phase: "done".into(),
                        verdict: "error".into(),
                        detail: it.error_msg.clone(),
                        duration_ms: 0,
                    },
                )?;
                continue;
            }
        };

        // 运行中进度
        emit_progress(
            &app,
            &RunProgress {
                report_id,
                total,
                current: idx + 1,
                case_id: case.id,
                case_name: case.case_name.clone(),
                phase: "running".into(),
                verdict: "".into(),
                detail: "请求中…".into(),
                duration_ms: 0,
            },
        )?;

        // 解析 URL（按当前环境 baseUrl）
        let resolved = match env_config::resolve_url(&case.url, &base_url) {
            Ok(u) => u,
            Err(e) => {
                let mut it = ItemSnapshot {
                    case_id: Some(case.id),
                    case_name: case.case_name.clone(),
                    method: case.method.clone(),
                    url: case.url.clone(),
                    is_success: false,
                    error_msg: e,
                    ..Default::default()
                };
                it.request_snapshot = request_snapshot(&case, "");
                {
                    let conn = state.db.lock().unwrap();
                    insert_report_item(&conn, report_id, &it)?;
                }
                fail += 1;
                emit_progress(
                    &app,
                    &RunProgress {
                        report_id,
                        total,
                        current: idx + 1,
                        case_id: case.id,
                        case_name: case.case_name.clone(),
                        phase: "done".into(),
                        verdict: "error".into(),
                        detail: it.error_msg.clone(),
                        duration_ms: 0,
                    },
                )?;
                continue;
            }
        };

        // 组装请求
        let rb = match build_request(&client, &case, &resolved) {
            Ok(rb) => rb,
            Err(e) => {
                let mut it = ItemSnapshot {
                    case_id: Some(case.id),
                    case_name: case.case_name.clone(),
                    method: case.method.clone(),
                    url: resolved.clone(),
                    is_success: false,
                    error_msg: e.to_string(),
                    ..Default::default()
                };
                it.request_snapshot = request_snapshot(&case, &resolved);
                {
                    let conn = state.db.lock().unwrap();
                    insert_report_item(&conn, report_id, &it)?;
                }
                fail += 1;
                emit_progress(
                    &app,
                    &RunProgress {
                        report_id,
                        total,
                        current: idx + 1,
                        case_id: case.id,
                        case_name: case.case_name.clone(),
                        phase: "done".into(),
                        verdict: "error".into(),
                        detail: it.error_msg.clone(),
                        duration_ms: 0,
                    },
                )?;
                continue;
            }
        };

        let timeout = Duration::from_secs(case.timeout_secs.max(1) as u64);
        let started = Instant::now();
        let resp_result = rb.timeout(timeout).send().await;
        let dur = started.elapsed().as_millis() as u64;

        let (verdict, it) = match resp_result {
            Err(e) => {
                let msg = if e.is_timeout() {
                    format!("请求超时（{}s）：{}", timeout.as_secs(), e)
                } else {
                    format!("请求失败：{e}")
                };
                (
                    "error".to_string(),
                    ItemSnapshot {
                        case_id: Some(case.id),
                        case_name: case.case_name.clone(),
                        method: case.method.clone(),
                        url: resolved.clone(),
                        request_snapshot: request_snapshot(&case, &resolved),
                        response_status: None,
                        response_headers: "{}".into(),
                        response_body: "".into(),
                        assert_results: "[]".into(),
                        is_success: false,
                        error_msg: msg,
                        duration_ms: dur,
                    },
                )
            }
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers = collect_headers(&resp);
                let body_text = resp.text().await.unwrap_or_default();
                let body_json: Option<Value> = serde_json::from_str(&body_text).ok();
                let truncated = truncate(&body_text, 65536);
                let rules = parse(&case.assert_expr).unwrap_or_default();
                let assert_results: Vec<AssertItem> = rules
                    .iter()
                    .map(|r| eval(r, status, body_json.as_ref(), &headers))
                    .collect();
                let ok = if rules.is_empty() {
                    true
                } else {
                    assert_results.iter().all(|a| a.verdict == "pass")
                };
                let assert_json =
                    serde_json::to_string(&assert_results).unwrap_or_else(|_| "[]".into());
                let verdict = if ok { "pass" } else { "fail" }.to_string();
                let err_summary = if ok {
                    "".into()
                } else {
                    assert_results
                        .iter()
                        .find(|a| a.verdict != "pass")
                        .map(|a| format!("{}：{}", a.rule, a.message))
                        .unwrap_or_default()
                };
                (
                    verdict,
                    ItemSnapshot {
                        case_id: Some(case.id),
                        case_name: case.case_name.clone(),
                        method: case.method.clone(),
                        url: resolved.clone(),
                        request_snapshot: request_snapshot(&case, &resolved),
                        response_status: Some(status),
                        response_headers: serde_json::to_string(&headers)
                            .unwrap_or_else(|_| "{}".into()),
                        response_body: truncated,
                        assert_results: assert_json,
                        is_success: ok,
                        error_msg: err_summary,
                        duration_ms: dur,
                    },
                )
            }
        };

        {
            let conn = state.db.lock().unwrap();
            insert_report_item(&conn, report_id, &it)?;
        }
        if it.is_success {
            success += 1;
        } else {
            fail += 1;
        }
        emit_progress(
            &app,
            &RunProgress {
                report_id,
                total,
                current: idx + 1,
                case_id: case.id,
                case_name: case.case_name.clone(),
                phase: "done".into(),
                verdict,
                detail: it.error_msg.clone(),
                duration_ms: dur,
            },
        )?;
    }

    let duration = start.elapsed().as_millis() as i64;
    {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "UPDATE reports SET success_count = ?1, fail_count = ?2, duration_ms = ?3 WHERE id = ?4",
            params![success as i64, fail as i64, duration, report_id],
        )?;
    }
    Ok(report_id)
}

/// 单条试运行（FR-C10）：按当前环境解析 URL，发送但不落库
#[tauri::command]
pub async fn try_run(
    input: CaseInput,
    state: State<'_, AppState>,
) -> Result<TryRunResult, AppError> {
    let env = env_config::load_or_create(&state.data_dir)?;
    let base_url = env.active_base_url().to_string();
    let resolved = env_config::resolve_url(&input.url, &base_url).map_err(AppError::new)?;

    let mut case = CaseRow {
        id: input.id.unwrap_or(0),
        group_id: input.group_id,
        group_path: None,
        case_name: input.case_name.clone(),
        method: input.method.clone(),
        content_type: input.content_type.clone(),
        url: input.url.clone(),
        headers: input.headers.clone().unwrap_or_default(),
        query_params: input.query_params.clone().unwrap_or_default(),
        request_body: input.request_body.clone().unwrap_or_default(),
        assert_expr: input.assert_expr.clone().unwrap_or_default(),
        timeout_secs: input.timeout_secs.unwrap_or(5),
        remark: input.remark.clone().unwrap_or_default(),
        created_at: "".into(),
        updated_at: "".into(),
    };
    // 合并当前激活环境的通用请求头（FR-E7）
    case.headers = match env.active_env() {
        Some(e) => e.merged_headers(&case.headers),
        None => case.headers.clone(),
    };

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| AppError::new(format!("HTTP 客户端创建失败：{e}")))?;
    let rb = build_request(&client, &case, &resolved)?;
    let timeout = Duration::from_secs(case.timeout_secs.max(1) as u64);
    let started = Instant::now();
    let result = rb.timeout(timeout).send().await;
    let dur = started.elapsed().as_millis() as u64;

    match result {
        Err(e) => Ok(TryRunResult {
            resolved_url: resolved,
            status: None,
            headers: HashMap::new(),
            body: "".into(),
            duration_ms: dur,
            error_msg: if e.is_timeout() {
                format!("请求超时（{}s）", timeout.as_secs())
            } else {
                format!("请求失败：{e}")
            },
            assert_results: vec![],
        }),
        Ok(resp) => {
            let status = resp.status().as_u16();
            let headers = collect_headers(&resp);
            let body_text = resp.text().await.unwrap_or_default();
            let body_json: Option<Value> = serde_json::from_str(&body_text).ok();
            let rules = parse(&input.assert_expr.unwrap_or_default()).unwrap_or_default();
            let assert_results: Vec<AssertItem> = rules
                .iter()
                .map(|r| eval(r, status, body_json.as_ref(), &headers))
                .collect();
            Ok(TryRunResult {
                resolved_url: resolved,
                status: Some(status),
                headers,
                body: body_text,
                duration_ms: dur,
                error_msg: "".into(),
                assert_results,
            })
        }
    }
}

// ============ 辅助 ============

fn insert_report_item(conn: &Connection, report_id: i64, it: &ItemSnapshot) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO report_items (report_id, case_id, case_name, method, url, request_snapshot,
            response_status, response_headers, response_body, assert_results, is_success, error_msg, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            report_id,
            it.case_id,
            it.case_name,
            it.method,
            it.url,
            it.request_snapshot,
            it.response_status,
            it.response_headers,
            it.response_body,
            it.assert_results,
            it.is_success as i64,
            it.error_msg,
            it.duration_ms as i64
        ],
    )?;
    Ok(())
}

/// 按 content_type 组装请求（FR-E2：url 已解析为完整地址）
fn build_request(
    client: &reqwest::Client,
    case: &CaseRow,
    resolved_url: &str,
) -> Result<reqwest::RequestBuilder, AppError> {
    let method = reqwest::Method::from_bytes(case.method.as_bytes())
        .map_err(|_| AppError::new(format!("不支持的请求方法：{}", case.method)))?;
    let mut rb = client.request(method, resolved_url);

    // 请求头
    let mut header_map = HeaderMap::new();
    if !case.headers.trim().is_empty() {
        let hm: HashMap<String, Value> = serde_json::from_str(&case.headers)
            .map_err(|e| AppError::new(format!("请求头不是合法 JSON：{e}")))?;
        for (k, v) in hm {
            let val = match v {
                Value::String(s) => s,
                other => other.to_string(),
            };
            if let (Ok(name), Ok(value)) =
                (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&val))
            {
                header_map.insert(name, value);
            }
        }
    }
    if case.content_type != "none" && !header_map.contains_key(CONTENT_TYPE) {
        if let Ok(value) = HeaderValue::from_str(&case.content_type) {
            header_map.insert(CONTENT_TYPE, value);
        }
    }
    rb = rb.headers(header_map);

    // 查询参数
    if !case.query_params.trim().is_empty() {
        let qm: HashMap<String, Value> = serde_json::from_str(&case.query_params)
            .map_err(|e| AppError::new(format!("查询参数不是合法 JSON：{e}")))?;
        let q: Vec<(String, String)> = qm
            .into_iter()
            .map(|(k, v)| {
                let val = match v {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                (k, val)
            })
            .collect();
        rb = rb.query(&q);
    }

    // 请求体
    match case.content_type.as_str() {
        "none" => {}
        "multipart/form-data" => {
            if !case.request_body.trim().is_empty() {
                let b: HashMap<String, Value> = serde_json::from_str(&case.request_body)
                    .map_err(|e| AppError::new(format!("请求体不是合法 JSON：{e}")))?;
                let mut form = reqwest::multipart::Form::new();
                for (k, v) in b {
                    let val = match v {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    form = form.text(k, val);
                }
                rb = rb.multipart(form);
            }
        }
        _ => {
            if !case.request_body.trim().is_empty() {
                rb = rb.body(case.request_body.clone());
            }
        }
    }
    Ok(rb)
}

fn collect_headers(resp: &reqwest::Response) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for (k, v) in resp.headers() {
        if let Ok(s) = v.to_str() {
            m.insert(k.as_str().to_string(), s.to_string());
        }
    }
    m
}

/// 请求快照（JSON 字符串）
fn request_snapshot(case: &CaseRow, resolved_url: &str) -> String {
    json!({
        "method": case.method,
        "url": resolved_url,
        "content_type": case.content_type,
        "headers": safe_json(&case.headers),
        "query_params": safe_json(&case.query_params),
        "request_body": case.request_body,
    })
    .to_string()
}

fn safe_json(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or(Value::Null)
}

/// 截断超长响应体（报告存储上限，FR 风险项）
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
    format!("{}…(已截断，原长{}字符)", &s[..cut], s.chars().count())
}

fn default_report_name(conn: &Connection) -> String {
    conn.query_row(
        "SELECT strftime('%Y%m%d-%H%M%S','now','localtime')",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "report".into())
}

fn emit_progress(app: &AppHandle, p: &RunProgress) -> Result<(), AppError> {
    app.emit("run-progress", p)
        .map_err(|e| AppError::new(format!("进度推送失败：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundary() {
        let s = "你好world";
        assert_eq!(truncate(s, 2), "你好…(已截断，原长7字符)");
        assert_eq!(truncate(s, 100), s);
    }

    #[test]
    fn request_snapshot_is_valid_json() {
        let case = CaseRow {
            id: 1,
            group_id: Some(1),
            group_path: None,
            case_name: "x".into(),
            method: "POST".into(),
            content_type: "application/json".into(),
            url: "/a".into(),
            headers: "{\"Authorization\":\"Bearer t\"}".into(),
            query_params: "{\"p\":1}".into(),
            request_body: "{\"k\":\"v\"}".into(),
            assert_expr: "".into(),
            timeout_secs: 30,
            remark: "".into(),
            created_at: "".into(),
            updated_at: "".into(),
        };
        let snap = request_snapshot(&case, "http://x/a");
        let v: Value = serde_json::from_str(&snap).unwrap();
        assert_eq!(v["method"], "POST");
        assert_eq!(v["url"], "http://x/a");
        assert_eq!(v["headers"]["Authorization"], "Bearer t");
    }

    #[test]
    fn default_report_name_format() {
        // 不依赖真实 DB 时间格式，仅验证返回非空且为 13 位 yyyyMMdd-HHmmss
        let conn = Connection::open_in_memory().unwrap();
        let name = default_report_name(&conn);
        assert_eq!(name.len(), 15);
        assert!(name.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }
}
