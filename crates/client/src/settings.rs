//! Player settings. Missing files use the content defaults. A corrupt or
//! out-of-range file is rejected whole and the screen shows the reset notice.
//! Values are never clamped on load.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use gemini_sim::content::Content;

#[derive(Clone, Debug, PartialEq, Resource)]
pub struct Prefs {
    pub sensitivity: f32,
    pub fov: f32,
    pub invert_y: bool,
    pub view_bob: bool,
    pub master: f32,
    pub music: f32,
    pub effects: f32,
    pub character: Option<String>,
    pub difficulty: Option<String>,
    /// Idempotency key of the pull this client has not finished watching. Empty means none.
    pub pull_key: String,
    /// True when `pull_key` was a ten-pull.
    pub pull_ten: bool,
    /// True once that pull's best card has flipped on this client. The server never stores this.
    pub pull_seen: bool,
    /// True when the last send of `pull_key` never got a reply. The next press reuses the key.
    pub pull_awaiting: bool,
}

pub enum Loaded {
    /// No file yet.
    Fresh(Prefs),
    Restored(Prefs),
    /// The file was rejected. `Prefs` are the content defaults.
    Rejected(Prefs),
}

impl Prefs {
    pub fn defaults(content: &Content) -> Self {
        let settings = &content.ui.settings;
        Self {
            sensitivity: settings.sensitivity,
            fov: settings.fov,
            invert_y: settings.invert_y,
            view_bob: settings.view_bob,
            master: settings.master,
            music: settings.music,
            effects: settings.effects,
            character: None,
            difficulty: None,
            pull_key: String::new(),
            pull_ten: false,
            pull_seen: false,
            pull_awaiting: false,
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "sensitivity = {}\nfov = {}\ninvert_y = {}\nview_bob = {}\nmaster = {}\nmusic = {}\neffects = {}\ncharacter = {}\ndifficulty = {}\npull_key = {}\npull_ten = {}\npull_seen = {}\npull_awaiting = {}\n",
            self.sensitivity,
            self.fov,
            self.invert_y,
            self.view_bob,
            self.master,
            self.music,
            self.effects,
            quote_opt(self.character.as_deref()),
            quote_opt(self.difficulty.as_deref()),
            quote_opt(Some(self.pull_key.as_str())),
            self.pull_ten,
            self.pull_seen,
            self.pull_awaiting,
        )
    }
}

fn quote_opt(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", value.replace('"', "")),
        None => "\"\"".into(),
    }
}

pub fn default_path(content: &Content) -> PathBuf {
    if let Ok(path) = std::env::var("GEMINI_SETTINGS") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    PathBuf::from(&content.ui.settings.settings_file)
}

pub fn load_path(path: &Path, content: &Content) -> Loaded {
    match fs::read_to_string(path) {
        Ok(text) => load_text(&text, content),
        Err(_) => Loaded::Fresh(Prefs::defaults(content)),
    }
}

pub fn load_text(text: &str, content: &Content) -> Loaded {
    match parse_prefs(text, content) {
        Ok(prefs) => Loaded::Restored(prefs),
        Err(_) => Loaded::Rejected(Prefs::defaults(content)),
    }
}

pub fn save_path(path: &Path, prefs: &Prefs) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
    }
    fs::write(path, prefs.encode()).map_err(|err| err.to_string())
}

/// `Err` means the whole file is rejected. An unknown remembered id is cleared
/// and does not reject the numeric settings.
pub fn parse_prefs(text: &str, content: &Content) -> Result<Prefs, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| err.to_string())?;
    let table = root.as_table().ok_or_else(|| "settings are not a table".to_string())?;
    let settings = &content.ui.settings;
    let mut prefs = Prefs::defaults(content);
    prefs.sensitivity = ranged(table, "sensitivity", settings.sensitivity_min, settings.sensitivity_max)?;
    prefs.fov = ranged(table, "fov", settings.fov_min, settings.fov_max)?;
    prefs.invert_y = flag(table, "invert_y")?;
    prefs.view_bob = flag(table, "view_bob")?;
    prefs.master = ranged(table, "master", settings.volume_min, settings.volume_max)?;
    prefs.music = ranged(table, "music", settings.volume_min, settings.volume_max)?;
    prefs.effects = ranged(table, "effects", settings.volume_min, settings.volume_max)?;
    prefs.character = remembered_hero(table, content)?;
    prefs.difficulty = remembered_tier(table, content)?;
    prefs.pull_key = required_text(table, "pull_key")?;
    prefs.pull_ten = flag(table, "pull_ten")?;
    prefs.pull_seen = flag(table, "pull_seen")?;
    prefs.pull_awaiting = flag(table, "pull_awaiting")?;
    Ok(prefs)
}

fn required_text(table: &toml::map::Map<String, toml::Value>, key: &str) -> Result<String, String> {
    table
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| format!("settings {key} is not a string"))
}

