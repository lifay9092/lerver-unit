mod assert_expr;
mod cases;
mod executor;
mod reports;
mod commands;
mod db;
mod env_config;
mod error;
mod groups;
mod import;
mod migrations;

use std::sync::Mutex;

use tauri::Manager;

/// 全局应用状态
pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub env: Mutex<env_config::EnvFile>,
    pub data_dir: std::path::PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = db::data_dir(&app.handle())?;
            let conn = db::init_db(&dir)?;
            import::ensure_builtin_templates(&conn)?;
            let env = env_config::load_or_create(&dir)?;
            app.manage(AppState {
                db: Mutex::new(conn),
                env: Mutex::new(env),
                data_dir: dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping_db,
            commands::env_list,
            commands::env_switch,
            commands::env_reload,
            commands::env_save,
            commands::data_dir,
            commands::resolve_url,
            groups::group_list,
            groups::group_create,
            groups::group_update,
            groups::group_delete,
            groups::group_move,
            groups::group_tree,
            cases::case_page,
            cases::case_create,
            cases::case_update,
            cases::case_delete,
            cases::case_ids_by_group,
            cases::assert_expr_validate,
            executor::run_cases,
            executor::try_run,
            import::template_list,
            import::template_create,
            import::template_update,
            import::template_delete,
            import::template_copy,
            import::template_export,
            import::read_xlsx_headers,
            import::read_json_fields,
            import::import_preview,
            import::import_commit,
            reports::report_page,
            reports::report_detail,
            reports::report_delete,
            reports::report_export
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
