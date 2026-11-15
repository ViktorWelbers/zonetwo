# ZoneTwo

Play World of Warcraft with your **Zwift Ride** controllers during an easy zone-2 ride.

The project has two parts:

- **`zonetwo`** is a small program that connects to the Ride controllers over Bluetooth and turns button presses into keyboard keys. A key stays held for as long as you hold the button.
- **`ZoneTwo`** is a WoW addon that binds those keys and turns on soft targeting and the interact key, so you can play without a mouse. It also shows a cheat-sheet of the buttons on screen.

Your trainer keeps working in Zwift (or any other app) in ERG mode or at a fixed gear. Only the controllers are used for WoW.

## Install

1. Download from [Releases](../../releases):
   - `zonetwo-<platform>.zip`, the program for your computer
   - `ZoneTwo-addon.zip`, the WoW addon
2. Unzip the addon into your WoW `Interface/AddOns/` folder, for example `World of Warcraft/_retail_/Interface/AddOns/ZoneTwo`.
3. Unzip the program anywhere.
   - **macOS:** the binary isn't signed. Allow it once with `xattr -d com.apple.quarantine zonetwo`.

## Use

1. Make sure no other app is connected to the controllers. Quit BikeControl, and in Zwift pair only your trainer, **not** the Ride controls.
2. Run the program from a terminal:
   ```
   ./zonetwo          # macOS / Linux
   zonetwo.exe        # Windows
   ```
   Then press a button on each controller to wake it. You should see `[Ride left] ready` and `[Ride right] ready`.
3. **macOS:** allow Bluetooth for your terminal app when asked. Also turn your terminal app on in System Settings → Privacy & Security → **Accessibility**, otherwise key presses are blocked.
4. In WoW, enable **ZoneTwo** on the AddOns screen. Click into the game window and type `/zt apply`.
5. Ctrl+C in the terminal stops the program and releases every key.

## Default layout

| Ride button | Key | WoW action |
|---|---|---|
| ↑ / ↓ | `W` / `S` | move forward / back |
| ← / → | `A` / `D` | turn left / right |
| A / B / Y / Z | `1`–`4` | action bar 1–4 |
| Right shift + / − | `5` / `6` | action bar 5–6 |
| Left shift + | `Tab` | target nearest enemy |
| Left shift − | `Esc` | clear target / close windows |
| Left paddle | `F` | interact (loot, NPCs, quest objects) |
| Right paddle | `Space` | jump |
| Right power button | `M` | open / close the world map |
| Left power button | – | unassigned |
| Right / left power-up | `7` / `8` | action bar 7 / 8 (mount, heal) |

### Custom layout

Run `zonetwo --print-config > zonetwo.toml` and edit the file. The program looks for `zonetwo.toml` in the current folder first, then next to the binary. You can also point to a file with `--config path`. If you change the keys, rebind them in WoW to match.

## Addon commands

| Command | What it does |
|---|---|
| `/zt apply` | Binds the keys and turns on soft targeting, interact and auto loot. The first run saves a snapshot of your current settings. |
| `/zt restore` | Puts your original bindings and settings back. |
| `/zt autorun` | ↑ toggles autorun instead of holding forward. |
| `/zt hud` | Shows or hides the button cheat-sheet (drag it to move it). |

## Troubleshooting

- **No controllers found:** another app is still connected to them. Quit BikeControl and unpair the Ride controls in Zwift.
- **"Zwift service is hidden":** newer Zwift Ride V2 controllers (firmware 1.3 and later) are locked to Zwift and won't work.
- **The terminal shows presses but the game doesn't react:** the Accessibility permission is missing on macOS, or the game window doesn't have focus.
- **Linux:** sending keys works on X11/XWayland. Native Wayland isn't supported.
- `zonetwo --debug` prints the raw controller data.

## Build from source

```
cargo build --release      # binary in target/release/zonetwo
cargo test
```

Pushing a `v*` tag builds the binaries for macOS (arm64/x64), Windows and Linux, plus the addon zip, and attaches them to a GitHub release.

`extras/` has the original Python prototype and a BikeControl keymap profile, for anyone who'd rather use BikeControl.

## Credits

The Zwift Ride Bluetooth protocol comes from [BikeControl](https://github.com/jonasbark/swiftcontrol) by jonasbark. If you want a polished app with support for many trainer apps, use and support BikeControl.

Not affiliated with Zwift or Blizzard.
