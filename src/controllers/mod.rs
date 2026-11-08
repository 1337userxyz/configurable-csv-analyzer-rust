use std::cell::RefCell;
use std::rc::Rc;

use crate::{AppState, ConfigurationWindow, InfoFileEditorWindow, MainWindow};

pub mod dialogs;
pub mod file;
pub mod filter;
pub mod table;
pub mod tabs_views;
pub mod transform;

pub fn register_all(
    main_window: &MainWindow,
    state: Rc<RefCell<AppState>>,
    config_window: Rc<ConfigurationWindow>,
    info_editor_window: Rc<InfoFileEditorWindow>,
) {
    file::register(main_window, state.clone());
    tabs_views::register(main_window, state.clone());
    table::register(main_window, state.clone());
    filter::register(main_window, state.clone());
    transform::register(main_window, state.clone());
    dialogs::register(main_window, state, config_window, info_editor_window);
}
