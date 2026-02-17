pub mod dispatcher;
pub mod loader;
pub mod schema;

pub use dispatcher::{Action, KeybindingDispatcher};
pub use loader::{LoadError, default_config_path, load_keybindings};
pub use schema::KeybindingConfig;
