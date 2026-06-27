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
    /// Which level a fresh run begins on: `1`..`11` (default `1`), or `"random"`.
    pub start_level: StartLevel,
}

impl Default for Config {
    fn default() -> Self {
        Self { fullscreen: true, width: 1280, height: 720, vsync: true, start_level: StartLevel::default() }
    }
}

/// The configured starting level. Accepts a bare integer (`start_level: 3`) or a
/// string (`start_level: "random"` / `start_level: "3"`) in `config.ron`. The
/// number is 1-based (level 1..11); it is range-clamped where it's consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartLevel {
    Random,
    Fixed(usize),
}

impl Default for StartLevel {
    fn default() -> Self {
        StartLevel::Fixed(1)
    }
}

impl<'de> Deserialize<'de> for StartLevel {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = StartLevel;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"a level number 1-11 or the string "random""#)
            }
            fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<StartLevel, E> {
                Ok(StartLevel::Fixed((n as usize).max(1)))
            }
            fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<StartLevel, E> {
                Ok(StartLevel::Fixed((n.max(1)) as usize))
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<StartLevel, E> {
                let t = s.trim();
                if t.eq_ignore_ascii_case("random") {
                    Ok(StartLevel::Random)
                } else if let Ok(n) = t.parse::<usize>() {
                    Ok(StartLevel::Fixed(n.max(1)))
                } else {
                    Err(E::custom(format!(r#"invalid start_level {s:?} (use 1-11 or "random")"#)))
                }
            }
        }
        d.deserialize_any(V)
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
