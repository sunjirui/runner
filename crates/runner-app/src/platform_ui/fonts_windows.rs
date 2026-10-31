pub const SYSTEM_MONOSPACE_FONT: &str = "Consolas";
pub const UI_MONOSPACE_FONT: &str = "JetBrainsMono Nerd Font Mono";
pub const APP_FONT_FALLBACKS: &[&str] = &["Segoe UI", "Microsoft YaHei"];
pub const TERMINAL_FONT_FALLBACKS: &[&str] = &["Microsoft YaHei", "Segoe UI"];
// DirectWrite's system fallback serves symbols such as U+23FA from the color
// Segoe UI Emoji; Segoe UI Symbol has monochrome glyphs for them (#1064).
pub const TERMINAL_SYMBOL_FALLBACKS: &[&str] = &["Segoe UI Symbol"];
