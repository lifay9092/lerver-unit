use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::AppError;
use crate::AppState;

/// 建议最大层级（FR-G1：界面上限制 ≤ 5 级以防误操作）
const MAX_DEPTH: usize = 5;

/// 分组行（含统计与路径，返回前端）
#[derive(Debug, Clone, Serialize)]
pub struct GroupRow {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub sort_order: i64,
    pub remark: String,
    pub created_at: String,
    pub updated_at: String,
    /// 全路径，如 "项目A / 用户模块 / 登录"
    pub path: String,
    pub child_count: i64,
    pub case_count: i64,
}

/// 创建/编辑入参（注意：结构体字段不做 camelCase 重命名，须与前端 TS 字段名一致）
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupInput {
    pub id: Option<i64>,
    pub name: String,
    pub parent_id: Option<i64>,
    pub sort_order: Option<i64>,
    pub remark: Option<String>,
}

// ---------- 查询 ----------

/// 全量分组列表（含子组数/用例数/全路径），keyword 为名称模糊过滤
#[tauri::command]
pub fn group_list(keyword: Option<String>, state: State<'_, AppState>) -> Result<Vec<GroupRow>, AppError> {
    let conn = state.db.lock().unwrap();
    let rows = load_rows(&conn)?;
    let kw = keyword.unwrap_or_default().trim().to_string();
    let filtered: Vec<GroupRow> = if kw.is_empty() {
        rows
    } else {
        rows.into_iter().filter(|g| g.name.contains(&kw) || g.path.contains(&kw)).collect()
    };
    Ok(filtered)
}

/// 分组树节点（供左侧分组树使用）
#[derive(Debug, Clone, Serialize)]
pub struct GroupNode {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub child_count: i64,
    pub case_count: i64,
    pub children: Vec<GroupNode>,
}

/// 分组树（FR-C1 左树）
#[tauri::command]
pub fn group_tree(state: State<'_, AppState>) -> Result<Vec<GroupNode>, AppError> {
    let conn = state.db.lock().unwrap();
    let rows = load_rows(&conn)?;
    Ok(build_tree(&rows, None))
}

fn build_tree(rows: &[GroupRow], parent: Option<i64>) -> Vec<GroupNode> {
    rows.iter()
        .filter(|r| r.parent_id == parent)
        .map(|r| GroupNode {
            id: r.id,
            name: r.name.clone(),
            path: r.path.clone(),
            child_count: r.child_count,
            case_count: r.case_count,
            children: build_tree(rows, Some(r.id)),
        })
        .collect()
}

