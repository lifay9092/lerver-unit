//! 用例导入模板管理（FR-TEMPLATE）与批量导入（FR-C4）
//!
//! 模板解决「外部文件字段 → 标准 HTTP 用例字段」的映射问题。
//! 批量导入策略：解析后逐行校验，非法行跳过并汇总原因（见 NFR-4）。

use std::collections::HashMap;

use calamine::{Data, Reader};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::cases::{insert_case, validate_case, CaseInput};
use crate::error::{AppError, PageResult};
use crate::AppState;

/// 标准 HTTP 用例字段（映射目标）。case_name / method / url 为必填。
pub const STD_FIELDS: &[(&str, &str, bool)] = &[
    ("case_name", "用例名称", true),
    ("method", "请求方法", true),
    ("url", "请求地址", true),
    ("content_type", "内容类型", false),
    ("headers", "请求头(JSON)", false),
    ("query_params", "查询参数(JSON)", false),
    ("request_body", "请求体", false),
    ("assert_expr", "校验表达式", false),
    ("timeout_secs", "超时(秒)", false),
    ("remark", "备注", false),
];

// ---------- 行结构 ----------

/// 模板行（mappings 以 JSON 文本返回，前端自行解析）
#[derive(Debug, Clone, Serialize)]
pub struct TemplateRow {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub mappings: String,
    pub is_builtin: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 模板创建/编辑入参
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateInput {
    pub id: Option<i64>,
    #[serde(rename = "type")]
    pub type_: String,
    pub name: String,
    pub mappings: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateQuery {
    pub keyword: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// 导入预览行
#[derive(Debug, Clone, Serialize)]
pub struct ImportPreviewRow {
    /// 数据行号（从 1 开始，不含表头）
    pub row: usize,
    pub case_input: CaseInput,
    /// 校验错误信息；None 表示合法
    pub error: Option<String>,
}

/// 导入结果汇总
#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub success_count: usize,
    pub failed: Vec<ImportFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportFailure {
    pub row: usize,
    pub case_name: String,
    pub reason: String,
}

/// 解析中间结构（不序列化）
struct ParsedRow {
    row: usize,
    input: CaseInput,
    error: Option<String>,
}

// ---------- 启动种子 ----------

/// 首次运行预置内置 xlsx 通用模板（FR-T6）
pub fn ensure_builtin_templates(conn: &Connection) -> Result<(), AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM templates WHERE is_builtin = 1",
        [],
        |r| r.get(0),
    )?;
    if n > 0 {
        return Ok(());
    }
    let mappings = serde_json::json!({
        "case_name": "用例名称",
        "method": "请求方法",
        "url": "请求地址",
        "content_type": "内容类型",
        "headers": "请求头",
        "query_params": "查询参数",
        "request_body": "请求体",
        "assert_expr": "校验表达式",
        "timeout_secs": "超时(秒)",
        "remark": "备注"
    });
    conn.execute(
        "INSERT INTO templates (name, type, mappings, is_builtin) VALUES (?1, 'xlsx', ?2, 1)",
        params!["通用模板(xlsx中文标题)", mappings.to_string()],
    )?;
    Ok(())
}

// ---------- 模板 CRUD ----------

fn parse_mappings(raw: &str) -> Result<HashMap<String, String>, AppError> {
    let v: Value =
        serde_json::from_str(raw).map_err(|e| AppError::new(format!("映射关系不是合法 JSON：{e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| AppError::new("映射关系必须是 JSON 对象"))?;
    let mut m = HashMap::new();
    for (k, val) in obj {
        let field = k.trim();
        if field.is_empty() {
            continue;
        }
        let src = match val {
            Value::String(s) => s.trim().to_string(),
            Value::Null => continue,
            _ => val.to_string(),
        };
        if !src.is_empty() {
            m.insert(field.to_string(), src);
        }
    }
    Ok(m)
}

fn validate_template(input: &TemplateInput) -> Result<(), AppError> {
    if input.name.trim().is_empty() {
        return Err(AppError::new("模板名称不能为空"));
    }
    if !["xlsx", "json"].contains(&input.type_.as_str()) {
        return Err(AppError::new("模板类型须为 xlsx 或 json"));
    }
    parse_mappings(&input.mappings)?;
    Ok(())
}

fn row_from(conn: &Connection, id: i64) -> Result<TemplateRow, AppError> {
    conn.query_row(
        "SELECT id, name, type, mappings, is_builtin, created_at, updated_at
         FROM templates WHERE id = ?1",
        params![id],
        |r| {
            Ok(TemplateRow {
                id: r.get(0)?,
                name: r.get(1)?,
                type_: r.get(2)?,
                mappings: r.get(3)?,
                is_builtin: r.get::<_, i64>(4)? != 0,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        },
    )
    .map_err(|_| AppError::new("模板不存在或已被删除"))
}

#[tauri::command]
pub fn template_list(
    query: TemplateQuery,
    state: State<'_, AppState>,
) -> Result<PageResult<TemplateRow>, AppError> {
    let conn = state.db.lock().unwrap();
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 500);
    let offset = (page - 1) * page_size;
    let kw = query.keyword.unwrap_or_default().trim().to_string();
    let like = format!("%{kw}%");

    let list = conn
        .prepare(
            "SELECT id, name, type, mappings, is_builtin, created_at, updated_at
             FROM templates WHERE (?1 = '' OR name LIKE ?1)
             ORDER BY is_builtin DESC, updated_at DESC, id DESC
             LIMIT ?2 OFFSET ?3",
        )?
        .query_map(params![like, page_size, offset], |r| {
            Ok(TemplateRow {
                id: r.get(0)?,
                name: r.get(1)?,
                type_: r.get(2)?,
                mappings: r.get(3)?,
                is_builtin: r.get::<_, i64>(4)? != 0,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM templates WHERE (?1 = '' OR name LIKE ?1)",
        params![like],
        |r| r.get(0),
    )?;
    Ok(PageResult {
        list,
        total,
        page,
        page_size,
    })
}

#[tauri::command]
pub fn template_create(input: TemplateInput, state: State<'_, AppState>) -> Result<TemplateRow, AppError> {
    validate_template(&input)?;
    let conn = state.db.lock().unwrap();
    conn.execute(
        "INSERT INTO templates (name, type, mappings) VALUES (?1, ?2, ?3)",
        params![input.name.trim(), input.type_, input.mappings],
    )?;
    let id = conn.last_insert_rowid();
    row_from(&conn, id)
}

#[tauri::command]
pub fn template_update(input: TemplateInput, state: State<'_, AppState>) -> Result<TemplateRow, AppError> {
    let id = input.id.ok_or_else(|| AppError::new("缺少模板 id"))?;
    validate_template(&input)?;
    let conn = state.db.lock().unwrap();
    let is_builtin: i64 = conn
        .query_row("SELECT is_builtin FROM templates WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|_| AppError::new("模板不存在或已被删除"))?;
    if is_builtin != 0 {
        return Err(AppError::new("内置模板不可直接修改，请先「复制」生成可编辑副本"));
    }
    let n = conn.execute(
        "UPDATE templates SET name = ?1, type = ?2, mappings = ?3, updated_at = datetime('now','localtime')
         WHERE id = ?4",
        params![input.name.trim(), input.type_, input.mappings, id],
    )?;
    if n == 0 {
        return Err(AppError::new("模板不存在或已被删除"));
    }
    row_from(&conn, id)
}

#[tauri::command]
pub fn template_delete(id: i64, state: State<'_, AppState>) -> Result<(), AppError> {
    let conn = state.db.lock().unwrap();
    let is_builtin: i64 = conn
        .query_row("SELECT is_builtin FROM templates WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|_| AppError::new("模板不存在或已被删除"))?;
    if is_builtin != 0 {
        return Err(AppError::new("内置模板不可删除，请复制后再管理"));
    }
    conn.execute("DELETE FROM templates WHERE id = ?1", params![id])?;
    Ok(())
}

/// 复制内置/任意模板为可编辑副本（FR-T6）
#[tauri::command]
pub fn template_copy(id: i64, state: State<'_, AppState>) -> Result<TemplateRow, AppError> {
    let conn = state.db.lock().unwrap();
    let (name, type_, mappings): (String, String, String) = conn
        .query_row(
            "SELECT name, type, mappings FROM templates WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| AppError::new("模板不存在或已被删除"))?;
    conn.execute(
        "INSERT INTO templates (name, type, mappings) VALUES (?1, ?2, ?3)",
        params![format!("{name}副本"), type_, mappings],
    )?;
    let new_id = conn.last_insert_rowid();
    row_from(&conn, new_id)
}

// ---------- 模板导出（FR-T7） ----------

/// 标准字段对应的示例填写值（导出模板时作为示例行，引导用户填写）
fn std_sample(std: &str) -> &'static str {
    match std {
        "case_name" => "示例用例-登录成功",
        "method" => "POST",
        "url" => "/api/login",
        "content_type" => "application/json",
        "headers" => "{\"Authorization\":\"Bearer xxxx\"}",
        "query_params" => "{\"id\":1}",
        "request_body" => "{\"username\":\"admin\",\"password\":\"123456\"}",
        "assert_expr" => "status == 200",
        "timeout_secs" => "30",
        "remark" => "示例备注",
        _ => "",
    }
}

/// 导出模板为可填写的空文件（表头 + 示例行），便于用户快速填写后导入（FR-T7）
#[tauri::command]
pub fn template_export(
    template_id: i64,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let conn = state.db.lock().unwrap();
    let template = row_from(&conn, template_id)?;
    let mappings = parse_mappings(&template.mappings)?;
    drop(conn);

    // 按标准字段顺序构造 (源列名, 示例值)
    let mut cols: Vec<(String, String)> = Vec::new();
    for (std, _label, _req) in STD_FIELDS {
        if let Some(src) = mappings.get(*std) {
            cols.push((src.clone(), std_sample(std).to_string()));
        }
    }
    if cols.is_empty() {
        return Err(AppError::new("该模板未配置任何字段映射，无法导出"));
    }

    match template.type_.as_str() {
        "xlsx" => export_xlsx(&file_path, &cols)?,
        "json" => export_json(&file_path, &cols)?,
        other => return Err(AppError::new(format!("不支持的模板类型：{other}"))),
    }
    Ok(())
}

/// 写出 xlsx：第 1 行为表头（源列名），第 2 行为示例值
fn export_xlsx(path: &str, cols: &[(String, String)]) -> Result<(), AppError> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    for (i, (header, sample)) in cols.iter().enumerate() {
        let c = i as u16;
        sheet
            .write_string(0, c, header)
            .map_err(|e| AppError::new(format!("写入表头失败：{e}")))?;
        sheet
            .write_string(1, c, sample)
            .map_err(|e| AppError::new(format!("写入示例失败：{e}")))?;
    }
    workbook
        .save(path)
        .map_err(|e| AppError::new(format!("保存文件失败：{e}")))?;
    Ok(())
}

/// 写出 json：源列名可能是点路径（如 data.url），重建嵌套结构，输出含一条示例对象的数组
fn export_json(path: &str, cols: &[(String, String)]) -> Result<(), AppError> {
    let mut root = serde_json::Map::new();
    for (src, sample) in cols {
        let parts: Vec<&str> = src.split('.').collect();
        insert_nested(&mut root, &parts, serde_json::Value::String(sample.clone()));
    }
    let arr = serde_json::json!([serde_json::Value::Object(root)]);
    std::fs::write(path, serde_json::to_string_pretty(&arr).unwrap())
        .map_err(|e| AppError::new(format!("保存文件失败：{e}")))?;
    Ok(())
}

/// 把点路径写入嵌套 JSON 对象
fn insert_nested(
    map: &mut serde_json::Map<String, serde_json::Value>,
    parts: &[&str],
    val: serde_json::Value,
) {
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        map.insert(parts[0].to_string(), val);
        return;
    }
    let entry = map
        .entry(parts[0].to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let serde_json::Value::Object(child) = entry {
        insert_nested(child, &parts[1..], val);
    }
}

// ---------- 文件读取辅助 ----------

/// calamine Data → 字符串
fn cell_to_string(d: &Data) -> String {
    match d {
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Bool(b) => b.to_string(),
        Data::Empty => String::new(),
        _ => String::new(),
    }
}

/// 打开 xlsx 第一个工作表
fn open_first_sheet(path: &str) -> Result<calamine::Range<Data>, AppError> {
    let mut wb = calamine::open_workbook_auto(path)
        .map_err(|e| AppError::new(format!("打开文件失败：{e}")))?;
    wb.worksheet_range_at(0)
        .ok_or_else(|| AppError::new("文件没有工作表"))?
        .map_err(|e| AppError::new(format!("读取工作表失败：{e}")))
}

/// 读取 xlsx 表头（第一张表第一行），供映射辅助选择
#[tauri::command]
pub fn read_xlsx_headers(file_path: String) -> Result<Vec<String>, AppError> {
    let range = open_first_sheet(&file_path)?;
    let headers: Vec<String> = (0..range.width())
        .map(|c| cell_to_string(range.get((0, c)).unwrap_or(&Data::Empty)))
        .filter(|s| !s.is_empty())
        .collect();
    Ok(headers)
}

/// 读取 json 字段路径（第一对象的全部叶子路径），供映射辅助选择
#[tauri::command]
pub fn read_json_fields(file_path: String) -> Result<Vec<String>, AppError> {
    let txt = std::fs::read_to_string(&file_path)
        .map_err(|e| AppError::new(format!("读取文件失败：{e}")))?;
    let v: Value =
        serde_json::from_str(&txt).map_err(|e| AppError::new(format!("JSON 解析失败：{e}")))?;
    let obj = match &v {
        Value::Array(a) => a.iter().filter_map(|x| x.as_object()).next(),
        Value::Object(o) => Some(o),
        _ => None,
    };
    match obj {
        Some(o) => {
            let mut m = HashMap::new();
            flatten_object(o, "", &mut m);
            let mut keys: Vec<String> = m.into_keys().collect();
            keys.sort();
            Ok(keys)
        }
        None => Ok(vec![]),
    }
}

/// JSON 对象拍平为 点路径 → 值字符串（叶子）
fn flatten_object(obj: &serde_json::Map<String, Value>, prefix: &str, out: &mut HashMap<String, String>) {
    for (k, val) in obj {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match val {
            Value::Object(child) => flatten_object(child, &key, out),
            Value::Null => {}
            other => {
                let s = match other {
                    Value::String(s) => s.clone(),
                    _ => other.to_string(),
                };
                out.insert(key, s);
            }
        }
    }
}

/// xlsx：按表头读取为 [{表头: 值}]
fn read_xlsx_rows(path: &str) -> Result<Vec<HashMap<String, String>>, AppError> {
    let range = open_first_sheet(path)?;
    let (height, width) = range.get_size();
    if height <= 1 {
        return Ok(vec![]); // 无数据行
    }
    let headers: Vec<String> = (0..width)
        .map(|c| cell_to_string(range.get((0, c)).unwrap_or(&Data::Empty)))
        .collect();
    let mut rows = Vec::new();
    for r in 1..height {
        let mut m = HashMap::new();
        for (c, h) in headers.iter().enumerate() {
            if h.is_empty() {
                continue;
            }
            let v = cell_to_string(range.get((r, c)).unwrap_or(&Data::Empty));
            m.insert(h.clone(), v);
        }
        // 跳过全空行
        if m.values().all(|v| v.trim().is_empty()) {
            continue;
        }
        rows.push(m);
    }
    Ok(rows)
}

/// json：读取为 [{路径: 值}]
fn read_json_rows(path: &str) -> Result<Vec<HashMap<String, String>>, AppError> {
    let txt = std::fs::read_to_string(path).map_err(|e| AppError::new(format!("读取文件失败：{e}")))?;
    let v: Value =
        serde_json::from_str(&txt).map_err(|e| AppError::new(format!("JSON 解析失败：{e}")))?;
    let objs: Vec<serde_json::Map<String, Value>> = match &v {
        Value::Array(a) => a.iter().filter_map(|x| x.as_object().cloned()).collect(),
        Value::Object(o) => vec![o.clone()],
        _ => return Err(AppError::new("JSON 根节点需为对象数组或单个对象")),
    };
    Ok(objs
        .iter()
        .map(|o| {
            let mut m = HashMap::new();
            flatten_object(o, "", &mut m);
            m
        })
        .collect())
}

/// 按映射将一行原始字段组装为 CaseInput
fn build_case(row_map: &HashMap<String, String>, mappings: &HashMap<String, String>) -> CaseInput {
    let get = |std: &str| -> Option<String> {
        mappings
            .get(std)
            .and_then(|src| row_map.get(src))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    CaseInput {
        id: None,
        group_id: None,
        case_name: get("case_name").unwrap_or_default(),
        method: get("method").unwrap_or_default(),
        content_type: get("content_type").unwrap_or_else(|| "application/json".to_string()),
        url: get("url").unwrap_or_default(),
        headers: get("headers"),
        query_params: get("query_params"),
        request_body: get("request_body"),
        assert_expr: get("assert_expr"),
        timeout_secs: get("timeout_secs").and_then(|s| s.parse::<i64>().ok()),
        remark: get("remark"),
    }
}

/// 解析文件并逐行校验（预览与提交共用）
fn parse_file(file_path: &str, template: &TemplateRow) -> Result<Vec<ParsedRow>, AppError> {
    let mappings = parse_mappings(&template.mappings)?;
    let rows: Vec<HashMap<String, String>> = match template.type_.as_str() {
        "xlsx" => read_xlsx_rows(file_path)?,
        "json" => read_json_rows(file_path)?,
        other => return Err(AppError::new(format!("不支持的模板类型：{other}"))),
    };
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(i, row_map)| {
            let input = build_case(&row_map, &mappings);
            let error = validate_case(&input).err().map(|e| e.msg);
            ParsedRow {
                row: i + 1,
                input,
                error,
            }
        })
        .collect())
}

/// 导入预览（不落库，FR-C4）
#[tauri::command]
pub fn import_preview(
    file_path: String,
    template_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<ImportPreviewRow>, AppError> {
    let conn = state.db.lock().unwrap();
    let template = row_from(&conn, template_id)?;
    drop(conn);
    let parsed = parse_file(&file_path, &template)?;
    Ok(parsed
        .into_iter()
        .map(|p| ImportPreviewRow {
            row: p.row,
            case_input: p.input,
            error: p.error,
        })
        .collect())
}

/// 批量导入提交：跳过非法行并汇总（NFR-4）
#[tauri::command]
pub fn import_commit(
    group_id: i64,
    file_path: String,
    template_id: i64,
    state: State<'_, AppState>,
) -> Result<ImportResult, AppError> {
    let conn = state.db.lock().unwrap();
    let _: i64 = conn
        .query_row("SELECT id FROM groups WHERE id = ?1", params![group_id], |r| r.get(0))
        .map_err(|_| AppError::new("目标分组不存在"))?;
    let template = row_from(&conn, template_id)?;
    let parsed = parse_file(&file_path, &template)?;

    let mut success_count = 0usize;
    let mut failed: Vec<ImportFailure> = Vec::new();
    for p in &parsed {
        let case_name = if p.input.case_name.trim().is_empty() {
            format!("(第{}条)", p.row)
        } else {
            p.input.case_name.clone()
        };
        if let Some(err) = &p.error {
            failed.push(ImportFailure {
                row: p.row,
                case_name,
                reason: err.clone(),
            });
            continue;
        }
        let mut input = p.input.clone();
        input.group_id = Some(group_id);
        match validate_case(&input).and_then(|_| insert_case(&conn, &input)) {
            Ok(_) => success_count += 1,
            Err(e) => failed.push(ImportFailure {
                row: p.row,
                case_name,
                reason: e.msg,
            }),
        }
    }
    Ok(ImportResult {
        success_count,
        failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        ensure_builtin_templates(&conn).unwrap();
        conn
    }

    fn write_temp(name: &str, content: &str) -> String {
        let p = std::env::temp_dir().join(name);
        std::fs::write(&p, content).unwrap();
        p.to_string_lossy().to_string()
    }

    #[test]
    fn builtin_template_seeded_once() {
        let conn = setup();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM templates WHERE is_builtin=1", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        // 再次调用不应重复
        ensure_builtin_templates(&conn).unwrap();
        let n2: i64 = conn.query_row("SELECT COUNT(*) FROM templates WHERE is_builtin=1", [], |r| r.get(0)).unwrap();
        assert_eq!(n2, 1);
    }

    #[test]
    fn parse_json_with_errors() {
        let conn = setup();
        // 用 json 类型模板（复用内置模板的映射），避免 calamine 按 xlsx 打开 json 内容
        let builtin_mappings: String = conn
            .query_row("SELECT mappings FROM templates WHERE is_builtin=1", [], |r| r.get::<_, String>(0))
            .unwrap();
        conn.execute(
            "INSERT INTO templates (name, type, mappings) VALUES ('jt','json',?1)",
            params![builtin_mappings],
        )
        .unwrap();
        let tpl = conn.query_row("SELECT id,type,mappings FROM templates WHERE name='jt'", [], |r| {
            Ok(TemplateRow { id: r.get(0)?, name: "".into(), type_: r.get(1)?, mappings: r.get(2)?, is_builtin: false, created_at: "".into(), updated_at: "".into() })
        }).unwrap();

        // json 用例：3 条，1 条非法 method
        let json = r#"[
            {"用例名称":"登录成功","请求方法":"POST","请求地址":"/api/login"},
            {"用例名称":"获取用户","请求方法":"TRACE","请求地址":"/api/user"},
            {"用例名称":"删除订单","请求方法":"DELETE","请求地址":"/api/order"}
        ]"#;
        let path = write_temp("m4_test.json", json);
        let rows = parse_file(&path, &tpl).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].error.is_none());
        assert!(rows[1].error.is_some()); // TRACE 非法
        assert!(rows[2].error.is_none());
    }

    #[test]
    fn builtin_mapping_headers_match() {
        let conn = setup();
        let mappings: HashMap<String, String> = conn
            .query_row("SELECT mappings FROM templates WHERE is_builtin=1", [], |r| r.get::<_, String>(0))
            .map(|s| parse_mappings(&s).unwrap())
            .unwrap();
        assert_eq!(mappings.get("case_name").unwrap(), "用例名称");
        assert_eq!(mappings.get("url").unwrap(), "请求地址");
    }

    #[test]
    fn commit_skips_invalid_and_reports() {
        let conn = setup();
        // 建分组
        conn.execute("INSERT INTO groups (id, name) VALUES (1, 'G')", []).unwrap();
        // 用 json 类型模板（复用内置映射），避免 calamine 按 xlsx 打开 json 内容
        let builtin_mappings: String = conn
            .query_row("SELECT mappings FROM templates WHERE is_builtin=1", [], |r| r.get::<_, String>(0))
            .unwrap();
        conn.execute(
            "INSERT INTO templates (name, type, mappings) VALUES ('jt','json',?1)",
            params![builtin_mappings],
        )
        .unwrap();
        let tpl = conn.query_row("SELECT id,type,mappings FROM templates WHERE name='jt'", [], |r| {
            Ok(TemplateRow { id: r.get(0)?, name: "".into(), type_: r.get(1)?, mappings: r.get(2)?, is_builtin: false, created_at: "".into(), updated_at: "".into() })
        }).unwrap();
        let json = r#"[
            {"用例名称":"登录成功","请求方法":"POST","请求地址":"/api/login"},
            {"用例名称":"坏方法","请求方法":"TRACE","请求地址":"/api/user"},
            {"用例名称":"删除订单","请求方法":"DELETE","请求地址":"/api/order"}
        ]"#;
        let path = write_temp("m4_commit.json", json);
        let res = {
            // 复用 import_commit 逻辑：手动走 parse_file + 插入
            let parsed = parse_file(&path, &tpl).unwrap();
            let mut success = 0usize;
            let mut failed = Vec::new();
            for p in &parsed {
                if let Some(err) = &p.error {
                    failed.push(ImportFailure { row: p.row, case_name: p.input.case_name.clone(), reason: err.clone() });
                    continue;
                }
                let mut input = p.input.clone();
                input.group_id = Some(1);
                match validate_case(&input).and_then(|_| insert_case(&conn, &input)) {
                    Ok(_) => success += 1,
                    Err(e) => failed.push(ImportFailure { row: p.row, case_name: p.input.case_name.clone(), reason: e.msg }),
                }
            }
            ImportResult { success_count: success, failed }
        };
        assert_eq!(res.success_count, 2);
        assert_eq!(res.failed.len(), 1);
        // 库内应有 2 条
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM test_cases", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 2);
    }

    #[test]
    fn json_nested_path_resolves() {
        let conn = setup();
        let mappings = serde_json::json!({"case_name":"data.name","method":"data.method","url":"path"}).to_string();
        conn.execute("INSERT INTO templates (name, type, mappings) VALUES ('t','json',?1)", params![mappings]).unwrap();
        let tpl = conn.query_row("SELECT id,type,mappings FROM templates WHERE name='t'", [], |r| {
            Ok(TemplateRow { id: r.get(0)?, name: "".into(), type_: r.get(1)?, mappings: r.get(2)?, is_builtin: false, created_at: "".into(), updated_at: "".into() })
        }).unwrap();
        let json = r#"[{"data":{"name":"嵌套用例","method":"GET"},"path":"/api/x"}]"#;
        let path = write_temp("m4_nested.json", json);
        let rows = parse_file(&path, &tpl).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].error.is_none());
        assert_eq!(rows[0].input.case_name, "嵌套用例");
        assert_eq!(rows[0].input.url, "/api/x");
    }

    #[test]
    fn template_rejects_bad_type() {
        let _conn = setup();
        let input = TemplateInput { id: None, type_: "csv".into(), name: "x".into(), mappings: "{}".into() };
        assert!(validate_template(&input).is_err());
    }

    #[test]
    fn export_xlsx_headers_roundtrip() {
        let cols = vec![
            ("用例名称".to_string(), "示例用例".to_string()),
            ("请求方法".to_string(), "POST".to_string()),
            ("请求地址".to_string(), "/api/x".to_string()),
        ];
        let dir = std::env::temp_dir();
        let xlsx_path = dir.join("m4_export_x.xlsx").to_string_lossy().to_string();
        export_xlsx(&xlsx_path, &cols).unwrap();
        let range = open_first_sheet(&xlsx_path).unwrap();
        let headers: Vec<String> = (0..range.width())
            .map(|c| cell_to_string(range.get((0, c)).unwrap_or(&Data::Empty)))
            .collect();
        assert_eq!(
            headers,
            vec!["用例名称".to_string(), "请求方法".to_string(), "请求地址".to_string()]
        );
        // 示例行存在
        let sample = cell_to_string(range.get((1, 1)).unwrap_or(&Data::Empty));
        assert_eq!(sample, "POST");
    }

    #[test]
    fn export_json_rebuilds_nested() {
        let cols = vec![
            ("data.url".to_string(), "/api/y".to_string()),
            ("method".to_string(), "GET".to_string()),
        ];
        let dir = std::env::temp_dir();
        let json_path = dir.join("m4_export_j.json").to_string_lossy().to_string();
        export_json(&json_path, &cols).unwrap();
        let txt = std::fs::read_to_string(&json_path).unwrap();
        let v: Value = serde_json::from_str(&txt).unwrap();
        let arr = v.as_array().unwrap();
        let obj = arr[0].as_object().unwrap();
        assert_eq!(obj.get("method").unwrap().as_str().unwrap(), "GET");
        assert_eq!(
            obj.get("data").unwrap().get("url").unwrap().as_str().unwrap(),
            "/api/y"
        );
    }
}
