//! Persisted calibration settings (see `specs/app-flow.md`).
//!
//! The calibration offset only needs to be measured once per machine/audio
//! setup, not once per process launch — this module persists the measured
//! `offset_ms` to a small file in the OS config directory so subsequent
//! launches can skip straight to the main menu with modes unlocked.
//!
//! Deliberately minimal: a single `offset_ms: i64` field, hand-rolled
//! JSON-ish serialization instead of pulling in `serde`/`serde_json` for
//! one field — this is a hobby project (see `specs/vision.md`), not worth
//! a new dependency for.

use std::path::PathBuf;

/// Sub-path (below the OS config dir) where the calibration file lives.
const APP_DIR_NAME: &str = "quietmytempo";
const FILE_NAME: &str = "calibration.json";

/// Resolves the calibration file path without creating anything on disk.
/// Uses `dirs::config_dir()` (Windows: `%APPDATA%`, Linux:
/// `$XDG_CONFIG_HOME`/`~/.config`, macOS: `~/Library/Application Support`)
/// so this works the same way across platforms without hand-rolling OS
/// detection.
fn calibration_path() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push(APP_DIR_NAME);
    dir.push(FILE_NAME);
    Some(dir)
}

/// Loads a previously persisted calibration offset, if any. Returns `None`
/// if the file doesn't exist, can't be read, or its contents are not the
/// expected shape — any of these cases are treated the same way by the
/// caller (see `menu.rs`/`main.rs`): fall back to "not calibrated yet"
/// rather than erroring out the whole app over a corrupt settings file.
pub fn load_offset_ms() -> Option<i64> {
    let path = calibration_path()?;
    let contents = std::fs::read_to_string(path).ok()?;
    parse_offset_ms(&contents)
}

/// Persists `offset_ms` to disk, creating the config directory if needed.
/// Returns `Err` only for genuine I/O failures — callers should surface
/// this as a non-fatal warning (calibration still works for the current
/// process; it just won't be remembered next launch).
pub fn save_offset_ms(offset_ms: i64) -> anyhow::Result<()> {
    let path =
        calibration_path().ok_or_else(|| anyhow::anyhow!("no config directory available"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serialize_offset_ms(offset_ms))?;
    Ok(())
}

fn serialize_offset_ms(offset_ms: i64) -> String {
    format!("{{\"offset_ms\":{offset_ms}}}\n")
}

/// Extracts the `offset_ms` integer value from a small hand-rolled JSON
/// object of the shape `{"offset_ms": <int>}` (whitespace-tolerant). Not a
/// general JSON parser — deliberately only handles this one known shape
/// (see module docs for why we're not pulling in `serde_json`).
fn parse_offset_ms(contents: &str) -> Option<i64> {
    let key_pos = contents.find("\"offset_ms\"")?;
    let after_key = &contents[key_pos + "\"offset_ms\"".len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    let end = after_colon
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(after_colon.len());
    after_colon[..end].parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_positive_offset() {
        let serialized = serialize_offset_ms(42);
        assert_eq!(parse_offset_ms(&serialized), Some(42));
    }

    #[test]
    fn round_trips_negative_offset() {
        let serialized = serialize_offset_ms(-17);
        assert_eq!(parse_offset_ms(&serialized), Some(-17));
    }

    #[test]
    fn parses_offset_regardless_of_whitespace() {
        assert_eq!(parse_offset_ms("{ \"offset_ms\" :   7 }"), Some(7));
    }

    #[test]
    fn returns_none_for_garbage_content() {
        assert_eq!(parse_offset_ms("not json at all"), None);
    }
}
