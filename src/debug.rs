use std::env;

pub fn set_debug(_enabled: bool) {
    // deprecated, use env var
}

pub fn debug_enabled() -> bool {
    env::var("LANG_DEBUG").map(|v| v == "1").unwrap_or(false)
}