fn ranged(table: &toml::map::Map<String, toml::Value>, key: &str, min: f32, max: f32) -> Result<f32, String> {
    let value = table.get(key).ok_or_else(|| format!("settings missing {key}"))?;
    let number = value
        .as_float()
        .map(|n| n as f32)
        .or_else(|| value.as_integer().map(|n| n as f32))
        .ok_or_else(|| format!("settings {key} is not a number"))?;
    if !number.is_finite() || number < min || number > max {
        return Err(format!("settings {key} {number} is outside [{min}, {max}]"));
    }
    Ok(number)
}

fn flag(table: &toml::map::Map<String, toml::Value>, key: &str) -> Result<bool, String> {
    table
        .get(key)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| format!("settings {key} is not a bool"))
}

fn remembered_hero(table: &toml::map::Map<String, toml::Value>, content: &Content) -> Result<Option<String>, String> {
    let Some(value) = table.get("character") else {
        return Ok(None);
    };
    let id = value.as_str().ok_or_else(|| "settings character is not a string".to_string())?;
    if id.is_empty() {
        return Ok(None);
    }
    Ok(content.hero_selectable(id).then(|| id.to_string()))
}

fn remembered_tier(table: &toml::map::Map<String, toml::Value>, content: &Content) -> Result<Option<String>, String> {
    let Some(value) = table.get("difficulty") else {
        return Ok(None);
    };
    let id = value.as_str().ok_or_else(|| "settings difficulty is not a string".to_string())?;
    if id.is_empty() {
        return Ok(None);
    }
    let known = content.difficulty.iter().any(|tier| tier.id == id);
    Ok(known.then(|| id.to_string()))
}

#[cfg(target_arch = "wasm32")]
pub fn load_saved(content: &Content) -> Loaded {
    let key = content.ui.settings.settings_key.as_str();
    let text = web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(key).ok().flatten());
    match text {
        Some(text) => load_text(&text, content),
        None => Loaded::Fresh(Prefs::defaults(content)),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_saved(content: &Content) -> Loaded {
    load_path(&default_path(content), content)
}

#[cfg(target_arch = "wasm32")]
pub fn store(content: &Content, prefs: &Prefs) {
    let key = content.ui.settings.settings_key.as_str();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.set_item(key, &prefs.encode());
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn store(content: &Content, prefs: &Prefs) {
    let _ = save_path(&default_path(content), prefs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_reject_bad_values() {
        let content = Content::embedded();
        let mut prefs = Prefs::defaults(&content);
        prefs.sensitivity = 1.5;
        prefs.fov = 100.0;
        prefs.invert_y = true;
        prefs.view_bob = false;
        prefs.master = 0.5;
        prefs.music = 0.25;
        prefs.effects = 0.4;
        prefs.character = Some(content.hero.id.clone());
        prefs.difficulty = Some(content.ui.settings.default_difficulty.clone());
        let text = prefs.encode();
        let loaded = parse_prefs(&text, &content).expect("round trip");
        assert_eq!(loaded, prefs);

        let dir = std::env::temp_dir().join(format!("gemini-settings-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("settings.toml");
        save_path(&path, &prefs).expect("write");
        match load_path(&path, &content) {
            Loaded::Restored(back) => assert_eq!(back, prefs),
            _ => panic!("expected restored, got a different load"),
        }
        let _ = other_guard(&path);

        let high = text.replace("fov = 100", "fov = 200");
        assert!(parse_prefs(&high, &content).is_err(), "fov 200 must be rejected");
        match load_text(&high, &content) {
            Loaded::Rejected(back) => {
                assert_eq!(back.fov, content.ui.settings.fov);
                assert!(!back.invert_y);
            }
            _ => panic!("out of range must reject the file"),
        }
        let low = text.replace("master = 0.5", "master = -0.1");
        assert!(parse_prefs(&low, &content).is_err());
        assert!(parse_prefs("fov = 90", &content).is_err(), "missing keys");
        let missing_pull = text.replace("pull_key = \"\"\n", "");
        assert!(parse_prefs(&missing_pull, &content).is_err(), "pull_key is required");
        let mut unseen = prefs.clone();
        unseen.pull_key = "0123456789abcdef0123456789abcdef".into();
        unseen.pull_ten = true;
        unseen.pull_seen = false;
        unseen.pull_awaiting = true;
        let loaded = parse_prefs(&unseen.encode(), &content).expect("unseen pull round trip");
        assert_eq!(loaded.pull_key, unseen.pull_key);
        assert!(loaded.pull_ten);
        assert!(!loaded.pull_seen);
        assert!(loaded.pull_awaiting);
        assert!(parse_prefs("not toml", &content).is_err());

        let unknown_hero = text.replace(&format!("character = \"{}\"", content.hero.id), "character = \"nope\"");
        let cleared = parse_prefs(&unknown_hero, &content).expect("bad id clears memory");
        assert!(cleared.character.is_none());
        assert_eq!(cleared.fov, prefs.fov);
        let unknown_tier = text.replace(
            &format!("difficulty = \"{}\"", content.ui.settings.default_difficulty),
            "difficulty = \"nightmare\"",
        );
        let cleared = parse_prefs(&unknown_tier, &content).expect("bad tier");
        assert!(cleared.difficulty.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    fn other_guard(path: &std::path::Path) -> bool {
        path.is_file()
    }
}
