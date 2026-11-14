//! zonetwo: use Zwift Ride controllers as a keyboard (made for World of Warcraft).

mod keys;
mod protocol;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use anyhow::{Context, Result};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Manager, Peripheral, PeripheralId};
use futures::StreamExt;

use keys::{DEFAULT_CONFIG, Event, Mapping};
use protocol::*;

const HELP: &str = "zonetwo: use Zwift Ride controllers as keyboard keys (for WoW)

USAGE: zonetwo [--config FILE] [--debug] [--print-config]

  --config FILE    button mapping (default: ./zonetwo.toml, then next to the binary, then built-in)
  --debug          print raw controller data
  --print-config   print the built-in mapping (save it as zonetwo.toml to customise)

Quit BikeControl/Zwift's connection to the controllers first. Ctrl+C to stop.";

struct Args {
    config: Option<PathBuf>,
    debug: bool,
}

fn parse_args() -> Result<Option<Args>> {
    let mut args = Args { config: None, debug: false };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => args.config = Some(it.next().context("--config needs a file")?.into()),
            "--debug" => args.debug = true,
            "--print-config" => {
                print!("{DEFAULT_CONFIG}");
                return Ok(None);
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("zonetwo {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            other => anyhow::bail!("unknown argument `{other}`\n\n{HELP}"),
        }
    }
    Ok(Some(args))
}

fn load_config(explicit: Option<PathBuf>) -> Result<Mapping> {
    let candidates = match explicit {
        Some(p) => vec![p],
        None => {
            let mut c = vec![PathBuf::from("zonetwo.toml")];
            if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
                c.push(dir.join("zonetwo.toml"));
            }
            c
        }
    };
    for path in &candidates {
        if path.exists() {
            let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            println!("Using mapping from {}", path.display());
            return keys::load_mapping(&text).with_context(|| path.display().to_string());
        }
    }
    keys::load_mapping(DEFAULT_CONFIG)
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let Some(args) = parse_args()? else { return Ok(()) };
    let mapping = Arc::new(load_config(args.config)?);
    let (tx, key_thread) = keys::spawn_key_thread()?;

    let result = tokio::select! {
        r = scan_and_connect(mapping, tx.clone(), args.debug) => r,
        _ = tokio::signal::ctrl_c() => Ok(()),
    };

    let _ = tx.send(Event::ReleaseAll);
    drop(tx);
    let _ = key_thread.join();
    println!("\nStopped, all keys released.");
    result
}

async fn scan_and_connect(mapping: Arc<Mapping>, tx: mpsc::Sender<Event>, debug: bool) -> Result<()> {
    let manager = Manager::new().await.context("Bluetooth unavailable (is it on, and allowed for this terminal?)")?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .context("no Bluetooth adapter found")?;
    adapter.start_scan(ScanFilter::default()).await.context("can't scan for Bluetooth devices")?;
    println!("Scanning for Zwift Ride controllers... press a button on each one to wake it.");

    let active: Arc<Mutex<HashSet<PeripheralId>>> = Arc::default();
    let mut hinted = false;
    for round in 0u32.. {
        for p in adapter.peripherals().await? {
            let id = p.id();
            if active.lock().unwrap().contains(&id) {
                continue;
            }
            let Ok(Some(props)) = p.properties().await else { continue };
            let side = match props.manufacturer_data.get(&ZWIFT_MANUFACTURER_ID).and_then(|d| d.first()) {
                Some(&RIDE_LEFT) => "Ride left",
                Some(&RIDE_RIGHT) => "Ride right",
                Some(other) => {
                    if debug {
                        println!("ignoring other Zwift device type 0x{other:02x}");
                    }
                    continue;
                }
                None => continue,
            };
            active.lock().unwrap().insert(id.clone());
            let (mapping, tx, active) = (mapping.clone(), tx.clone(), active.clone());
            tokio::spawn(async move {
                if let Err(e) = run_controller(&p, side, &mapping, &tx, debug).await {
                    println!("[{side}] {e:#}");
                }
                let _ = tx.send(Event::Hold { controller: side.into(), keys: HashSet::new() });
                let _ = p.disconnect().await;
                println!("[{side}] disconnected. Press a button on it to reconnect.");
                tokio::time::sleep(Duration::from_secs(3)).await;
                active.lock().unwrap().remove(&id);
            });
        }
        if round == 10 && active.lock().unwrap().is_empty() && !hinted {
            hinted = true;
            println!("No controllers yet. Make sure BikeControl is quit and Zwift isn't paired to the Ride controls.");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Ok(())
}

async fn run_controller(p: &Peripheral, side: &str, mapping: &Mapping, tx: &mpsc::Sender<Event>, debug: bool) -> Result<()> {
    println!("[{side}] connecting...");
    if !p.is_connected().await? {
        tokio::time::timeout(Duration::from_secs(15), p.connect()).await.context("connect timed out")??;
    }
    p.discover_services().await?;
    let services: HashSet<_> = p.services().iter().map(|s| s.uuid).collect();
    if !services.contains(&RIDE_SERVICE) && !services.contains(&LEGACY_SERVICE) {
        anyhow::bail!(
            "Zwift service is hidden. This is probably a locked Zwift Ride V2 (firmware 1.3+), which only works after unlocking it in Zwift/BikeControl."
        );
    }
    let chars = p.characteristics();
    let find = |uuid| chars.iter().find(|c| c.uuid == uuid).cloned().context("controller is missing a Zwift characteristic");
    let (async_c, sync_tx, sync_rx) = (find(ASYNC_CHAR)?, find(SYNC_TX_CHAR)?, find(SYNC_RX_CHAR)?);

    let mut notifications = p.notifications().await?;
    p.subscribe(&async_c).await?;
    p.subscribe(&sync_tx).await?;
    p.write(&sync_rx, RIDE_ON, WriteType::WithoutResponse).await?;
    println!("[{side}] ready. Click into the game window and play.");

    let mut last: Option<Vec<&str>> = None;
    let mut check = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            n = notifications.next() => {
                let Some(n) = n else { return Ok(()) };
                if debug {
                    let hex: Vec<_> = n.value.iter().map(|b| format!("{b:02x}")).collect();
                    println!("[{side}] {}", hex.join(" "));
                }
                let Some(pressed) = pressed_buttons(&n.value) else { continue };
                if last.as_ref() != Some(&pressed) {
                    let keys: HashSet<String> = pressed.iter().filter_map(|b| mapping.get(b).cloned()).collect();
                    if !pressed.is_empty() {
                        let mut k: Vec<_> = keys.iter().map(String::as_str).collect();
                        k.sort();
                        println!("[{side}] {} -> {}", pressed.join(" + "), if k.is_empty() { "(unassigned)".into() } else { k.join(" ") });
                    }
                    tx.send(Event::Hold { controller: side.into(), keys })?;
                    last = Some(pressed);
                }
            }
            _ = check.tick() => {
                if !p.is_connected().await.unwrap_or(false) {
                    return Ok(());
                }
            }
        }
    }
}
