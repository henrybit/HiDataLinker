mod analysis_history;
mod commands;
mod db;
mod error;
mod llm;
mod models;
mod paths;
mod runtime;
mod state;

use commands::{
    complete_llm, connect_session, create_database, delete_analysis_history, delete_connection,
    delete_query_cache, disconnect_session, drop_database, dump_database, dump_table, execute_sql,
    get_columns, get_ddl, list_analysis_history, list_charset_catalog, list_connections,
    list_databases, list_indexes, list_routines, list_tables, list_triggers, list_views,
    migrate_database, poll_llm, preview_table, read_analysis_history, read_llm_catalog,
    read_locale, read_query_cache, save_analysis_history, start_llm, table_row_count,
    test_connection, upsert_connection, write_llm_catalog, write_locale, write_query_cache,
    write_text_file,
};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let paths = crate::paths::AppPaths::from_home(&crate::paths::home_dir()?);
            let legacy = app.path().app_data_dir().ok();
            crate::runtime::handle();
            app.manage(AppState::load(paths, legacy)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            complete_llm,
            start_llm,
            poll_llm,
            list_analysis_history,
            read_analysis_history,
            read_llm_catalog,
            write_llm_catalog,
            read_locale,
            write_locale,
            save_analysis_history,
            delete_analysis_history,
            list_connections,
            upsert_connection,
            delete_connection,
            test_connection,
            connect_session,
            disconnect_session,
            list_databases,
            create_database,
            drop_database,
            dump_database,
            dump_table,
            write_text_file,
            write_query_cache,
            read_query_cache,
            delete_query_cache,
            migrate_database,
            list_charset_catalog,
            list_tables,
            list_views,
            list_indexes,
            list_triggers,
            list_routines,
            get_columns,
            get_ddl,
            preview_table,
            table_row_count,
            execute_sql,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
