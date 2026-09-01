//! 测试用例批量导出 / 导入（固定 JSON 格式，FR-C12 / FR-C13）
//!
//! 固定格式（与模板导入解耦，便于任意分组 / 用例范围间快速搬运用例）：
//! ```json
//! {
//!   "format": "lever-unit-case-export",
//!   "version": 1,
//!   "exported_at": "2026-08-31 12:00:00",
//!   "group_path": "项目A / 用户模块",      // 仅参考信息
//!   "cases": [ { "case_name": "...", "method": "GET", ... } ]
//! }
//! ```
//! `cases[]` 内未知字段宽容忽略；顶层 format 不符 / version 超前则明确报错。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::cases::{fetch_case, insert_case, validate_case, CaseInput};
use crate::error::AppError;
use crate::import::{ImportFailure, ImportIgnored, ImportPreviewRow, ImportResult};
use crate::AppState;

const EXPORT_FORMAT: &str = "lever-unit-case-export";
const SUPPORTED_VERSION: u32 = 1;
/// 超时默认值（FR-C2：不填默认 5s）。导出时若该值与默认一致则写 null（表达「使用默认」），
/// 导入时 null/缺失再回退为 5，保证「默认语义」往返无损。
const DEFAULT_TIMEOUT: i64 = 5;

// ============ 入参（遵循 M3+ 约定：snake_case + deny_unknown_fields） ============

/// 导出入参
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseExportInput {
    pub scope: ExportScope,
    pub file_path: String,
}

/// 导出范围
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportScope {
    /// "all" | "group" | "selected"
    pub kind: String,
    pub group_id: Option<i64>,
    pub recursive: Option<bool>,
    pub case_ids: Option<Vec<i64>>,
}

/// 导出结果摘要
#[derive(Debug, Serialize)]
pub struct ExportSummary {
    pub file_path: String,
    pub count: usize,
}

// ============ 导出 ============

#[tauri::command]
pub fn case_export_json(
    input: CaseExportInput,
    state: State<'_, AppState>,
) -> Result<ExportSummary, AppError> {
    let conn = state.db.lock().unwrap();
    case_export_json_impl(&conn, &input.scope, &input.file_path)
}

/// 核心导出逻辑（纯函数，便于单测）
fn case_export_json_impl(
    conn: &Connection,
    scope: &ExportScope,
    file_path: &str,
) -> Result<ExportSummary, AppError> {
    let ids = collect_ids(conn, scope)?;
    let mut cases = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(row) = fetch_case(conn, id)? {
            cases.push(ExportedCase::from_row(&row));
        }
    }

    // 分组路径（仅 group 范围有意义）
    let group_path: Option<String> = match (scope.kind.as_str(), scope.group_id) {
        ("group", Some(gid)) => group_path(conn, gid),
        _ => None,
    };
    let exported_at: String = conn
        .query_row("SELECT datetime('now','localtime')", [], |r| r.get::<_, String>(0))
        .unwrap_or_default();

    let payload = serde_json::json!({
        "format": EXPORT_FORMAT,
        "version": SUPPORTED_VERSION,
        "exported_at": exported_at,
        "group_path": group_path,
        "cases": cases,
    });
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|e| AppError::new(format!("序列化导出数据失败：{e}")))?;
    std::fs::write(file_path, text)
        .map_err(|e| AppError::new(format!("写入文件失败：{e}")))?;
    Ok(ExportSummary {
        file_path: file_path.to_string(),
        count: cases.len(),
    })
}

/// 计算分组全路径（groups 表不存储 path，按 parent_id 回溯，与 groups.rs load_rows 一致）
fn group_path(conn: &Connection, group_id: i64) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur: Option<i64> = Some(group_id);
    let mut guard = 0;
    while let Some(id) = cur {
        let row: Option<(String, Option<i64>)> = conn
            .query_row(
                "SELECT name, parent_id FROM groups WHERE id = ?1",
                params![id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .ok();
        match row {
            Some((name, parent)) => {
                parts.push(name);
                cur = parent;
            }
            None => break,
        }
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    parts.reverse();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" / "))
    }
}

