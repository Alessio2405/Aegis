// Prevents additional console window on Windows in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use tauri::Manager;

use aegis_lib::{
    db,
    state::{AppState, BotSharedState},
    commands::*,
};

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Resolve the app data directory — platform-appropriate location
            let app_data = app
                .path_resolver()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            std::fs::create_dir_all(&app_data)
                .expect("Failed to create app data directory");

            let db_path   = app_data.join("vault.db");
            let salt_path = app_data.join("vault_salt.bin");

            eprintln!("[init] Vault DB path: {}", db_path.display());

            let conn = db::open(&db_path).expect("Failed to open/create vault database");

            // Create the shared bot state
            let bot = Arc::new(BotSharedState::new(db_path.clone(), app.handle()));

            // Auto-start Telegram bot if it was enabled in settings
            {
                let telegram_enabled = db::get_setting(&conn, "telegram_enabled")
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some("true");

                if telegram_enabled {
                    let token = db::get_setting(&conn, "telegram_token")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let chat_id_str = db::get_setting(&conn, "telegram_chat_id")
                        .ok()
                        .flatten()
                        .unwrap_or_default();

                    if !token.is_empty() && !chat_id_str.is_empty() {
                        if let Ok(chat_id) = chat_id_str.parse::<i64>() {
                            eprintln!("[init] Auto-starting Telegram bot");
                            aegis_lib::telegram::start(token, chat_id, Arc::clone(&bot));
                        }
                    }
                }
            }

            // Auto-start Discord bot if it was enabled in settings
            {
                let discord_enabled = db::get_setting(&conn, "discord_enabled")
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some("true");

                if discord_enabled {
                    let token = db::get_setting(&conn, "discord_token")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let user_id = db::get_setting(&conn, "discord_allowed_user_id")
                        .ok()
                        .flatten()
                        .unwrap_or_default();

                    if !token.is_empty() && !user_id.is_empty() {
                        eprintln!("[init] Auto-starting Discord bot");
                        aegis_lib::discord::start(token, user_id, Arc::clone(&bot));
                    }
                }
            }

            // Auto-start WhatsApp webhook server if it was enabled in settings
            {
                let whatsapp_enabled = db::get_setting(&conn, "whatsapp_enabled")
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some("true");

                if whatsapp_enabled {
                    let access_token    = db::get_setting(&conn, "whatsapp_access_token").ok().flatten().unwrap_or_default();
                    let phone_number_id = db::get_setting(&conn, "whatsapp_phone_number_id").ok().flatten().unwrap_or_default();
                    let allowed_number  = db::get_setting(&conn, "whatsapp_allowed_number").ok().flatten().unwrap_or_default();
                    let verify_token    = db::get_setting(&conn, "whatsapp_verify_token").ok().flatten().unwrap_or_default();
                    let port_str        = db::get_setting(&conn, "whatsapp_webhook_port").ok().flatten().unwrap_or_else(|| "8765".into());
                    let port: u16       = port_str.parse().unwrap_or(8765);

                    if !access_token.is_empty() && !phone_number_id.is_empty() && !allowed_number.is_empty() {
                        eprintln!("[init] Auto-starting WhatsApp webhook server on port {port}");
                        aegis_lib::whatsapp::start(
                            access_token, phone_number_id, allowed_number,
                            verify_token, port, Arc::clone(&bot),
                        );
                    }
                }
            }

            // Start the agent scheduler
            eprintln!("[init] Starting agent scheduler");
            aegis_lib::scheduler::start(Arc::clone(&bot));

            // Start the file watcher
            eprintln!("[init] Starting file watcher");
            aegis_lib::watcher::start(Arc::clone(&bot));

            app.manage(AppState {
                db: Mutex::new(conn),
                crypto: Mutex::new(None),
                salt_path,
                db_path,
                bot,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Vault
            get_vault_status,
            create_vault,
            unlock_vault,
            lock_vault,
            // Settings
            get_settings,
            update_settings,
            // Workspaces
            add_workspace,
            list_workspaces,
            list_documents,
            ingest_workspace,
            // Q&A
            ask_question,
            get_chunk_source,
            // Memories
            list_memories,
            accept_memory,
            edit_memory,
            reject_memory,
            delete_memory,
            propose_memory,
            // Permissions
            list_permissions,
            revoke_permission,
            check_permission,
            // Audit
            list_audit_events,
            // Chat
            chat_message,
            chat_message_stream,
            list_chat_history,
            clear_chat_history,
            // Agents
            create_agent,
            list_agents,
            update_agent,
            delete_agent,
            run_agent,
            // Pipelines
            create_pipeline,
            list_pipelines,
            delete_pipeline,
            run_pipeline,
            list_agent_runs,
            // File output
            generate_file,
            open_file,
            // Desktop automation
            desktop_type,
            desktop_key,
            desktop_open,
            // Telegram bot
            start_telegram_bot,
            stop_telegram_bot,
            get_telegram_status,
            // Discord bot
            start_discord_bot,
            stop_discord_bot,
            get_discord_status,
            // WhatsApp bot
            start_whatsapp_bot,
            stop_whatsapp_bot,
            get_whatsapp_status,
            // Python IDE
            get_python_info,
            run_python,
            // Scheduler / Hands
            list_hand_templates,
            get_scheduled_agents,
            get_scheduler_status,
            trigger_agent_now,
            pause_agent_schedule,
            resume_agent_schedule,
            replay_agent_run,
            // Utils
            open_folder_dialog,
            // Artifacts
            list_artifacts,
            delete_artifact,
            // MCP servers
            list_mcp_servers,
            add_mcp_server,
            remove_mcp_server,
            refresh_mcp_tools,
            // Task queue
            enqueue_task,
            list_tasks,
            cancel_task,
            get_task,
            // Scratchpad
            list_scratchpad,
            delete_scratchpad_key,
            // Watch triggers
            add_trigger,
            list_triggers,
            remove_trigger,
            toggle_trigger,
            get_watcher_status,
            // Activity dashboard
            get_system_status,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Aegis");
}
