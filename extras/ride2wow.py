#!/usr/bin/env python3
"""Zwift Ride controllers -> keyboard keys (for WoW). Replaces BikeControl.

Run:   .venv/bin/python ride2wow.py           (normal)
       .venv/bin/python ride2wow.py --debug   (print raw button data)

Needs: BikeControl quit, Zwift NOT paired to the Ride controls,
       Bluetooth + Accessibility permission for your terminal app.
Protocol taken from BikeControl's open-source code (jonasbark/swiftcontrol).
"""
import asyncio
import signal
import sys

from bleak import BleakClient, BleakScanner
import Quartz

DEBUG = "--debug" in sys.argv

ZWIFT_MANUFACTURER_ID = 0x094A
RIDE_TYPES = {0x07: "Ride right", 0x08: "Ride left"}
RIDE_SERVICE = "0000fc82-0000-1000-8000-00805f9b34fb"
OLD_SERVICE = "00000001-19ca-4651-86e5-fa29dcdd09d1"
ASYNC_CHAR = "00000002-19ca-4651-86e5-fa29dcdd09d1"
SYNC_RX_CHAR = "00000003-19ca-4651-86e5-fa29dcdd09d1"
SYNC_TX_CHAR = "00000004-19ca-4651-86e5-fa29dcdd09d1"
RIDE_ON = b"RideOn"
RIDE_NOTIFICATION = 0x23
PADDLE_THRESHOLD = 25

# macOS virtual key codes
KEY = {"W": 13, "S": 1, "A": 0, "D": 2, "F": 3, "1": 18, "2": 19, "3": 20, "4": 21,
       "5": 23, "6": 22, "7": 26, "8": 28, "TAB": 48, "SPACE": 49, "ESC": 53}

# button bit (0 = pressed) -> key. Same layout as the ZoneTwo addon.
BUTTONS = {
    0x00002: ("Up", "W"),
    0x00008: ("Down", "S"),
    0x00001: ("Left", "A"),
    0x00004: ("Right", "D"),
    0x00010: ("A", "1"),
    0x00020: ("B", "2"),
    0x00040: ("Y", "3"),
    0x00080: ("Z", "4"),
    0x01000: ("R shift up", "5"),
    0x02000: ("R shift down", "6"),
    0x00100: ("L shift up", "TAB"),
    0x00200: ("L shift down", "F"),
    0x04000: ("R power-up", "7"),
    0x00400: ("L power-up", "8"),
    # 0x00800 / 0x08000 on-off buttons: unassigned
}
PADDLES = {0: ("L paddle", "ESC"), 1: ("R paddle", "SPACE")}

held: set[str] = set()  # keys currently held down by us (shared by both controllers)


def post_key(name: str, down: bool) -> None:
    ev = Quartz.CGEventCreateKeyboardEvent(None, KEY[name], down)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, ev)


def set_keys(source: dict, wanted: set[str]) -> None:
    """Make keys held by this controller match `wanted`."""
    before = source.setdefault("keys", set())
    for k in wanted - before:
        if k not in held:
            post_key(k, True)
        held.add(k)
    for k in before - wanted:
        if not any(k in s.get("keys", ()) for s in controllers.values() if s is not source):
            post_key(k, False)
            held.discard(k)
    source["keys"] = set(wanted)


def release_all() -> None:
    for k in list(held):
        post_key(k, False)
    held.clear()
    for s in controllers.values():
        s["keys"] = set()


def read_varint(buf: bytes, i: int) -> tuple[int, int]:
    shift = val = 0
    while True:
        b = buf[i]
        i += 1
        val |= (b & 0x7F) << shift
        shift += 7
        if not b & 0x80:
            return val, i


