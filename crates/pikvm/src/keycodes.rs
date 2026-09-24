//! Mapping between Linux evdev key codes and the browser `KeyboardEvent.code`
//! names (`web_name` in kvmd's `keymap.csv`) that PiKVM expects in `key` events.
//!
//! Generated from <https://github.com/pikvm/kvmd/blob/master/keymap.csv> and
//! `<linux/input-event-codes.h>`. GTK hardware keycodes on X11/Wayland are `evdev + 8`.

/// `(evdev code, web name)` pairs for every key PiKVM knows.
pub const KEYMAP: &[(u16, &str)] = &[
    (30, "KeyA"),
    (48, "KeyB"),
    (46, "KeyC"),
    (32, "KeyD"),
    (18, "KeyE"),
    (33, "KeyF"),
    (34, "KeyG"),
    (35, "KeyH"),
    (23, "KeyI"),
    (36, "KeyJ"),
    (37, "KeyK"),
    (38, "KeyL"),
    (50, "KeyM"),
    (49, "KeyN"),
    (24, "KeyO"),
    (25, "KeyP"),
    (16, "KeyQ"),
    (19, "KeyR"),
    (31, "KeyS"),
    (20, "KeyT"),
    (22, "KeyU"),
    (47, "KeyV"),
    (17, "KeyW"),
    (45, "KeyX"),
    (21, "KeyY"),
    (44, "KeyZ"),
    (2, "Digit1"),
    (3, "Digit2"),
    (4, "Digit3"),
    (5, "Digit4"),
    (6, "Digit5"),
    (7, "Digit6"),
    (8, "Digit7"),
    (9, "Digit8"),
    (10, "Digit9"),
    (11, "Digit0"),
    (28, "Enter"),
    (1, "Escape"),
    (14, "Backspace"),
    (15, "Tab"),
    (57, "Space"),
    (12, "Minus"),
    (13, "Equal"),
    (26, "BracketLeft"),
    (27, "BracketRight"),
    (43, "Backslash"),
    (39, "Semicolon"),
    (40, "Quote"),
    (41, "Backquote"),
    (51, "Comma"),
    (52, "Period"),
    (53, "Slash"),
    (58, "CapsLock"),
    (59, "F1"),
    (60, "F2"),
    (61, "F3"),
    (62, "F4"),
    (63, "F5"),
    (64, "F6"),
    (65, "F7"),
    (66, "F8"),
    (67, "F9"),
    (68, "F10"),
    (87, "F11"),
    (88, "F12"),
    (99, "PrintScreen"),
    (110, "Insert"),
    (102, "Home"),
    (104, "PageUp"),
    (111, "Delete"),
    (107, "End"),
    (109, "PageDown"),
    (106, "ArrowRight"),
    (105, "ArrowLeft"),
    (108, "ArrowDown"),
    (103, "ArrowUp"),
    (29, "ControlLeft"),
    (42, "ShiftLeft"),
    (56, "AltLeft"),
    (125, "MetaLeft"),
    (97, "ControlRight"),
    (54, "ShiftRight"),
    (100, "AltRight"),
    (126, "MetaRight"),
    (119, "Pause"),
    (70, "ScrollLock"),
    (69, "NumLock"),
    (438, "ContextMenu"),
    (98, "NumpadDivide"),
    (55, "NumpadMultiply"),
    (74, "NumpadSubtract"),
    (78, "NumpadAdd"),
    (96, "NumpadEnter"),
    (79, "Numpad1"),
    (80, "Numpad2"),
    (81, "Numpad3"),
    (75, "Numpad4"),
    (76, "Numpad5"),
    (77, "Numpad6"),
    (71, "Numpad7"),
    (72, "Numpad8"),
    (73, "Numpad9"),
    (82, "Numpad0"),
    (83, "NumpadDecimal"),
    (116, "Power"),
    (86, "IntlBackslash"),
    (124, "IntlYen"),
    (89, "IntlRo"),
    (90, "KanaMode"),
    (92, "Convert"),
    (94, "NonConvert"),
    (113, "AudioVolumeMute"),
    (115, "AudioVolumeUp"),
    (114, "AudioVolumeDown"),
    (190, "F20"),
    (191, "F21"),
    (192, "F22"),
    (193, "F23"),
    (194, "F24"),
    (183, "F13"),
    (184, "F14"),
    (185, "F15"),
    (186, "F16"),
    (187, "F17"),
    (188, "F18"),
    (189, "F19"),
];

/// Offset between GTK hardware keycodes and evdev codes on Linux.
pub const GTK_KEYCODE_OFFSET: u16 = 8;

/// Translate an evdev key code into PiKVM's web key name.
pub fn web_name_for_evdev(code: u16) -> Option<&'static str> {
    KEYMAP.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// Translate a GTK hardware keycode (as delivered by `EventControllerKey`) into a web key name.
pub fn web_name_for_gtk_keycode(keycode: u32) -> Option<&'static str> {
    let code = keycode.checked_sub(u32::from(GTK_KEYCODE_OFFSET))?;
    u16::try_from(code).ok().and_then(web_name_for_evdev)
}

/// Returns `true` when the web key name is a modifier key.
pub fn is_modifier(web_name: &str) -> bool {
    matches!(
        web_name,
        "ShiftLeft" | "ShiftRight" | "ControlLeft" | "ControlRight" | "AltLeft" | "AltRight" | "MetaLeft" | "MetaRight"
    )
}

/// All known web key names, useful for shortcut pickers.
pub fn all_web_names() -> impl Iterator<Item = &'static str> {
    KEYMAP.iter().map(|(_, n)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_and_specials() {
        assert_eq!(web_name_for_evdev(30), Some("KeyA"));
        assert_eq!(web_name_for_evdev(28), Some("Enter"));
        assert_eq!(web_name_for_gtk_keycode(38), Some("KeyA"));
        assert_eq!(web_name_for_gtk_keycode(0), None);
    }

    #[test]
    fn modifiers_are_detected() {
        assert!(is_modifier("ControlLeft"));
        assert!(!is_modifier("KeyA"));
    }
}
