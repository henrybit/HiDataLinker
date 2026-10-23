// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|arg| arg == "--agent-mcp") {
        app_lib::run_agent_mcp();
        return;
    }
    app_lib::run();
}