def parse_status(buf: bytes) -> tuple[int | None, dict[int, int]]:
    """RideKeyPadStatus: field 1 = buttonMap, field 3 = repeated {1: location, 2: value}."""
    button_map, paddles, i = None, {}, 0
    while i < len(buf):
        tag, i = read_varint(buf, i)
        field, wire = tag >> 3, tag & 7
        if wire == 0:
            val, i = read_varint(buf, i)
            if field == 1:
                button_map = val
        elif wire == 2:
            ln, i = read_varint(buf, i)
            sub, i = buf[i:i + ln], i + ln
            if field == 3:
                loc, raw, j = 0, 0, 0
                while j < len(sub):
                    t, j = read_varint(sub, j)
                    v, j = read_varint(sub, j) if t & 7 == 0 else (0, len(sub))
                    if t >> 3 == 1:
                        loc = v
                    elif t >> 3 == 2:
                        raw = v
                # int32 negative -> huge varint; sint32 -> zigzag. Both: magnitude check works.
                mag = (1 << 64) - raw if raw >= 1 << 63 else ((1 << 32) - raw if raw >= 1 << 31 else raw)
                paddles[loc] = mag
        elif wire == 5:
            i += 4
        elif wire == 1:
            i += 8
        else:
            break
    return button_map, paddles


controllers: dict[str, dict] = {}


def make_handler(name: str):
    state = controllers.setdefault(name, {"keys": set()})

    def on_data(_char, data: bytearray) -> None:
        data = bytes(data)
        if DEBUG:
            print(f"[{name}] {data.hex(' ')}")
        if not data or data[0] != RIDE_NOTIFICATION:
            return
        try:
            button_map, paddles = parse_status(data[1:])
        except IndexError:
            return
        if button_map is None:
            return
        wanted, labels = set(), []
        for bit, (label, key) in BUTTONS.items():
            if button_map & bit == 0:
                wanted.add(key)
                labels.append(label)
        for loc, mag in paddles.items():
            if loc in PADDLES and mag >= PADDLE_THRESHOLD:
                label, key = PADDLES[loc]
                wanted.add(key)
                labels.append(label)
        if DEBUG or labels:
            print(f"[{name}] pressed: {', '.join(labels) or '-'}  ->  {' '.join(sorted(wanted)) or '-'}")
        set_keys(state, wanted)

    return on_data


async def run_controller(address: str, name: str) -> None:
    while True:
        try:
            print(f"Connecting to {name} ...")
            async with BleakClient(address, timeout=15) as client:
                uuids = {s.uuid.lower() for s in client.services}
                if RIDE_SERVICE not in uuids and OLD_SERVICE not in uuids:
                    print(f"[{name}] Zwift service is hidden: this is probably a locked Ride V2 "
                          f"(firmware 1.3+). It can't be used without BikeControl's unlock.")
                    return
                handler = make_handler(name)
                await client.start_notify(ASYNC_CHAR, handler)
                await client.start_notify(SYNC_TX_CHAR, handler)
                await client.write_gatt_char(SYNC_RX_CHAR, RIDE_ON, response=False)
                print(f"[{name}] ready. Click into WoW and play.")
                while client.is_connected:
                    await asyncio.sleep(1)
        except Exception as e:  # noqa: BLE001
            print(f"[{name}] {type(e).__name__}: {e}")
        set_keys(controllers.setdefault(name, {"keys": set()}), set())
        print(f"[{name}] disconnected, retrying in 3 s (press a button to wake it)")
        await asyncio.sleep(3)


async def find_rides(timeout: float = 10.0) -> dict[str, str]:
    print("Scanning for Zwift Ride controllers (press a button to wake them) ...")
    found = await BleakScanner.discover(timeout=timeout, return_adv=True)
    rides = {}
    for address, (dev, adv) in found.items():
        data = adv.manufacturer_data.get(ZWIFT_MANUFACTURER_ID)
        if data and data[0] in RIDE_TYPES:
            rides[address] = RIDE_TYPES[data[0]]
            print(f"  found {RIDE_TYPES[data[0]]} ({dev.name}, {address})")
        elif data and DEBUG:
            print(f"  other Zwift device type 0x{data[0]:02x} ({dev.name})")
    return rides


async def main() -> None:
    rides = {}
    while not rides:
        rides = await find_rides()
        if not rides:
            print("None found. Is BikeControl quit, and Zwift not connected to the controls? Retrying ...")
    await asyncio.gather(*(run_controller(a, n) for a, n in rides.items()))


if __name__ == "__main__":
    loop = asyncio.new_event_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, loop.stop)
    try:
        loop.run_until_complete(main())
    except RuntimeError:
        pass  # loop stopped by Ctrl+C
    finally:
        release_all()
        print("\nStopped, all keys released.")
