//! 测试用例管理（FR-C1 ~ C3、C9、C11）

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, PageResult};
use crate::AppState;

const METHODS: [&str; 5] = ["GET", "POST", "PUT", "DELETE", "PATCH"];
const CONTENT_TYPES: [&str; 4] = [
    "none",
    "application/json",
    "application/x-www-form-urlencoded",
    "multipart/form-data",
];

/// 用例行（含所属分组全路径）
#[derive(Debug, Clone, Serialize)]
pub struct CaseRow {
    pub id: i64,
    pub group_id: Option<i64>,
    pub group_path: Option<String>,
    pub case_name: String,
    pub method: String,
    pub content_type: String,
    pub url: String,
    pub headers: String,
    pub query_params: String,
    pub request_body: String,
    pub assert_expr: String,
    pub timeout_secs: i64,
    pub remark: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 用例创建/编辑入参（字段名须与前端 TS 一致：snake_case）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseInput {
    pub id: Option<i64>,
    pub group_id: Option<i64>,
    pub case_name: String,
    pub method: String,
    pub content_type: String,
    pub url: String,
    pub headers: Option<String>,
    pub query_params: Option<String>,
    pub request_body: Option<String>,
    pub assert_expr: Option<String>,
    pub timeout_secs: Option<i64>,
    pub remark: Option<String>,
}

/// 分页查询参数
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseQuery {
    pub group_id: Option<i64>,
    /// 是否包含子分组用例
    pub recursive: Option<bool>,
    pub keyword: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

// ---------- 查询 ----------

/// 分页查询用例（FR-C1、FR-C9）
#[tauri::command]
pub fn case_page(query: CaseQuery, state: State<'_, AppState>) -> Result<PageResult<CaseRow>, AppError> {
    let conn = state.db.lock().unwrap();
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 500);
    let offset = (page - 1) * page_size;
    let kw = query.keyword.unwrap_or_default().trim().to_string();

    let path_map = group_path_map(&conn)?;

    let (rows, total): (Vec<CaseRow>, i64) = match (query.group_id, query.recursive.unwrap_or(false)) {
        // 全部用例
        (None, _) => {
            let like = format!("%{kw}%");
            let list = query_rows(
                &conn,
                "SELECT c.id, c.group_id, c.case_name, c.method, c.content_type, c.url, c.headers,
                        c.query_params, c.request_body, c.assert_expr, c.timeout_secs, c.remark,
                        c.created_at, c.updated_at
                 FROM test_cases c
                 WHERE (?1 = '' OR c.case_name LIKE ?1 OR c.url LIKE ?1)
                 ORDER BY c.updated_at DESC, c.id DESC
                 LIMIT ?2 OFFSET ?3",
                params![like, page_size, offset],
                &path_map,
            )?;
            let total: i64 = conn.query_row(
                "SELECT COUNT(*) FROM test_cases c WHERE (?1 = '' OR c.case_name LIKE ?1 OR c.url LIKE ?1)",
                params![like],
                |r| r.get(0),
            )?;
            (list, total)
        }
        // 指定分组（不含子分组）
        (Some(gid), false) => {
            let like = format!("%{kw}%");
            let list = query_rows(
                &conn,
                "SELECT c.id, c.group_id, c.case_name, c.method, c.content_type, c.url, c.headers,
                        c.query_params, c.request_body, c.assert_expr, c.timeout_secs, c.remark,
                        c.created_at, c.updated_at
                 FROM test_cases c
                 WHERE c.group_id = ?2 AND (?1 = '' OR c.case_name LIKE ?1 OR c.url LIKE ?1)
                 ORDER BY c.updated_at DESC, c.id DESC
                 LIMIT ?3 OFFSET ?4",
                params![like, gid, page_size, offset],
                &path_map,
            )?;
            let total: i64 = conn.query_row(
                "SELECT COUNT(*) FROM test_cases c
                 WHERE c.group_id = ?2 AND (?1 = '' OR c.case_name LIKE ?1 OR c.url LIKE ?1)",
                params![like, gid],
                |r| r.get(0),
            )?;
            (list, total)
        }
        // 指定分组 + 递归子分组
        (Some(gid), true) => {
            let like = format!("%{kw}%");
            let list = query_rows(
                &conn,
                "WITH RECURSIVE sub(id) AS (
                    SELECT ?2
                    UNION ALL
                    SELECT g.id FROM groups g JOIN sub ON g.parent_id = sub.id
                 )
                 SELECT c.id, c.group_id, c.case_name, c.method, c.content_type, c.url, c.headers,
                        c.query_params, c.request_body, c.assert_expr, c.timeout_secs, c.remark,
                        c.created_at, c.updated_at
                 FROM test_cases c
                 WHERE c.group_id IN (SELECT id FROM sub)
                   AND (?1 = '' OR c.case_name LIKE ?1 OR c.url LIKE ?1)
                 ORDER BY c.updated_at DESC, c.id DESC
                 LIMIT ?3 OFFSET ?4",
                params![like, gid, page_size, offset],
                &path_map,
            )?;
            let total: i64 = conn.query_row(
                "WITH RECURSIVE sub(id) AS (
                    SELECT ?2
                    UNION ALL
                    SELECT g.id FROM groups g JOIN sub ON g.parent_id = sub.id
                 )
                 SELECT COUNT(*) FROM test_cases c
                 WHERE c.group_id IN (SELECT id FROM sub)
                   AND (?1 = '' OR c.case_name LIKE ?1 OR c.url LIKE ?1)",
                params![like, gid],
                |r| r.get(0),
            )?;
            (list, total)
        }
    };

    Ok(PageResult { list: rows, total, page, page_size })
}

