//! Application state — shared between all Tauri commands via Tauri managed state.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::PathBuf;
use rusqlite::Connection;
use crate::crypto::CryptoContext;

/// State shared between the main Tauri thread and the background Telegram bot thread.
/// Wrapped in Arc so both sides hold a reference to the same allocation.
pub struct BotSharedState {
    /// Path to the SQLite DB — the bot opens its own Connection per operation.
    pub db_path: PathBuf,
    /// Raw vault key bytes (32 bytes). Set when vault unlocks, cleared when it locks.
    /// None means the vault is locked; bot will refuse commands that need decryption.
    pub shared_key: Mutex<Option<[u8; 32]>>,
    /// Set to false to signal the Telegram bot polling thread to exit gracefully.
    pub running: AtomicBool,
    /// Set to false to signal the Discord bot thread to exit gracefully.
    pub discord_running: AtomicBool,
    /// Set to false to signal the WhatsApp webhook server thread to exit gracefully.
    pub whatsapp_running: AtomicBool,
    /// Set to false to signal the scheduler background thread to exit gracefully.
    pub scheduler_running: AtomicBool,
    /// Set to false to signal the file-watcher background thread to exit gracefully.
    pub watcher_running: AtomicBool,
    /// Tauri app handle — used by background threads to fire desktop notifications.
    pub app_handle: tauri::AppHandle,
}

impl BotSharedState {
    pub fn new(db_path: PathBuf, app_handle: tauri::AppHandle) -> Self {
        Self {
            db_path,
            shared_key: Mutex::new(None),
            running: AtomicBool::new(false),
            discord_running: AtomicBool::new(false),
            whatsapp_running: AtomicBool::new(false),
            scheduler_running: AtomicBool::new(false),
            watcher_running: AtomicBool::new(false),
            app_handle,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn discord_is_running(&self) -> bool {
        self.discord_running.load(Ordering::SeqCst)
    }

    pub fn whatsapp_is_running(&self) -> bool {
        self.whatsapp_running.load(Ordering::SeqCst)
    }

    pub fn scheduler_is_running(&self) -> bool {
        self.scheduler_running.load(Ordering::SeqCst)
    }

    pub fn watcher_is_running(&self) -> bool {
        self.watcher_running.load(Ordering::SeqCst)
    }

    /// Show a native desktop notification (best-effort — never panics).
    pub fn notify(&self, title: &str, body: &str) {
        let id = &self.app_handle.config().tauri.bundle.identifier;
        let _ = tauri::api::notification::Notification::new(id)
            .title(title)
            .body(body)
            .show();
    }
}

/// All shared runtime state — injected into every Tauri command via State<AppState>.
pub struct AppState {
    /// Main SQLite connection. Mutex for exclusive access from command handlers.
    pub db: Mutex<Connection>,
    /// In-memory vault key. Some = unlocked, None = locked.
    pub crypto: Mutex<Option<CryptoContext>>,
    /// Path to the Argon2 salt file (sibling to vault.db).
    pub salt_path: PathBuf,
    /// Path to the SQLite DB file — passed to background threads that need their own connection.
    pub db_path: PathBuf,
    /// Shared state with the Telegram bot background thread.
    pub bot: Arc<BotSharedState>,
}

impl AppState {
    pub fn is_unlocked(&self) -> bool {
        self.crypto.lock().unwrap().is_some()
    }

    /// Run a closure with a reference to the active CryptoContext.
    /// Returns VaultLocked error if the vault is not open.
    pub fn with_crypto<F, T>(&self, f: F) -> crate::error::AppResult<T>
    where
        F: FnOnce(&CryptoContext) -> crate::error::AppResult<T>,
    {
        let guard = self.crypto.lock().unwrap();
        match guard.as_ref() {
            Some(ctx) => f(ctx),
            None => Err(crate::error::AppError::VaultLocked),
        }
    }
}
