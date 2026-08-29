use tauri::{AppHandle, State};

use crate::env_config::{self, EnvFile};
use crate::error::AppError;
use crate::AppState;

/// 冒烟验证：返回 SQLite 版本与数据目录（M1 验收用）
#[tauri::command]
pub fn ping_db(app: AppHandle, state: State<'_, AppState>) -> Result<String, AppError> {
    let version: String = state
        .db
        .lock()
        .unwrap()
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let dir = crate::db::data_dir(&app)?;
    Ok(format!("SQLite {version} @ {}", dir.display()))
}

/// 环境列表 + 当前激活环境（FR-E3）
#[tauri::command]
pub fn env_list(state: State<'_, AppState>) -> Result<EnvFile, AppError> {
    Ok(state.env.lock().unwrap().clone())
}

/// 切换激活环境并持久化到 environments.json（FR-E3）
#[tauri::command]
pub fn env_switch(id: String, state: State<'_, AppState>) -> Result<EnvFile, AppError> {
    let mut env = state.env.lock().unwrap();
    if !env.environments.iter().any(|e| e.id == id) {
        return Ok(env.clone());
    }
    env.active = id;
    env_config::save(&state.data_dir, &env)?;
    Ok(env.clone())
}

/// 重新加载环境配置文件（FR-E4，支持手工编辑后刷新）
#[tauri::command]
pub fn env_reload(state: State<'_, AppState>) -> Result<EnvFile, AppError> {
    let cfg = env_config::load_or_create(&state.data_dir)?;
    let mut env = state.env.lock().unwrap();
    *env = cfg.clone();
    Ok(cfg)
}

/// 整体保存环境配置（FR-E5 环境管理小窗）：校验后写回 environments.json 并刷新内存
#[tauri::command]
pub fn env_save(file: EnvFile, state: State<'_, AppState>) -> Result<EnvFile, AppError> {
    // 校验：至少一个环境；id 非空且唯一；active 必须指向存在的环境，否则回退到第一个
    if file.environments.is_empty() {
        return Err(AppError::new("至少需要保留一个环境"));
    }
    let mut seen = std::collections::HashSet::new();
    for e in &file.environments {
        let id = e.id.trim();
        if id.is_empty() {
            return Err(AppError::new("环境 id 不能为空"));
        }
        if !seen.insert(id.to_string()) {
            return Err(AppError::new(format!("环境 id 重复：{id}")));
        }
    }
    let mut file = file;
    for e in &mut file.environments {
        e.id = e.id.trim().to_string();
        e.name = e.name.trim().to_string();
        e.base_url = e.base_url.trim().to_string();
    }
    if !file.environments.iter().any(|e| e.id == file.active) {
        file.active = file.environments[0].id.clone();
    }

    env_config::save(&state.data_dir, &file)?;
    let mut env = state.env.lock().unwrap();
    *env = file.clone();
    Ok(file)
}

/// 数据目录路径（NFR-3：前端「打开数据目录」用）
#[tauri::command]
pub fn data_dir(state: State<'_, AppState>) -> Result<String, AppError> {
    Ok(state.data_dir.to_string_lossy().to_string())
}

/// 按当前激活环境解析 URL（FR-C11 预览 / 调试用）
#[tauri::command]
pub fn resolve_url(url: String, state: State<'_, AppState>) -> Result<String, AppError> {
    let env = state.env.lock().unwrap();
    env_config::resolve_url(&url, env.active_base_url()).map_err(AppError::new)
}