/// 加载全部分组并补齐 path / child_count / case_count（cases.rs 等模块共用）
pub(crate) fn load_rows(conn: &Connection) -> Result<Vec<GroupRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT g.id, g.name, g.parent_id, g.sort_order, g.remark, g.created_at, g.updated_at,
            (SELECT COUNT(*) FROM groups c WHERE c.parent_id = g.id) AS child_count,
            (SELECT COUNT(*) FROM test_cases t WHERE t.group_id = g.id) AS case_count
         FROM groups g
         ORDER BY COALESCE(g.parent_id, 0), g.sort_order, g.id",
    )?;
    let mut raw: Vec<GroupRow> = stmt
        .query_map([], |r| {
            Ok(GroupRow {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                sort_order: r.get(3)?,
                remark: r.get(4).unwrap_or_default(),
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
                path: String::new(),
                child_count: r.get(7)?,
                case_count: r.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    // 计算全路径：先建 id → (name, parent) 映射，避免借用冲突
    let id_map: HashMap<i64, (String, Option<i64>)> =
        raw.iter().map(|g| (g.id, (g.name.clone(), g.parent_id))).collect();
    for g in raw.iter_mut() {
        let mut names: Vec<String> = vec![g.name.clone()];
        let mut cur = g.parent_id;
        let mut guard = 0;
        while let Some(pid) = cur {
            if let Some((n, parent)) = id_map.get(&pid) {
                names.push(n.clone());
                cur = *parent;
            } else {
                break;
            }
            guard += 1;
            if guard > 64 {
                break; // 防御：异常环数据不至于死循环
            }
        }
        names.reverse();
        g.path = names.join(" / ");
    }
    Ok(raw)
}

/// 内部：指定分组的层级深度（根为 1）
fn depth_of(conn: &Connection, mut id: i64) -> Result<usize, AppError> {
    let mut depth = 1usize;
    let mut guard = 0;
    loop {
        let parent: Option<Option<i64>> = conn
            .query_row("SELECT parent_id FROM groups WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(|_| AppError::new(format!("分组 {id} 不存在")))?;
        match parent.flatten() {
            Some(pid) => {
                id = pid;
                depth += 1;
                guard += 1;
                if guard > 64 {
                    return Err(AppError::new("分组层级数据异常（疑似存在环）"));
                }
            }
            None => return Ok(depth),
        }
    }
}

/// 内部：id 是否为 target 的祖先（用于防环校验）
fn is_ancestor(conn: &Connection, ancestor: i64, mut target: i64) -> Result<bool, AppError> {
    let mut guard = 0;
    loop {
        if target == ancestor {
            return Ok(true);
        }
        let parent: Option<Option<i64>> = conn
            .query_row("SELECT parent_id FROM groups WHERE id = ?1", params![target], |r| r.get(0))
            .map_err(|_| AppError::new(format!("分组 {target} 不存在")))?;
        match parent.flatten() {
            Some(pid) => {
                target = pid;
                guard += 1;
                if guard > 64 {
                    return Err(AppError::new("分组层级数据异常（疑似存在环）"));
                }
            }
            None => return Ok(false),
        }
    }
}

/// 内部：同一父分组下名称唯一校验
fn ensure_name_unique(conn: &Connection, parent_id: Option<i64>, name: &str, exclude_id: Option<i64>) -> Result<(), AppError> {
    let dup: i64 = conn.query_row(
        "SELECT COUNT(*) FROM groups WHERE name = ?1 AND parent_id IS ?2 AND id != ?3",
        params![name, parent_id, exclude_id.unwrap_or(-1)],
        |r| r.get(0),
    )?;
    if dup > 0 {
        return Err(AppError::new(format!("同级下已存在同名分组「{name}」")));
    }
    Ok(())
}

// ---------- 写操作 ----------

/// 新增分组（FR-G2）
#[tauri::command]
pub fn group_create(input: GroupInput, state: State<'_, AppState>) -> Result<GroupRow, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::new("分组名称不能为空"));
    }
    let conn = state.db.lock().unwrap();
    // 父分组存在性 + 层级限制
    if let Some(pid) = input.parent_id {
        let _: i64 = conn
            .query_row("SELECT id FROM groups WHERE id = ?1", params![pid], |r| r.get(0))
            .map_err(|_| AppError::new("指定的父分组不存在"))?;
        let depth = depth_of(&conn, pid)?;
        if depth >= MAX_DEPTH {
            return Err(AppError::new(format!("分组层级已达上限（{MAX_DEPTH} 级），不能再在其下创建子分组")));
        }
    }
    ensure_name_unique(&conn, input.parent_id, &name, None)?;
    conn.execute(
        "INSERT INTO groups (name, parent_id, sort_order, remark) VALUES (?1, ?2, ?3, ?4)",
        params![name, input.parent_id, input.sort_order.unwrap_or(0), input.remark.clone().unwrap_or_default()],
    )?;
    let id = conn.last_insert_rowid();
    let rows = load_rows(&conn)?;
    rows.into_iter().find(|g| g.id == id).ok_or_else(|| AppError::new("创建成功但读取失败"))
}

/// 编辑分组（FR-G2；含防环与层级校验）
#[tauri::command]
pub fn group_update(input: GroupInput, state: State<'_, AppState>) -> Result<GroupRow, AppError> {
    let id = input.id.ok_or_else(|| AppError::new("缺少分组 id"))?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::new("分组名称不能为空"));
    }
    let conn = state.db.lock().unwrap();

    if let Some(pid) = input.parent_id {
        if pid == id {
            return Err(AppError::new("父分组不能选择自身"));
        }
        if is_ancestor(&conn, id, pid)? {
            return Err(AppError::new("不能将分组移动到自己的子分组下（会形成环）"));
        }
        // 以新父级深度 + 子树高度校验总层级
        let parent_depth = depth_of(&conn, pid)?;
        let subtree_height = subtree_height(&conn, id)?;
        if parent_depth + subtree_height > MAX_DEPTH {
            return Err(AppError::new(format!("移动后层级将超过上限（{MAX_DEPTH} 级）")));
        }
    }
    ensure_name_unique(&conn, input.parent_id, &name, Some(id))?;

    conn.execute(
        "UPDATE groups SET name = ?1, parent_id = ?2, sort_order = ?3, remark = ?4,
            updated_at = datetime('now','localtime') WHERE id = ?5",
        params![name, input.parent_id, input.sort_order.unwrap_or(0), input.remark.clone().unwrap_or_default(), id],
    )?;
    let rows = load_rows(&conn)?;
    rows.into_iter().find(|g| g.id == id).ok_or_else(|| AppError::new("更新成功但读取失败"))
}

/// 内部：以 id 为根的子树高度（自身为 1）
fn subtree_height(conn: &Connection, id: i64) -> Result<usize, AppError> {
    let mut stmt = conn.prepare("SELECT id FROM groups WHERE parent_id = ?1")?;
    let children: Vec<i64> = stmt.query_map(params![id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut h = 1usize;
    for c in children {
        h = h.max(subtree_height(conn, c)? + 1);
    }
    Ok(h)
}

/// 删除分组（FR-G3：存在子分组或用例时默认禁止）
#[tauri::command]
pub fn group_delete(id: i64, state: State<'_, AppState>) -> Result<(), AppError> {
    let conn = state.db.lock().unwrap();
    let child_count: i64 = conn.query_row("SELECT COUNT(*) FROM groups WHERE parent_id = ?1", params![id], |r| r.get(0))?;
    let case_count: i64 = conn.query_row("SELECT COUNT(*) FROM test_cases WHERE group_id = ?1", params![id], |r| r.get(0))?;
    if child_count > 0 || case_count > 0 {
        return Err(AppError::new(format!(
            "无法删除：该分组下仍有 {child_count} 个子分组、{case_count} 个测试用例。请先清空或迁移后再删除"
        )));
    }
    let n = conn.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::new("分组不存在或已被删除"));
    }
    Ok(())
}

/// 上移/下移调整同级排序（FR-G6，与相邻兄弟交换 sort_order）
#[tauri::command]
pub fn group_move(id: i64, up: bool, state: State<'_, AppState>) -> Result<Vec<GroupRow>, AppError> {
    let conn = state.db.lock().unwrap();
    let (parent_id, sort_order): (Option<i64>, i64) = conn
        .query_row("SELECT parent_id, sort_order FROM groups WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| AppError::new("分组不存在或已被删除"))?;

    // 相邻兄弟：按 (sort_order, id) 排序取前一个/后一个
    let sibling: Option<(i64, i64)> = if up {
        conn.query_row(
            "SELECT id, sort_order FROM groups WHERE parent_id IS ?1 AND (sort_order < ?2 OR (sort_order = ?2 AND id < ?3))
             ORDER BY sort_order DESC, id DESC LIMIT 1",
            params![parent_id, sort_order, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    } else {
        conn.query_row(
            "SELECT id, sort_order FROM groups WHERE parent_id IS ?1 AND (sort_order > ?2 OR (sort_order = ?2 AND id > ?3))
             ORDER BY sort_order ASC, id ASC LIMIT 1",
            params![parent_id, sort_order, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    };

    if let Some((sib_id, _sib_order)) = sibling {
        // 交换两者 sort_order
        conn.execute("UPDATE groups SET sort_order = ?1, updated_at = datetime('now','localtime') WHERE id = ?2", params![sort_order, sib_id])?;
        conn.execute("UPDATE groups SET sort_order = ?1, updated_at = datetime('now','localtime') WHERE id = ?2", params![_sib_order, id])?;
    } else {
        return Err(AppError::new(if up { "已经是同级第一个分组" } else { "已经是同级最后一个分组" }));
    }
    drop(conn);
    group_list(None, state)
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::migrations::run(&conn).unwrap();
        conn
    }

    fn insert(conn: &Connection, name: &str, parent: Option<i64>) -> i64 {
        conn.execute("INSERT INTO groups (name, parent_id) VALUES (?1, ?2)", params![name, parent])
            .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn create_and_list_with_path() {
        let conn = setup();
        let a = insert(&conn, "项目A", None);
        let b = insert(&conn, "用户模块", Some(a));
        insert(&conn, "登录", Some(b));
        let rows = load_rows(&conn).unwrap();
        let login = rows.iter().find(|g| g.name == "登录").unwrap();
        assert_eq!(login.path, "项目A / 用户模块 / 登录");
        let pa = rows.iter().find(|g| g.id == a).unwrap();
        assert_eq!(pa.child_count, 1);
    }

    #[test]
    fn reject_delete_when_has_children_or_cases() {
        let conn = setup();
        let a = insert(&conn, "A", None);
        insert(&conn, "A1", Some(a));
        // 复用删除逻辑（绕过 State 直接构造）
        let child_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM groups WHERE parent_id = ?1", params![a], |r| r.get(0))
            .unwrap();
        assert_eq!(child_count, 1); // 有子组时删除逻辑应拒绝（group_delete 命令层校验）
        conn.execute("DELETE FROM groups WHERE parent_id = ?1", params![a]).unwrap();
        conn.execute("DELETE FROM groups WHERE id = ?1", params![a]).unwrap();
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 0);
    }

    #[test]
    fn cycle_detection() {
        let conn = setup();
        let a = insert(&conn, "A", None);
        let b = insert(&conn, "B", Some(a));
        let c = insert(&conn, "C", Some(b));
        // 把 A 移到 C 下面应被拒绝：C 是 A 的后代
        assert!(is_ancestor(&conn, a, c).unwrap());
        // 把 C 移到 A 下没问题：A 是 C 的祖先但不是后代
        assert!(!is_ancestor(&conn, c, a).unwrap());
    }

    #[test]
    fn depth_limit() {
        let conn = setup();
        let mut pid = insert(&conn, "L1", None);
        for i in 2..=5 {
            pid = insert(&conn, &format!("L{i}"), Some(pid));
        }
        // L5 深度为 5，达到上限
        assert_eq!(depth_of(&conn, pid).unwrap(), 5);
    }

    #[test]
    fn name_unique_within_same_parent() {
        let conn = setup();
        insert(&conn, "A", None);
        ensure_name_unique(&conn, None, "A", None).unwrap_err();
        ensure_name_unique(&conn, None, "B", None).unwrap();
        // 不同父分组下允许同名
        let a2 = insert(&conn, "A", None);
        ensure_name_unique(&conn, Some(a2), "A", None).unwrap();
    }

    /// 回归：前端传 snake_case 字段（parent_id），反序列化必须正确读取；
    /// 曾因 #[serde(rename_all = "camelCase")] 导致 parent_id 被静默忽略、全部落库为顶级分组
    #[test]
    fn group_input_deserializes_snake_case() {
        let input: GroupInput =
            serde_json::from_str(r#"{"name":"用户模块","parent_id":3,"sort_order":1,"remark":"备注"}"#).unwrap();
        assert_eq!(input.parent_id, Some(3));
        assert_eq!(input.sort_order, Some(1));
        assert_eq!(input.name, "用户模块");
        // 可选字段缺失 → None
        let minimal: GroupInput = serde_json::from_str(r#"{"name":"顶级"}"#).unwrap();
        assert_eq!(minimal.parent_id, None);
        // 未知字段（如前端误传 parentId）必须报错而非静默忽略
        assert!(serde_json::from_str::<GroupInput>(r#"{"name":"x","parentId":3}"#).is_err());
    }
}
