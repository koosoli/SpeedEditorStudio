# Speed Editor Studio

Your own key bindings for the Blackmagic **DaVinci Resolve Speed Editor**, working inside DaVinci
Resolve, while every key and the jog wheel keep their native behaviour.

![Speed Editor Studio](docs/images/studio.png)

DaVinci Resolve opens the Speed Editor exclusively, so other tools can't add anything to it
while Resolve runs. Speed Editor Studio takes over the device instead and does what Resolve
would do. The wheel is calibrated against USB captures of Resolve driving a real Speed Editor,
and native key gestures use the shortcuts from *your* Resolve keyboard layout. On top of
that, you add your own pages, an Fn layer and Stream Deck-style actions. **You never have to
change a shortcut in Resolve.**

## Features

- **Native behaviour, measured.** JOG, SHTL and SCRL match Resolve's own feel: speed, shuttle
  curve, wheel modes and LEDs. Native gestures (press, double press, press-and-hold + wheel)
  work as in the official manual: trim in/out, roll, slip, slide, swap clips, audio level,
  split, ripple delete and more.
- **Three pages on CUT, DIS and SMTH CUT.** CUT is the "DaVinci" page: everything native,
  with any key you choose replaced by your own action. DIS and SMTH CUT are free pages. The
  active page's LED blinks.
- **Normal mode.** CUT + SMTH CUT switches every key back to its plain native function (the
  three LEDs flash to confirm). Press the chord again to return to your pages.
- **Fn layer.** Hold CUT and press another key: each page has its own second layer.
  Examples in the default profile: ripple start/end to playhead, trim start/end, select
  clips forward/backward on all tracks.
- **Track targeting.** CUT + CAM n makes only video track n + audio track n take part in
  edits, until CUT + CAM 9 turns all tracks back on. The CAM key stays lit.
- **Move a clip frame by frame.** Hold CUT + SPLIT and turn the wheel. SPLIT + wheel on its
  own swaps clips, one clip per touch.
- **Stream Deck-style keys.** Each key gets a label, a press action and a long-press action:
  a shortcut, a Resolve function, a Resolve menu item, a Resolve script command, an app or
  URL to open, or text to type.
- **Resolve menus without shortcuts.** Functions that have no usable shortcut are run through
  Resolve's own menus (Windows UI Automation), so they work on any keyboard layout.
- **Live mirror.** The app shows the device's LEDs, flashes the keys you press, turns the
  on-screen wheel and shows the battery level.
- **USB and Bluetooth.**
- **Profiles.** Save, export and import as plain TOML. See
  [`profiles/example.toml`](profiles/example.toml).

![The device view](docs/images/device.png)

Keys show their secondary function in brackets, as printed on the device. "DaVinci" means the
key does its native job. "—" means Resolve has no keyboard command for it, so the key is free
for your own mapping.

## Requirements

- Windows 10 or 11.
- DaVinci Resolve (developed and measured with Resolve Studio 21.1). For the script actions,
  set *Preferences › System › General › External scripting using* to **Local**.
- A DaVinci Resolve Speed Editor, over USB or paired over Bluetooth.

## Build and run

You need [Rust](https://rustup.rs/) (stable) and the WebView2 runtime, which Windows 11 has
built in.

```bash
cargo build --release
```

Then start `target\release\se-studio.exe`. Turn on **Connect** and **Mappings active**.

- **Give to Resolve** hands the device back to Resolve's own driver whenever Resolve runs.
  It's off by default, because then your mappings can't work inside Resolve.
- Your profile is saved to `%APPDATA%\OpenSpeedEditor\profile.toml`.

There is also a command-line runner without a window:

```bash
cargo run --release -p se-mapper -- profiles/example.toml
```

Setting the environment variable `OPEN_SPEED_EDITOR_LOG` to a file path makes the app log
every wheel report and every action it sends. That is useful when something doesn't reach
Resolve.

## How it works

| Part | What it does |
| --- | --- |
| [`crates/speed-editor`](crates/speed-editor) | HID protocol: authentication, keys, LEDs, jog modes, battery |
| [`crates/mapper`](crates/mapper) | Pure engine: pages, Fn layer, long press, native gestures, the Resolve-style wheel |
| [`crates/runtime`](crates/runtime) | Device loop, keyboard output, Resolve scripting helper, menus via UI Automation, focus handling, wheel pacing |
| [`apps/studio`](apps/studio) | The Tauri app (plain HTML/JS UI in `apps/studio/ui`) |
| [`apps/se-mapper`](apps/se-mapper) | Command-line runner |
| [`tools/se-probe`](tools/se-probe), [`tools/capture`](tools/capture) | Protocol probe, USB capture decoder and the test robots used for measuring Resolve |

Everything measured along the way is in [`docs/FINDINGS.md`](docs/FINDINGS.md), from wheel
units per frame and the shuttle curve to which key sequences give a ripple, roll or slip
trim. Decoded captures are in [`docs/captures`](docs/captures).

## Known limits

- Some Speed Editor functions have no Resolve keyboard command, so they do nothing until you
  map them: CLOSE UP, TRANS DUR, SYNC BIN, SMTH CUT (as a transition), SRC O/WR and others.
- TRIM IN / TRIM OUT through the keyboard follow Resolve's trim tool. Whether a trim ripples
  depends on the mode Resolve is in.
- Some shortcuts can't be typed on every keyboard layout (for example `[`, `]` and some Alt+F
  combinations on a German layout). Those functions go through Resolve's menus instead.
- The jog wheel's click is mechanical only. The device reports nothing, so it can't be mapped.
- Windows only for now.

## Credits

The device authentication and the key, LED and jog tables are ported from
[bmd.py](https://github.com/smunaut/blackmagic-misc/blob/master/bmd.py) by Sylvain Munaut.
See [`NOTICE`](NOTICE).

*DaVinci Resolve and Speed Editor are trademarks of Blackmagic Design. This project is not
affiliated with Blackmagic Design.*