/// 按范围收集用例 id
fn collect_ids(conn: &Connection, scope: &ExportScope) -> Result<Vec<i64>, AppError> {
    match scope.kind.as_str() {
        "all" => {
            let ids: Vec<i64> = conn
                .prepare("SELECT id FROM test_cases ORDER BY id")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(ids)
        }
        "group" => {
            let gid = scope
                .group_id
                .ok_or_else(|| AppError::new("导出分组范围缺少 group_id"))?;
            let _: i64 = conn
                .query_row("SELECT id FROM groups WHERE id = ?1", params![gid], |r| r.get(0))
                .map_err(|_| AppError::new("指定的分组不存在"))?;
            let recursive = scope.recursive.unwrap_or(true);
            let sql = if recursive {
                "WITH RECURSIVE sub(id) AS (
                    SELECT ?1 UNION ALL SELECT g.id FROM groups g JOIN sub ON g.parent_id = sub.id
                 )
                 SELECT id FROM test_cases WHERE group_id IN (SELECT id FROM sub) ORDER BY id"
            } else {
                "SELECT id FROM test_cases WHERE group_id = ?1 ORDER BY id"
            };
            let ids: Vec<i64> = conn
                .prepare(sql)?
                .query_map(params![gid], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(ids)
        }
        "selected" => scope
            .case_ids
            .clone()
            .ok_or_else(|| AppError::new("导出选中范围缺少 case_ids")),
        other => Err(AppError::new(format!(
            "不支持的导出范围类型：{other}（应为 all / group / selected）"
        ))),
    }
}

/// 固定导出格式中的单个用例（字段与 CaseInput 一致；全部可选以宽容解析外部文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportedCase {
    #[serde(default)]
    case_name: String,
    #[serde(default)]
    method: String,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: Option<String>,
    #[serde(default)]
    query_params: Option<String>,
    #[serde(default)]
    request_body: Option<String>,
    #[serde(default)]
    assert_expr: Option<String>,
    #[serde(default)]
    timeout_secs: Option<i64>,
    #[serde(default)]
    remark: Option<String>,
}

impl ExportedCase {
    /// 用例行 → 导出对象：空字符串字段归并为 None（JSON 中省略），超时等于默认值则写 null
    fn from_row(r: &crate::cases::CaseRow) -> Self {
        ExportedCase {
            case_name: r.case_name.clone(),
            method: r.method.clone(),
            content_type: r.content_type.clone(),
            url: r.url.clone(),
            headers: Some(if r.headers.trim().is_empty() {
                "{}".into()
            } else {
                r.headers.clone()
            }),
            query_params: Some(if r.query_params.trim().is_empty() {
                "{}".into()
            } else {
                r.query_params.clone()
            }),
            request_body: if r.request_body.trim().is_empty() {
                None
            } else {
                Some(r.request_body.clone())
            },
            assert_expr: if r.assert_expr.trim().is_empty() {
                None
            } else {
                Some(r.assert_expr.clone())
            },
            timeout_secs: if r.timeout_secs <= 0 || r.timeout_secs == DEFAULT_TIMEOUT {
                None
            } else {
                Some(r.timeout_secs)
            },
            remark: if r.remark.trim().is_empty() {
                None
            } else {
                Some(r.remark.clone())
            },
        }
    }

    /// 导出对象 → CaseInput（导入用，group_id 由调用方填充）
    fn to_input(&self) -> CaseInput {
        CaseInput {
            id: None,
            group_id: None,
            case_name: self.case_name.clone(),
            method: self.method.clone(),
            // 缺省内容类型宽容处理为 application/json（与模板导入 build_case 行为一致）
            content_type: if self.content_type.trim().is_empty() {
                "application/json".to_string()
            } else {
                self.content_type.clone()
            },
            url: self.url.clone(),
            headers: self.headers.clone(),
            query_params: self.query_params.clone(),
            request_body: self.request_body.clone(),
            assert_expr: self.assert_expr.clone(),
            timeout_secs: self.timeout_secs,
            remark: self.remark.clone(),
        }
    }
}

// ============ 导入（固定 JSON） ============

