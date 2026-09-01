use rusqlite::Connection;
use std::path::PathBuf;

use crate::error::AppError;

/// 应用数据目录（数据库 lever-unit.db、环境配置 environments.json 都放在这里）
/// 改为「当前可执行文件所在目录」：数据库与配置随程序走，便于便携部署/整体拷贝。
pub fn data_dir(_app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| AppError::new("无法获取可执行文件所在目录"))?
        .to_path_buf();
    Ok(dir)
}

/// 打开（必要时创建）数据库并执行建表迁移
pub fn init_db(dir: &PathBuf) -> Result<Connection, AppError> {
    std::fs::create_dir_all(dir)?;
    let conn = Connection::open(dir.join("lever-unit.db"))?;
    crate::migrations::run(&conn)?;
    Ok(conn)
}
