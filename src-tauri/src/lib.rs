mod agent;
mod analysis_history;
mod builtin_agents;
mod commands;
mod db;
mod error;
mod import;
mod llm;
mod migration_history;
mod models;
mod paths;
mod runtime;
mod runtime_log;
mod schema_cache;
mod sql_audit;
mod state;

use agent::{agent_status, list_agent_audit, set_agent_enabled};
use commands::{
    agent_append_transcript, agent_cancel_run, agent_compress_context, agent_create_session,
    agent_list_sessions, agent_load_session, agent_read_context, agent_read_system,
    agent_runner_status, agent_save_checkpoint, agent_stage_attachments, agent_start_run,
    agent_tool_call, agents_list, complete_llm, connect_session, create_database,
    delete_analysis_history, delete_connection, delete_migration_history, delete_query_cache,
    disconnect_session, drop_database, dump_database, dump_table, execute_sql, explain_plan,
    get_columns, get_ddl,
    import_cancel, import_infer_schema, import_parse_sample, import_prepare, import_read_text_file,
    import_start, import_status, insights_read_latest, insights_start_run, list_analysis_history, list_charset_catalog, list_connections,
    list_databases, list_indexes, list_migration_history, list_routines, list_schema_objects,
    list_server_databases,
    list_tables, list_triggers, list_views, migrate_database, switch_database, poll_llm, preview_table, read_analysis_history, read_llm_catalog,
    read_locale, read_migration_history, read_query_cache, read_runtime_settings,
    save_analysis_history, save_migration_history, start_llm, table_row_count, test_connection,
    upsert_connection, write_llm_catalog, write_locale, write_query_cache, write_runtime_settings,
    write_binary_file, write_text_file, list_sql_audit, list_sql_audit_days, read_sql_audit_settings,
    write_sql_audit_settings,
};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let paths = crate::paths::AppPaths::from_home(&crate::paths::home_dir()?);
            let legacy = app.path().app_data_dir().ok();
            // File logger under ~/.hidatalinker/runtime (daily files, 7-day retention).
            let runtime_log = crate::runtime_log::init(&paths)?;
            app.manage(runtime_log);
            crate::runtime::handle();
            app.manage(AppState::load(paths.clone(), legacy)?);
            let runner = crate::builtin_agents::AgentRunner::new(app.handle().clone());
            app.manage(runner);
            crate::agent::start(app.handle().clone(), &paths)?;
            // Warm schema metadata cache in the background (24h TTL).
            let refresh_handle = app.handle().clone();
            crate::runtime::handle().spawn(async move {
                let state = refresh_handle.state::<AppState>();
                if let Err(error) = crate::schema_cache::refresh_stale(&state).await {
                    log::warn!("schema cache startup refresh: {}", error.user_message());
                }
            });
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
            agent_read_context,
            agent_compress_context,
            agent_tool_call,
            agent_start_run,
            agent_cancel_run,
            agent_runner_status,
            agent_stage_attachments,
            insights_start_run,
            insights_read_latest,
            import_parse_sample,
            import_infer_schema,
            import_prepare,
            import_start,
            import_cancel,
            import_status,
            import_read_text_file,
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
            read_runtime_settings,
            write_runtime_settings,
            read_sql_audit_settings,
            write_sql_audit_settings,
            list_sql_audit,
            list_sql_audit_days,
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
            list_server_databases,
            switch_database,
            list_databases,
            create_database,
            drop_database,
            dump_database,
            dump_table,
            write_text_file,
            write_binary_file,
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
            list_schema_objects,
            preview_table,
            table_row_count,
            execute_sql,
            explain_plan,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn run_agent_mcp() {
    agent::run_mcp();
}
