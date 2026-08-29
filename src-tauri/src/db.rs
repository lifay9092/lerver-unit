use rusqlite::Connection;
use std::path::PathBuf;

use crate::error::AppError;

/// 应用数据目录（数据库、环境配置文件都放在这里）
/// Windows 下实际路径：%APPDATA%\com.leverunit.app\
pub fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    use tauri::Manager;
    let dir = app.path().app_data_dir()?;
    Ok(dir)
}

/// 打开（必要时创建）数据库并执行建表迁移
pub fn init_db(dir: &PathBuf) -> Result<Connection, AppError> {
    std::fs::create_dir_all(dir)?;
    let conn = Connection::open(dir.join("lever-unit.db"))?;
    crate::migrations::run(&conn)?;
    Ok(conn)
}
