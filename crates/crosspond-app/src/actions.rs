use gpui::{KeyBinding, actions};

actions!(app, [Quit, OpenSettings]);

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
    ]
}
