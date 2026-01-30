mod attribution;
mod commands;
mod file_watcher;
mod git_diff;
mod import;
mod link_commands;
mod linking;
mod models;
mod otlp_receiver;
mod session_hash;
mod session_links;

use notify::RecommendedWatcher;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_sql::{Migration, MigrationKind};

/// Global file watcher state
static FILE_WATCHER: std::sync::Mutex<Option<RecommendedWatcher>> = std::sync::Mutex::new(None);

/// Start the file watcher for auto-import
#[tauri::command]
fn start_file_watcher(app_handle: tauri::AppHandle, repo_root: String) -> Result<(), String> {
    // Stop existing watcher if any
    {
        let mut watcher = FILE_WATCHER.lock().map_err(|e| e.to_string())?;
        if watcher.is_some() {
            drop(watcher.take());
        }
    }

    // Start new watcher
    let new_watcher = file_watcher::start_session_watcher(app_handle, repo_root)?;

    {
        let mut watcher = FILE_WATCHER.lock().map_err(|e| e.to_string())?;
        *watcher = Some(new_watcher);
    }

    Ok(())
}

/// Database state wrapper for Tauri commands
pub struct DbState(pub Arc<SqlitePool>);

impl std::ops::Deref for DbState {
    type Target = Arc<SqlitePool>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let migrations = vec![
        Migration {
            version: 1,
            description: "create_initial_tables",
            sql: include_str!("../migrations/001_init.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "add_session_links_table",
            sql: include_str!("../migrations/002_add_session_links.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "add_agent_trace",
            sql: include_str!("../migrations/003_add_agent_trace.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 4,
            description: "add_session_attribution",
            sql: include_str!("../migrations/004_session_attribution.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 5,
            description: "add_attribution_notes",
            sql: include_str!("../migrations/005_attribution_notes.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 6,
            description: "add_commit_rewrite_keys",
            sql: include_str!("../migrations/006_rewrite_keys.sql"),
            kind: MigrationKind::Up,
        },
    ];

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::ensure_narrative_dirs,
            commands::write_narrative_file,
            commands::read_narrative_file,
            commands::list_narrative_files,
            commands::read_text_file,
            // Session link commands
            session_links::create_or_update_session_link,
            session_links::get_session_links_for_repo,
            session_links::get_session_links_for_commit,
            session_links::delete_session_link,
            // Linking algorithm commands
            link_commands::link_session_to_commit,
            link_commands::import_and_link_session_file,
            // Import commands
            import::commands::import_session_files,
            import::commands::import_session_file,
            import::commands::scan_for_session_files,
            import::commands::get_recent_sessions,
            // Git diff commands
            git_diff::get_commit_added_ranges,
            // Attribution commands
            attribution::commands::get_commit_contribution_stats,
            attribution::commands::get_file_source_lens,
            attribution::commands::compute_stats_batch,
            attribution::commands::import_attribution_note,
            attribution::commands::import_attribution_notes_batch,
            attribution::commands::export_attribution_note,
            // OTLP receiver commands
            otlp_receiver::set_active_repo_root,
            otlp_receiver::set_otlp_receiver_enabled,
            otlp_receiver::run_otlp_smoke_test,
            // File watcher commands
            start_file_watcher,
        ])
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:narrative.db", migrations)
                .build(),
        )
        .setup(|app| {
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            // Create a separate sqlx pool for backend Rust operations
            // Use the same database as tauri_plugin_sql to avoid duplication
            let home = dirs::home_dir().ok_or_else(|| {
                eprintln!("Narrative: Failed to determine home directory. Please ensure HOME environment variable is set.");
                "Could not determine home directory. Please ensure HOME environment variable is set."
            })?;
            let path = home.join("Library/Application Support/com.jamie.narrative-mvp/narrative.db");

            // Use blocking connect since setup is not async
            let pool = tauri::async_runtime::block_on(async {
                // Create database if it doesn't exist, then connect
                let options = SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true);

                SqlitePool::connect_with(options)
                    .await
                    .map_err(|e| {
                        eprintln!("Narrative: Database connection failed: {}", e);
                        format!("Failed to connect to database: {}. Please check file permissions and disk space.", e)
                    })
            })?;

            app.manage(DbState(Arc::new(pool)));

            let otel_state = otlp_receiver::OtelReceiverState::default();
            app.manage(otel_state.clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|e| {
            eprintln!("Narrative: Failed to run Tauri application: {}", e);
            Box::new(e) as Box<dyn std::error::Error>
        })?;
    Ok(())
}