fn query_rows(
    conn: &Connection,
    sql: &str,
    p: impl rusqlite::Params,
    path_map: &std::collections::HashMap<i64, String>,
) -> Result<Vec<CaseRow>, AppError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(p, |r| {
            let group_id: Option<i64> = r.get(1)?;
            Ok(CaseRow {
                id: r.get(0)?,
                group_id,
                group_path: group_id.and_then(|g| path_map.get(&g).cloned()),
                case_name: r.get(2)?,
                method: r.get(3)?,
                content_type: r.get(4)?,
                url: r.get(5)?,
                headers: r.get(6).unwrap_or_default(),
                query_params: r.get(7).unwrap_or_default(),
                request_body: r.get(8).unwrap_or_default(),
                assert_expr: r.get(9).unwrap_or_default(),
                timeout_secs: r.get(10)?,
                remark: r.get(11).unwrap_or_default(),
                created_at: r.get(12)?,
                updated_at: r.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 分组 id → 全路径
fn group_path_map(conn: &Connection) -> Result<std::collections::HashMap<i64, String>, AppError> {
    Ok(crate::groups::load_rows(conn)?
        .into_iter()
        .map(|g| (g.id, g.path))
        .collect())
}

// ---------- 校验 ----------

/// 用例字段校验（供 case_create / 导入复用）
/// `allow_empty_name`：导入场景允许名称为空（由导入流程自动命名，FR-C4），其余场景必须为非空
pub(crate) fn validate_case(input: &CaseInput, allow_empty_name: bool) -> Result<(), AppError> {
    let name = input.case_name.trim();
    if !allow_empty_name && name.is_empty() {
        return Err(AppError::new("用例名称不能为空"));
    }
    if name.chars().count() > 100 {
        return Err(AppError::new("用例名称不超过 100 字符"));
    }
    if !METHODS.contains(&input.method.as_str()) {
        return Err(AppError::new(format!(
            "请求方法「{}」不合法，支持：{}",
            input.method,
            METHODS.join(" / ")
        )));
    }
    if !CONTENT_TYPES.contains(&input.content_type.as_str()) {
        return Err(AppError::new(format!(
            "请求内容类型「{}」不合法，支持：{}",
            input.content_type,
            CONTENT_TYPES.join(" / ")
        )));
    }
    validate_url(&input.url)?;

    // headers / query_params 必须是合法 JSON 对象
    for (label, raw) in [("请求头", &input.headers), ("查询参数", &input.query_params)] {
        if let Some(raw) = raw {
            if raw.trim().is_empty() {
                continue;
            }
            let v: serde_json::Value =
                serde_json::from_str(raw).map_err(|e| AppError::new(format!("{label}不是合法 JSON：{e}")))?;
            if !v.is_object() {
                return Err(AppError::new(format!("{label}必须是 JSON 对象格式，如 {{\"k\":\"v\"}}")));
            }
        }
    }

    let timeout = input.timeout_secs.unwrap_or(10);
    if !(1..=600).contains(&timeout) {
        return Err(AppError::new("超时时间需在 1 ~ 600 秒之间（不填默认 10 秒）"));
    }

    // 校验表达式：保存阶段即报错（FR-C5）
    if let Some(expr) = &input.assert_expr {
        crate::assert_expr::parse(expr).map_err(AppError::new)?;
    }
    Ok(())
}

/// 内部：插入一条用例（已通过校验），返回新 id
pub(crate) fn insert_case(conn: &Connection, input: &CaseInput) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO test_cases (group_id, case_name, method, content_type, url, headers, query_params,
            request_body, assert_expr, timeout_secs, remark)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            input.group_id,
            input.case_name.trim(),
            input.method,
            input.content_type,
            input.url.trim(),
            input.headers.clone().unwrap_or_default(),
            input.query_params.clone().unwrap_or_default(),
            input.request_body.clone().unwrap_or_default(),
            input.assert_expr.clone().unwrap_or_default(),
            input.timeout_secs.unwrap_or(10),
            input.remark.clone().unwrap_or_default(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// URL 合法形态：绝对地址 / 相对路径 / {{baseUrl}} 占位符（FR-E2）
fn validate_url(url: &str) -> Result<(), AppError> {
    let u = url.trim();
    if u.is_empty() {
        return Err(AppError::new("请求地址不能为空"));
    }
    let ok = u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with('/')
        || u.starts_with("{{baseUrl}}");
    if !ok {
        return Err(AppError::new(
            "请求地址需为以下之一：完整地址 https://api.xxx.com/login、相对路径 /api/login、占位符 {{baseUrl}}/api/login",
        ));
    }
    Ok(())
}

// ---------- 写操作 ----------

/// 新增用例（FR-C3）
#[tauri::command]
pub fn case_create(input: CaseInput, state: State<'_, AppState>) -> Result<CaseRow, AppError> {
    validate_case(&input, false)?;
    let conn = state.db.lock().unwrap();
    if let Some(gid) = input.group_id {
        let _: i64 = conn
            .query_row("SELECT id FROM groups WHERE id = ?1", params![gid], |r| r.get(0))
            .map_err(|_| AppError::new("指定的分组不存在"))?;
    }
    let id = insert_case(&conn, &input)?;
    drop(conn);
    get_case(id, state)
}

/// 编辑用例（FR-C3）
#[tauri::command]
pub fn case_update(input: CaseInput, state: State<'_, AppState>) -> Result<CaseRow, AppError> {
    let id = input.id.ok_or_else(|| AppError::new("缺少用例 id"))?;
    validate_case(&input, false)?;
    let conn = state.db.lock().unwrap();
    if let Some(gid) = input.group_id {
        let _: i64 = conn
            .query_row("SELECT id FROM groups WHERE id = ?1", params![gid], |r| r.get(0))
            .map_err(|_| AppError::new("指定的分组不存在"))?;
    }
    let n = conn.execute(
        "UPDATE test_cases SET group_id = ?1, case_name = ?2, method = ?3, content_type = ?4, url = ?5,
            headers = ?6, query_params = ?7, request_body = ?8, assert_expr = ?9, timeout_secs = ?10,
            remark = ?11, updated_at = datetime('now','localtime')
         WHERE id = ?12",
        params![
            input.group_id,
            input.case_name.trim(),
            input.method,
            input.content_type,
            input.url.trim(),
            input.headers.clone().unwrap_or_default(),
            input.query_params.clone().unwrap_or_default(),
            input.request_body.clone().unwrap_or_default(),
            input.assert_expr.clone().unwrap_or_default(),
            input.timeout_secs.unwrap_or(10),
            input.remark.clone().unwrap_or_default(),
            id,
        ],
    )?;
    if n == 0 {
        return Err(AppError::new("用例不存在或已被删除"));
    }
    drop(conn);
    get_case(id, state)
}

/// 删除用例
#[tauri::command]
pub fn case_delete(id: i64, state: State<'_, AppState>) -> Result<(), AppError> {
    let conn = state.db.lock().unwrap();
    let n = conn.execute("DELETE FROM test_cases WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::new("用例不存在或已被删除"));
    }
    Ok(())
}

/// 校验表达式语法检查（供前端保存前预校验，FR-C5）
#[tauri::command]
pub fn assert_expr_validate(expr: String) -> Result<usize, AppError> {
    let rules = crate::assert_expr::parse(&expr).map_err(AppError::new)?;
    Ok(rules.len())
}

/// 内部：从连接读取单条用例（无锁，供 executor 复用）
pub(crate) fn fetch_case(conn: &Connection, id: i64) -> Result<Option<CaseRow>, AppError> {
    let path_map = group_path_map(conn)?;
    let mut rows = query_rows(
        conn,
        "SELECT c.id, c.group_id, c.case_name, c.method, c.content_type, c.url, c.headers,
                c.query_params, c.request_body, c.assert_expr, c.timeout_secs, c.remark,
                c.created_at, c.updated_at
         FROM test_cases c WHERE c.id = ?1",
        params![id],
        &path_map,
    )?;
    Ok(rows.pop())
}

/// 内部：读取单条用例（含锁）
fn get_case(id: i64, state: State<'_, AppState>) -> Result<CaseRow, AppError> {
    let conn = state.db.lock().unwrap();
    fetch_case(&conn, id)?.ok_or_else(|| AppError::new("用例读取失败"))
}

/// 收集某分组（可选递归）下的全部用例 id（执行前批量取 id，FR-C6/C7）
#[tauri::command]
pub fn case_ids_by_group(
    group_id: Option<i64>,
    recursive: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<i64>, AppError> {
    let conn = state.db.lock().unwrap();
    let ids: Vec<i64> = match (group_id, recursive.unwrap_or(false)) {
        (None, _) => conn
            .prepare("SELECT id FROM test_cases ORDER BY id")?
            .query_map((), |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?,
        (Some(gid), false) => conn
            .prepare("SELECT id FROM test_cases WHERE group_id = ?1 ORDER BY id")?
            .query_map(params![gid], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?,
        (Some(gid), true) => conn
            .prepare(
                "WITH RECURSIVE sub(id) AS (
                    SELECT ?1
                    UNION ALL
                    SELECT g.id FROM groups g JOIN sub ON g.parent_id = sub.id
                 )
                 SELECT id FROM test_cases WHERE group_id IN (SELECT id FROM sub) ORDER BY id",
            )?
            .query_map(params![gid], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_input() -> CaseInput {
        CaseInput {
            id: None,
            group_id: None,
            case_name: "登录成功".into(),
            method: "POST".into(),
            content_type: "application/json".into(),
            url: "/api/login".into(),
            headers: Some("{\"Authorization\":\"Bearer x\"}".into()),
            query_params: Some("{}".into()),
            request_body: Some("{\"u\":\"a\"}".into()),
            assert_expr: Some("status == 200".into()),
            timeout_secs: Some(30),
            remark: Some("".into()),
        }
    }

    #[test]
    fn validate_accepts_good_input() {
        validate_case(&good_input(), false).unwrap();
        let mut i = good_input();
        i.url = "https://api.example.com/login".into();
        validate_case(&i, false).unwrap();
        i.url = "{{baseUrl}}/api/login".into();
        validate_case(&i, false).unwrap();
    }

    #[test]
    fn validate_rejects_bad_input() {
        let mut i = good_input();
        i.method = "TRACE".into();
        assert!(validate_case(&i, false).err().unwrap().msg.contains("请求方法"));

        let mut i = good_input();
        i.content_type = "text/plain".into();
        assert!(validate_case(&i, false).err().unwrap().msg.contains("内容类型"));

        let mut i = good_input();
        i.url = "api/login".into();
        assert!(validate_case(&i, false).err().unwrap().msg.contains("请求地址"));

        let mut i = good_input();
        i.headers = Some("{bad json}".into());
        assert!(validate_case(&i, false).err().unwrap().msg.contains("请求头"));

        let mut i = good_input();
        i.query_params = Some("[1,2]".into());
        assert!(validate_case(&i, false).err().unwrap().msg.contains("必须是 JSON 对象"));

        let mut i = good_input();
        i.timeout_secs = Some(0);
        assert!(validate_case(&i, false).err().unwrap().msg.contains("超时时间"));

        let mut i = good_input();
        i.assert_expr = Some("status === 200".into());
        assert!(validate_case(&i, false).err().unwrap().msg.contains("比较符"));

        let mut i = good_input();
        i.case_name = "  ".into();
        assert!(validate_case(&i, false).err().unwrap().msg.contains("用例名称"));
    }

    #[test]
    fn query_by_group_recursive() {
        let conn = Connection::open_in_memory().unwrap();
        crate::migrations::run(&conn).unwrap();
        conn.execute("INSERT INTO groups (id, name) VALUES (1, '根')", []).unwrap();
        conn.execute("INSERT INTO groups (id, name, parent_id) VALUES (2, '子', 1)", []).unwrap();
        conn.execute("INSERT INTO groups (id, name, parent_id) VALUES (3, '孙', 2)", []).unwrap();
        conn.execute(
            "INSERT INTO test_cases (group_id, case_name, method, url) VALUES (1,'A','GET','/a')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO test_cases (group_id, case_name, method, url) VALUES (3,'C','GET','/c')",
            [],
        )
        .unwrap();

        let path_map = group_path_map(&conn).unwrap();
        assert_eq!(path_map.get(&3).unwrap(), "根 / 子 / 孙");

        // 递归：根分组下应包含孙分��用例
        let list = query_rows(
            &conn,
            "WITH RECURSIVE sub(id) AS (
                SELECT ?2 UNION ALL SELECT g.id FROM groups g JOIN sub ON g.parent_id = sub.id
             )
             SELECT c.id, c.group_id, c.case_name, c.method, c.content_type, c.url, c.headers,
                    c.query_params, c.request_body, c.assert_expr, c.timeout_secs, c.remark,
                    c.created_at, c.updated_at
             FROM test_cases c WHERE c.group_id IN (SELECT id FROM sub)
             ORDER BY c.id",
            params!["", 1i64],
            &path_map,
        )
        .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].group_path.as_deref(), Some("根 / 子 / 孙"));

        // 非递归：仅直属
        let direct = query_rows(
            &conn,
            "SELECT c.id, c.group_id, c.case_name, c.method, c.content_type, c.url, c.headers,
                    c.query_params, c.request_body, c.assert_expr, c.timeout_secs, c.remark,
                    c.created_at, c.updated_at
             FROM test_cases c WHERE c.group_id = ?1 ORDER BY c.id",
            params![1i64],
            &path_map,
        )
        .unwrap();
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].case_name, "A");
    }
}
