//! Startup configuration read from `config.ron`.
//!
//! Edit `config.ron` in the game directory and restart. A missing file — or any
//! missing field — falls back to the defaults below (fullscreen on). RON is the
//! Bevy/Rust-idiomatic text config format; it supports `//` comments.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Fill the whole screen (borderless fullscreen) instead of a window.
    pub fullscreen: bool,
    /// Window size in physical pixels — used only when `fullscreen` is false.
    pub width: u32,
    pub height: u32,
    /// Cap the frame rate to the monitor's refresh rate (avoids tearing).
    pub vsync: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { fullscreen: true, width: 1280, height: 720, vsync: true }
    }
}

impl Config {
    /// Load `config.ron`, searching the working dir, the cargo manifest dir, and
    /// the directory next to the executable, in that order. Falls back to the
    /// defaults (and logs why) when the file is absent or unparseable. Called
    /// before the Bevy app starts, so it logs via `eprintln!` rather than `info!`.
    pub fn load() -> Self {
        let Some((path, text)) = Self::find() else {
            eprintln!("config: no config.ron found — using defaults (fullscreen)");
            return Self::default();
        };
        match ron::from_str::<Config>(&text) {
            Ok(cfg) => {
                eprintln!("config: loaded {path}");
                cfg
            }
            Err(e) => {
                eprintln!("config: failed to parse {path}: {e} — using defaults");
                Self::default()
            }
        }
    }

    fn find() -> Option<(String, String)> {
        let mut candidates = vec![std::path::PathBuf::from("config.ron")];
        if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
            candidates.push(std::path::Path::new(&dir).join("config.ron"));
        }
        if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
            candidates.push(dir.join("config.ron"));
        }
        candidates.into_iter().find_map(|c| {
            std::fs::read_to_string(&c).ok().map(|t| (c.to_string_lossy().into_owned(), t))
        })
    }
}