/// 解析中间结构（不序列化）
#[derive(Debug, Clone)]
struct ParsedExportCase {
    row: usize,
    case_input: CaseInput,
    status: ExpRowStatus,
    will_auto_name: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum ExpRowStatus {
    Ok,
    Ignored,
    Error,
}

/// 解析固定格式文件并逐条校验（预览与提交共用）
fn parse_export_file(file_path: &str) -> Result<Vec<ParsedExportCase>, AppError> {
    let txt = std::fs::read_to_string(file_path)
        .map_err(|e| AppError::new(format!("读取文件失败：{e}")))?;
    let v: Value =
        serde_json::from_str(&txt).map_err(|e| AppError::new(format!("JSON 解析失败：{e}")))?;

    // 顶层 format 校验（明确报错）
    let format = v.get("format").and_then(|x| x.as_str()).unwrap_or("");
    if format != EXPORT_FORMAT {
        return Err(AppError::new(format!(
            "文件不是 LeverUnit 用例导出文件：期望 format=\"{EXPORT_FORMAT}\"，实际为 \"{format}\""
        )));
    }
    // version 超前 → 提示升级
    let version = v.get("version").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    if version > SUPPORTED_VERSION {
        return Err(AppError::new(format!(
            "文件版本过新（version={version}，当前最高支持 {SUPPORTED_VERSION}），请升级 LeverUnit 后重试"
        )));
    }
    let cases_val = v
        .get("cases")
        .and_then(|x| x.as_array())
        .ok_or_else(|| AppError::new("文件中缺少 cases 数组"))?;

    let mut out = Vec::with_capacity(cases_val.len());
    for (i, cv) in cases_val.iter().enumerate() {
        // cases[] 内未知字段宽容忽略
        let c: ExportedCase = serde_json::from_value(cv.clone())
            .map_err(|e| AppError::new(format!("第 {} 条用例解析失败：{}", i + 1, e)))?;
        let input = c.to_input();
        // 请求地址为空 → 忽略（不计失败，与 FR-C4 一致）
        if input.url.trim().is_empty() {
            out.push(ParsedExportCase {
                row: i + 1,
                case_input: input,
                status: ExpRowStatus::Ignored,
                will_auto_name: false,
                error: None,
            });
            continue;
        }
        // 导入场景允许用例名称为空（提交时自动命名）
        let error = validate_case(&input, true).err().map(|e| e.msg);
        let status = if error.is_some() {
            ExpRowStatus::Error
        } else {
            ExpRowStatus::Ok
        };
        let will_auto_name = status == ExpRowStatus::Ok && input.case_name.trim().is_empty();
        out.push(ParsedExportCase {
            row: i + 1,
            case_input: input,
            status,
            will_auto_name,
            error,
        });
    }
    Ok(out)
}

/// 导入预览（不落库）：返回与模板导入一致的 ImportPreviewRow
#[tauri::command]
pub fn case_import_json_preview(file_path: String) -> Result<Vec<ImportPreviewRow>, AppError> {
    let parsed = parse_export_file(&file_path)?;
    Ok(parsed
        .into_iter()
        .map(|p| {
            let status_str = match p.status {
                ExpRowStatus::Ok => "ok",
                ExpRowStatus::Ignored => "ignored",
                ExpRowStatus::Error => "error",
            }
            .to_string();
            ImportPreviewRow {
                row: p.row,
                case_input: p.case_input,
                status: status_str,
                will_auto_name: p.will_auto_name,
                error: p.error,
            }
        })
        .collect())
}

/// 导入提交：复用 M8 的「忽略空 url + 空名自动命名（分组名_序号）」规则
#[tauri::command]
pub fn case_import_json_commit(
    group_id: i64,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<ImportResult, AppError> {
    let conn = state.db.lock().unwrap();
    case_import_json_commit_impl(&conn, group_id, &file_path)
}

fn case_import_json_commit_impl(
    conn: &Connection,
    group_id: i64,
    file_path: &str,
) -> Result<ImportResult, AppError> {
    let _: i64 = conn
        .query_row("SELECT id FROM groups WHERE id = ?1", params![group_id], |r| r.get(0))
        .map_err(|_| AppError::new("目标分组不存在"))?;
    let group_name: String = conn
        .query_row("SELECT name FROM groups WHERE id = ?1", params![group_id], |r| r.get(0))
        .unwrap_or_default();
    let existing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM test_cases WHERE group_id = ?1",
            params![group_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let parsed = parse_export_file(file_path)?;

    let mut success_count = 0usize;
    let mut failed: Vec<ImportFailure> = Vec::new();
    let mut ignored: Vec<ImportIgnored> = Vec::new();
    // 自动命名序号：分组现有用例数 +1 起，本次导入内连续递增
    let mut seq: i64 = existing;

    for p in &parsed {
        // 请求地址为空 → 忽略
        if p.status == ExpRowStatus::Ignored {
            ignored.push(ImportIgnored {
                row: p.row,
                reason: "请求地址为空，已忽略".into(),
            });
            continue;
        }
        let case_name = p.case_input.case_name.trim().to_string();
        if let Some(err) = &p.error {
            failed.push(ImportFailure {
                row: p.row,
                case_name,
                reason: err.clone(),
            });
            continue;
        }
        let mut input = p.case_input.clone();
        input.group_id = Some(group_id);
        // 用例名称为空 → 自动命名「分组名_序号」
        if input.case_name.trim().is_empty() {
            seq += 1;
            let gname = if group_name.trim().is_empty() {
                "group"
            } else {
                group_name.trim()
            };
            input.case_name = format!("{}_{}", gname, seq);
        }
        match validate_case(&input, true).and_then(|_| insert_case(conn, &input)) {
            Ok(_) => success_count += 1,
            Err(e) => failed.push(ImportFailure {
                row: p.row,
                case_name: input.case_name.clone(),
                reason: e.msg,
            }),
        }
    }
    Ok(ImportResult {
        success_count,
        failed,
        ignored_count: ignored.len(),
        ignored,
    })
}

// ============ 单元测试 ============

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    fn tmp_json(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    fn insert_case_with(
        conn: &Connection,
        group_id: i64,
        name: &str,
        url: &str,
        timeout: i64,
    ) -> i64 {
        let input = CaseInput {
            id: None,
            group_id: Some(group_id),
            case_name: name.into(),
            method: "POST".into(),
            content_type: "application/json".into(),
            url: url.into(),
            headers: Some("{\"Authorization\":\"x\"}".into()),
            query_params: Some("{\"id\":1}".into()),
            request_body: Some("{\"a\":1}".into()),
            assert_expr: Some("status == 200".into()),
            timeout_secs: Some(timeout),
            remark: Some("备注".into()),
        };
        insert_case(conn, &input).unwrap()
    }

    #[test]
    fn export_writes_format_and_timeout_null_for_default() {
        let conn = setup();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, 'G1')", [])
            .unwrap();
        insert_case_with(&conn, 1, "默认超时", "/api/a", 5); // 默认 5 → 写 null
        insert_case_with(&conn, 1, "显式30", "/api/b", 30); // 显式 30 → 写 30

        let path = tmp_json("m9_fmt.json");
        let sum = case_export_json_impl(
            &conn,
            &ExportScope {
                kind: "group".into(),
                group_id: Some(1),
                recursive: Some(false),
                case_ids: None,
            },
            &path,
        )
        .unwrap();
        assert_eq!(sum.count, 2);

        let txt = std::fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&txt).unwrap();
        assert_eq!(v["format"], EXPORT_FORMAT);
        assert_eq!(v["version"], 1);
        assert_eq!(v["group_path"], "G1");
        let cases = v["cases"].as_array().unwrap();
        // 按用例名定位
        let default_case = cases.iter().find(|c| c["case_name"] == "默认超时").unwrap();
        assert!(default_case["timeout_secs"].is_null(), "默认超时应当写为 null");
        let explicit = cases.iter().find(|c| c["case_name"] == "显式30").unwrap();
        assert_eq!(explicit["timeout_secs"], 30);
    }

    #[test]
    fn export_import_roundtrip_preserves_fields() {
        let conn = setup();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, '源分组')", [])
            .unwrap();
        conn.execute("INSERT INTO groups (id, name) VALUES (2, '目标分组')", [])
            .unwrap();
        insert_case_with(&conn, 1, "登录成功", "/api/login", 10);
        insert_case_with(&conn, 1, "超时30", "/api/x", 30);

        // 导出全部
        let path = tmp_json("m9_roundtrip.json");
        let sum = case_export_json_impl(
            &conn,
            &ExportScope {
                kind: "all".into(),
                group_id: None,
                recursive: None,
                case_ids: None,
            },
            &path,
        )
        .unwrap();
        assert_eq!(sum.count, 2);

        // 导入到目标分组
        let res = case_import_json_commit_impl(&conn, 2, &path).unwrap();
        assert_eq!(res.success_count, 2);
        assert_eq!(res.ignored_count, 0);
        assert_eq!(res.failed.len(), 0);

        // 校验字段无损
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM test_cases WHERE group_id = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 2);
        let names: Vec<String> = conn
            .prepare("SELECT case_name FROM test_cases WHERE group_id = 2 ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        // 导入默认超时(null) → 回退 5；显式 30 保持
        let timeouts: Vec<i64> = conn
            .prepare("SELECT timeout_secs FROM test_cases WHERE group_id = 2 ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(names.contains(&"登录成功".to_string()));
        assert!(timeouts.contains(&5));
        assert!(timeouts.contains(&30));
    }

    #[test]
    fn import_rejects_bad_format() {
        let conn = setup();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, 'G1')", [])
            .unwrap();
        let path = tmp_json("m9_badfmt.json");
        std::fs::write(
            &path,
            r#"{"format":"other-app","version":1,"cases":[]}"#,
        )
        .unwrap();
        let err = case_import_json_commit_impl(&conn, 1, &path).err().unwrap();
        assert!(err.msg.contains(EXPORT_FORMAT) || err.msg.contains("不是 LeverUnit"));
    }

