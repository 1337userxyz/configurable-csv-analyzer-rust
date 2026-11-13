#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::rc::Rc;

use configurable_csv_analyzer::core::color_cache::get_global_color_cache;
use configurable_csv_analyzer::core::io::info::{
    default_config_path, default_views_path, load_app_config, load_views_file,
};
use configurable_csv_analyzer::core::logging::log_info;

mod controllers;
mod dialogs;
mod state;
mod ui_sync;

pub use state::AppState;
pub use ui_sync::sync_ui;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    kit_picker::ensure_dialog_ready();
    let app_config = load_app_config(&default_config_path()).unwrap_or_default();
    if let Ok(mut cache) = get_global_color_cache().lock() {
        cache.update_from_brushes(&app_config.brushes);
    }
    let views_config = load_views_file(&default_views_path()).unwrap_or_default();

    let mut initial_state = AppState::new();
    initial_state.app_config = app_config;
    initial_state.views_config = views_config;

    log_info("App", "Configurable CSV & ODS Analyzer gestartet.");

    // Check if there is a default view (or first view) and auto-load its files on startup
    if let Some(default_idx) = initial_state
        .views_config
        .views
        .iter()
        .position(|v| v.is_default_view)
        .or_else(|| {
            if !initial_state.views_config.views.is_empty() {
                Some(0)
            } else {
                None
            }
        })
    {
        initial_state.activate_and_load_view(default_idx);
    }

    let main_window = MainWindow::new()?;
    let config_window = Rc::new(ConfigurationWindow::new()?);
    let info_editor_window = Rc::new(InfoFileEditorWindow::new()?);
    let state = Rc::new(RefCell::new(initial_state));

    // Setup dialogs
    dialogs::configuration::setup_configuration_window(
        config_window.clone(),
        info_editor_window.clone(),
        main_window.as_weak(),
        state.clone(),
    );

    dialogs::info_editor::setup_info_editor_window(
        info_editor_window.clone(),
        main_window.as_weak(),
        state.clone(),
    );

    // Register all controllers
    controllers::register_all(&main_window, state.clone(), config_window.clone(), info_editor_window.clone());

    ui_sync::apply_theme_all(
        Some(&main_window),
        Some(&config_window),
        Some(&info_editor_window),
        state.borrow().app_config.theme,
    );
    sync_ui(&main_window, &state.borrow());
    main_window.run()
}
