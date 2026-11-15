//! Button -> key mapping (config) and the thread that presses/releases keys.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result, bail};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

use crate::protocol::BUTTONS;

pub const DEFAULT_CONFIG: &str = r#"# ZoneTwo button mapping. Keys: a single character ("w", "1"), or one of
# tab, space, escape, enter, backspace, shift, control, alt, up, down, left, right, f1-f12.
# Use "" to leave a button unassigned.
[keys]
up = "w"
down = "s"
left = "a"
right = "d"
a = "1"
b = "2"
y = "3"
z = "4"
shift_up_right = "5"
shift_down_right = "6"
shift_up_left = "tab"
shift_down_left = "escape"
paddle_right = "space"
paddle_left = "f"
powerup_right = "m"
powerup_left = "7"
onoff_right = ""
onoff_left = ""
"#;

#[derive(serde::Deserialize)]
struct Config {
    keys: BTreeMap<String, String>,
}

/// Button name -> key name.
pub type Mapping = HashMap<&'static str, String>;

pub fn load_mapping(text: &str) -> Result<Mapping> {
    let config: Config = toml::from_str(text).context("invalid config")?;
    let mut mapping = Mapping::new();
    for (name, key) in config.keys {
        let Some((button, _)) = BUTTONS.iter().find(|(b, _)| *b == name) else {
            let known: Vec<_> = BUTTONS.iter().map(|(b, _)| *b).collect();
            bail!("unknown button `{name}` in config (known: {})", known.join(", "));
        };
        if !key.is_empty() {
            parse_key(&key).with_context(|| format!("button `{name}`"))?;
            mapping.insert(button, key.to_lowercase());
        }
    }
    Ok(mapping)
}

fn parse_key(name: &str) -> Result<Key> {
    let lower = name.to_lowercase();
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Ok(Key::Unicode(c));
    }
    Ok(match lower.as_str() {
        "tab" => Key::Tab,
        "space" => Key::Space,
        "escape" | "esc" => Key::Escape,
        "enter" | "return" => Key::Return,
        "backspace" => Key::Backspace,
        "shift" => Key::Shift,
        "control" | "ctrl" => Key::Control,
        "alt" | "option" => Key::Alt,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        _ => bail!("unknown key `{name}`"),
    })
}

pub enum Event {
    /// Keys a controller wants held right now (replaces its previous set).
    Hold { controller: String, keys: HashSet<String> },
    ReleaseAll,
}

/// Spawns the thread that owns the keyboard. Keys held by several controllers stay down
/// until the last one lets go.
pub fn spawn_key_thread() -> Result<(mpsc::Sender<Event>, thread::JoinHandle<()>)> {
    let (tx, rx) = mpsc::channel::<Event>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
    let handle = thread::spawn(move || {
        let mut enigo = match Enigo::new(&Settings::default()) {
            Ok(e) => {
                let _ = ready_tx.send(Ok(()));
                e
            }
            Err(e) => {
                let _ = ready_tx.send(Err(anyhow::anyhow!("can't send key presses: {e}")));
                return;
            }
        };
        let mut per_controller: HashMap<String, HashSet<String>> = HashMap::new();
        let mut held: HashSet<String> = HashSet::new();
        let mut send = |key: &str, dir: Direction| {
            if let Ok(k) = parse_key(key)
                && let Err(e) = enigo.key(k, dir)
            {
                eprintln!("key {key} {dir:?} failed: {e}");
            }
        };
        for event in rx {
            match event {
                Event::Hold { controller, keys } => {
                    per_controller.insert(controller, keys);
                }
                Event::ReleaseAll => per_controller.clear(),
            }
            let wanted: HashSet<String> = per_controller.values().flatten().cloned().collect();
            for k in held.difference(&wanted) {
                send(k, Direction::Release);
            }
            for k in wanted.difference(&held) {
                send(k, Direction::Press);
            }
            held = wanted;
        }
        for k in &held {
            send(k, Direction::Release);
        }
    });
    ready_rx.recv().context("key thread died")??;
    Ok((tx, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let m = load_mapping(DEFAULT_CONFIG).unwrap();
        assert_eq!(m["up"], "w");
        assert_eq!(m["paddle_left"], "f");
        assert_eq!(m["powerup_right"], "m");
        assert_eq!(m["powerup_left"], "7");
        assert!(!m.contains_key("onoff_left"));
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(load_mapping("[keys]\nfoo = \"w\"").is_err());
        assert!(load_mapping("[keys]\nup = \"hyperspace\"").is_err());
    }
}