    #[test]
    fn import_rejects_future_version() {
        let conn = setup();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, 'G1')", [])
            .unwrap();
        let path = tmp_json("m9_future.json");
        std::fs::write(
            &path,
            r#"{"format":"lever-unit-case-export","version":99,"cases":[]}"#,
        )
        .unwrap();
        let err = case_import_json_commit_impl(&conn, 1, &path).err().unwrap();
        assert!(err.msg.contains("版本过新") || err.msg.contains("升级"));
    }

    #[test]
    fn import_ignores_empty_url_and_autonames() {
        let conn = setup();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, 'G1')", [])
            .unwrap();
        // 4 条：命名有效 / 名称空(url有效) / url空 / 非法方法
        let path = tmp_json("m9_rules.json");
        std::fs::write(
            &path,
            r#"{
              "format":"lever-unit-case-export","version":1,
              "cases":[
                {"case_name":"命名用例","method":"POST","url":"/api/login"},
                {"case_name":"","method":"GET","url":"/api/empty_name"},
                {"case_name":"空地址","method":"GET","url":""},
                {"case_name":"坏方法","method":"TRACE","url":"/api/z"}
              ]
            }"#,
        )
        .unwrap();
        let res = case_import_json_commit_impl(&conn, 1, &path).unwrap();
        assert_eq!(res.success_count, 2);
        assert_eq!(res.ignored_count, 1);
        assert_eq!(res.failed.len(), 1);
        let names: Vec<String> = conn
            .prepare("SELECT case_name FROM test_cases WHERE group_id = 1 ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(names.contains(&"命名用例".to_string()));
        assert!(names.contains(&"G1_1".to_string())); // 自动命名：分组名_序号
    }

    #[test]
    fn import_tolerates_unknown_fields() {
        let conn = setup();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, 'G1')", [])
            .unwrap();
        let path = tmp_json("m9_unknown.json");
        std::fs::write(
            &path,
            r#"{
              "format":"lever-unit-case-export","version":1,
              "extra_top":"ignored",
              "cases":[
                {"case_name":"带未知字段","method":"GET","url":"/api/a","foo":"bar","nested":{"x":1}}
              ]
            }"#,
        )
        .unwrap();
        let res = case_import_json_commit_impl(&conn, 1, &path).unwrap();
        assert_eq!(res.success_count, 1);
        assert_eq!(res.failed.len(), 0);
    }

    #[test]
    fn export_selected_by_ids() {
        let conn = setup();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, 'G1')", [])
            .unwrap();
        let c1 = insert_case_with(&conn, 1, "A", "/api/a", 10);
        let c2 = insert_case_with(&conn, 1, "B", "/api/b", 10);
        let _ = c1;
        let path = tmp_json("m9_selected.json");
        let sum = case_export_json_impl(
            &conn,
            &ExportScope {
                kind: "selected".into(),
                group_id: None,
                recursive: None,
                case_ids: Some(vec![c2]),
            },
            &path,
        )
        .unwrap();
        assert_eq!(sum.count, 1);
        let txt = std::fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&txt).unwrap();
        assert_eq!(v["cases"].as_array().unwrap().len(), 1);
        assert_eq!(v["cases"][0]["case_name"], "B");
    }
}
