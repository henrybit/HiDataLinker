mod agent;
mod analysis_history;
mod builtin_agents;
mod commands;
mod db;
mod error;
mod llm;
mod migration_history;
mod models;
mod paths;
mod runtime;
mod state;

use agent::{agent_status, list_agent_audit, set_agent_enabled};
use commands::{
    agent_append_transcript, agent_cancel_run, agent_create_session, agent_list_sessions,
    agent_load_session, agent_read_system, agent_runner_status, agent_save_checkpoint,
    agent_start_run, agent_tool_call, agents_list, complete_llm,
    connect_session, create_database, delete_analysis_history, delete_connection,
    delete_migration_history, delete_query_cache, disconnect_session, drop_database, dump_database,
    dump_table, execute_sql, get_columns, get_ddl, list_analysis_history, list_charset_catalog,
    list_connections, list_databases, list_indexes, list_migration_history, list_routines,
    list_tables, list_triggers, list_views, migrate_database, poll_llm, preview_table,
    read_analysis_history, read_llm_catalog, read_locale, read_migration_history, read_query_cache,
    save_analysis_history, save_migration_history, start_llm, table_row_count, test_connection,
    upsert_connection, write_llm_catalog, write_locale, write_query_cache, write_text_file,
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
            app.manage(AppState::load(paths.clone(), legacy)?);
            let runner = crate::builtin_agents::AgentRunner::new(app.handle().clone());
            app.manage(runner);
            crate::agent::start(app.handle().clone(), &paths)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agent_status,
            set_agent_enabled,
            list_agent_audit,
            agents_list,
            agent_read_system,
            agent_list_sessions,
            agent_create_session,
            agent_load_session,
            agent_append_transcript,
            agent_save_checkpoint,
            agent_tool_call,
            agent_start_run,
            agent_cancel_run,
            agent_runner_status,
            complete_llm,
            start_llm,
            poll_llm,
            list_analysis_history,
            read_analysis_history,
            list_migration_history,
            read_migration_history,
            read_llm_catalog,
            write_llm_catalog,
            read_locale,
            write_locale,
            save_analysis_history,
            delete_analysis_history,
            save_migration_history,
            delete_migration_history,
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

pub fn run_agent_mcp() {
    agent::run_mcp();
}
