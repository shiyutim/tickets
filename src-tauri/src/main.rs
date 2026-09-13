#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use tickets::{bilibili, clock, dm, notifications, subscriptions, tasks, utils, version};

fn main() {
    tauri::Builder::default()
        .manage(tasks::TaskManager::default())
        .setup(|app| {
            let directory = app
                .path_resolver()
                .app_data_dir()
                .ok_or("无法找到应用数据目录")?;
            let wechat =
                notifications::WechatManager::new(directory.join("wechat").join("binding.json"))?;
            wechat.resume();
            app.manage(wechat);
            Ok(())
        })
        .plugin(tauri_plugin_sql::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            dm::dm_project,
            dm::dm_tickets,
            dm::dm_buyers,
            bilibili::bili_search,
            bilibili::bili_project,
            bilibili::bili_buyers,
            bilibili::bili_addresses,
            bilibili::bili_screens,
            tasks::start_ticket_task,
            notifications::test_wechat_notification,
            notifications::get_wechat_status,
            notifications::start_wechat_login,
            notifications::poll_wechat_login,
            notifications::cancel_wechat_login,
            notifications::disconnect_wechat,
            tasks::cancel_ticket_task,
            tasks::list_ticket_tasks,
            tasks::provide_ticket_credentials,
            clock::sync_clock,
            subscriptions::refresh_subscription,
            subscriptions::remove_subscription,
            version::get_repo_release,
            utils::export_sql_to_txt,
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 Tickets");
}
