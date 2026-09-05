#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tickets::{bilibili, clock, dm, tasks, utils};

fn main() {
    tauri::Builder::default()
        .manage(tasks::TaskManager::default())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            dm::dm_project,
            dm::dm_tickets,
            dm::dm_buyers,
            bilibili::bili_project,
            bilibili::bili_buyers,
            bilibili::bili_addresses,
            bilibili::bili_screens,
            tasks::start_ticket_task,
            tasks::cancel_ticket_task,
            tasks::list_ticket_tasks,
            tasks::provide_ticket_credentials,
            clock::sync_clock,
            utils::export_sql_to_txt,
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 Tickets");
}
