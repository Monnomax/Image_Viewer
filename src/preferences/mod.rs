// preferences/mod.rs — оголошує підмодулі розділу налаштувань і
// реекспортує публічний API (`Settings` та `show`), яким користується
// решта застосунку (app.rs, viewer/input.rs, viewer/menu.rs).
//
// Розділ "Анімації" (animations.rs) навмисно НЕ оголошений тут — він живе
// в crate::viewer, поруч із рендерером; dialog.rs імпортує його звідти.

pub mod auto_hide;
mod dialog;
mod general;
mod navigation;
pub(crate) mod regulator;
pub mod settings;
mod view;

pub use dialog::show;
pub use settings::Settings;
