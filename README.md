# Speed Editor Studio

Your own key bindings for the Blackmagic **DaVinci Resolve Speed Editor**, working inside DaVinci
Resolve, while every key and the jog wheel keep their native behaviour.

[![Sponsor on GitHub](https://img.shields.io/badge/Sponsor-GitHub%20Sponsors-ea4aaa?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/koosoli)
[![Buy Me a Coffee](https://img.shields.io/badge/Buy%20me%20a%20coffee-support-ffdd00?logo=buymeacoffee&logoColor=black)](https://buymeacoffee.com/koosoli)

> [!TIP]
> **Don't want to compile? Ready-to-use builds for Windows (and an untested one for macOS)
> are waiting for supporters.**
> 1. Become a supporter: a monthly membership through
>    [GitHub Sponsors](https://github.com/sponsors/koosoli), or a one-time contribution through
>    [Buy Me a Coffee](https://buymeacoffee.com/koosoli) (leave your GitHub username there).
> 2. You're then added to the private builds repository and get its releases.
>
> **The builds repository is private:** its link
> ([SpeedEditorStudio-Builds](https://github.com/SpeedEditorStudio/SpeedEditorStudio-Builds/releases))
> shows "404 – Page not found" until you've been added and are signed in to GitHub. That's
> expected, not a broken link. Your support directly funds the project's development, testing
> and builds. Building from source stays free, and your rights under the license are the same
> either way.

> [!WARNING]
> **Made for Windows. macOS support is completely untested.** The macOS code exists and is
> built on a Mac, but it has never been run on a Mac with a Speed Editor or with DaVinci Resolve.
> Assume it doesn't work until someone reports otherwise. See [macOS](#macos-untested).

![Speed Editor Studio](docs/images/studio.png)

DaVinci Resolve opens the Speed Editor exclusively, so other tools can't add anything to it
while Resolve runs. Speed Editor Studio takes over the device instead and presents Resolve
with an **emulated Speed Editor**. Keys doing their native job and the wheel are passed
through to Resolve unchanged, so they behave exactly like the original, Smart Insert and
the jog feel included. On top of that, you add your own pages, function-key layers and
Stream Deck-style actions, which the app handles itself. **You never have to change a shortcut in
Resolve.**

## Features

- **Native behaviour, unchanged.** With [usbip-win2](https://github.com/vadimgrn/usbip-win2)
  installed, native keys and the wheel go to Resolve through an emulated Speed Editor, so
  Resolve itself handles them, including its LEDs and wheel modes. Without usbip-win2 the app
  imitates them: a calibrated wheel and Resolve's shortcuts and menus.
- **Three pages on CUT, DIS and SMTH CUT.** CUT is the "DaVinci" page: everything native,
  with any key you choose replaced by your own action. DIS and SMTH CUT are free pages. The
  active page's LED blinks.
- **Normal mode.** CUT + SMTH CUT switches every key back to its plain native function (the
  three LEDs flash to confirm). Press the chord again to return to your pages.
- **Three function keys on every page.** CUT, DIS and SMTH CUT also work like Fn keys: hold
  one and press another key, and that key does what you mapped for this combination. Every
  page has its own CUT + key, DIS + key and SMTH CUT + key layer, so each page holds up to four
  actions per key. A quick tap of CUT, DIS or SMTH CUT on its own still switches the page.
  In the default profile, CUT + IN/OUT ripple to the playhead, CUT + TRIM IN/OUT trim start or
  end, and CUT + SLIP SRC/DEST select clips backward/forward on all tracks.
- **Zoom the timeline with the jog wheel.** Hold CUT and turn the wheel: clockwise zooms in,
  anticlockwise zooms out.
- **Trim with or without ripple.** CUT + CLOSE UP switches Resolve to the Trim tool (CLOSE UP
  lights; TRIM IN/OUT ripple the timeline) and back to the Selection tool (trims leave a gap).
  CLOSE UP on its own keeps its native function.
- **Source Tape on the Edit page too.** SOURCE switches the Edit page's source viewer
  between Source Clip and Source Tape, as it does on the Cut page (needs Resolve's
  scripting set to Local, see Requirements).
- **Step by frame or by clip.** CUT + SHTL goes one frame back and CUT + SCRL one frame
  forward, like ← and →. DIS + SHTL jumps to the previous clip and DIS + SCRL to the next
  one, like ↑ and ↓.
- **Switch Resolve's pages.** SMTH CUT + CAM 1–7 opens the Media, Cut, Edit, Fusion, Color,
  Fairlight and Deliver page (needs Resolve's scripting set to Local, see Requirements).
- **Select a clip on a certain track.** DIS + CAM 1–8 selects the clip under the playhead on
  that track, with its linked sound, using Resolve's own selection keys.
- **Video or audio only.** VIDEO ONLY or AUDIO ONLY pressed on its own lights up and makes
  CUT + CAM and DIS + CAM take only the video or only the audio (even of a linked clip); press
  it again for both.
- **Track targeting.** CUT + CAM n makes only video track n + audio track n take part in
  edits, until CUT + CAM 9 turns all tracks back on. The CAM key stays lit.
- **Move a clip frame by frame.** Hold CUT + SPLIT and turn the wheel. SPLIT + wheel on its
  own swaps clips, one clip per touch.
- **Stream Deck-style keys.** Each key gets a label, a press action and a long-press action:
  a shortcut, a Resolve function, a Resolve menu item, a Resolve script command, an app or
  URL to open, or text to type.
- **Resolve menus without shortcuts.** Functions that have no usable shortcut are run through
  Resolve's own menus (UI Automation on Windows, Accessibility on macOS), so they work on any
  keyboard layout.
- **Live mirror.** The app shows the device's LEDs, flashes the keys you press, turns the
  on-screen wheel and shows the battery level.
- **USB and Bluetooth.**
- **Profiles.** Save, export and import as plain TOML. See
  [`profiles/example.toml`](profiles/example.toml).

![The device view](docs/images/device.png)

*The DaVinci page with its CUT + key layer: what each key does while CUT is held.*

## The editor

- **Pick a key** by clicking it on the picture, or press it on the Speed Editor while the
  app's window is in front. The panel on the right sets its label, press and long-press
  action. Keys show their secondary function in brackets, as printed on the device. Plain
  keys keep their DaVinci function; dimmed keys have no Resolve keyboard command, so they
  are free for your own mapping. Mapped keys turn teal.
- **Pages** are the tabs on the left (CUT, DIS, SMTH CUT). The icons on the right switch
  between the picture and a list of all mappings, and between the plain keys and the CUT,
  DIS and SMTH CUT layers. The picture turns purple only while you hold a function key on
  the device.
- **Actions** at the bottom folds open: drag an action onto a key, or select a key and click
  the action.
- **Top bar:** **Connect** takes over the Speed Editor, **Active** switches your mappings
  on (when off, presses only light up in the app, for testing). Then come import, export
  and revert, the settings (⚙) with **Give to Resolve** and **Start with Windows**, and
  **Save**. The dot shows the connection; hover it for details.
- **?** in the side panel shows the longer explanations.
- The console at the bottom shows what just happened, newest line first.

## Requirements

- Windows 10 or 11. macOS is **completely untested**, see [macOS](#macos-untested).
- DaVinci Resolve (developed and measured with Resolve Studio 21.1). For the script actions,
  set *Preferences › System › General › External scripting using* to **Local**.
- A DaVinci Resolve Speed Editor, over USB or paired over Bluetooth.
- **[usbip-win2](https://github.com/vadimgrn/usbip-win2/releases)** for the full native
  behaviour (see below).

### Install usbip-win2 first

Speed Editor Studio hands the Speed Editor's native keys and the jog wheel to Resolve through
an emulated Speed Editor. Windows needs a driver to plug in such a virtual device, and
usbip-win2 provides it: free, open source and with Microsoft-signed drivers.

1. Download `USBip-<version>-x64.exe` from the
   [usbip-win2 releases](https://github.com/vadimgrn/usbip-win2/releases).
2. Run it and confirm the Windows admin prompt; the default options are fine. A restart is
   usually not needed.

The app finds usbip-win2 by itself; there is nothing to configure. Its status line says
"Resolve pass-through on" when it works.

**Without usbip-win2** the app still runs, but imitates the native keys with Resolve's
shortcuts and menus. Then some Speed Editor functions do nothing until you map them yourself,
because Resolve has no keyboard command for them: CLOSE UP, TRANS DUR, SYNC BIN, SMTH CUT (as
a transition), SRC O/WR and others.

## Ready-made builds

Ready-made builds are for supporters (see the tip at the top); the builds repository
is private, so its link gives a 404 until you've been added. Each release has a Windows zip
and, for testing, an untested macOS zip (see [macOS](#macos-untested)).

Speed Editor Studio needs **no installation**: unzip the download and start
`Speed Editor Studio.exe`. Your settings are kept in `%APPDATA%\OpenSpeedEditor`.

The program isn't code-signed yet, so on first start Windows SmartScreen may say "Windows
protected your PC". Click **More info › Run anyway**; it won't ask again.

## Build and run

You need [Rust](https://rustup.rs/) (stable) and the WebView2 runtime, which Windows 11 has
built in.

```bash
cargo build --release
```

Then start `target\release\se-studio.exe`. Turn on **Connect** and **Active**.

**Start Speed Editor Studio before DaVinci Resolve.** Windows lets only one program hold the
Speed Editor, and Resolve takes one the moment it appears. When the app holds the real device
first, Resolve uses the emulated one instead.

The easiest way is **Start with Windows** (settings ⚙ in the top bar): the app then starts
hidden in the system tray when you sign in. Closing the window keeps it running in the tray; **Quit** in the tray
menu ends it. If Resolve got the device first anyway, the app says so: quit Resolve and start
it again.

- **Give to Resolve** (also under ⚙) hands the device back to Resolve's own driver whenever
  Resolve runs. It's off by default, because then your mappings can't work inside Resolve.
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
| [`crates/runtime`](crates/runtime) | Device loop, keyboard output, Resolve scripting helper, Resolve menus, focus handling, wheel pacing; one file per platform for the system-specific parts (`*_windows.rs`, `*_macos.rs`) |
| [`apps/studio`](apps/studio) | The Tauri app (plain HTML/JS UI in `apps/studio/ui`) |
| [`apps/se-mapper`](apps/se-mapper) | Command-line runner |
| [`tools/se-probe`](tools/se-probe), [`tools/se-emu`](tools/se-emu), [`tools/capture`](tools/capture) | Protocol probe, stand-alone emulated Speed Editor, USB capture decoder and the test robots used for measuring Resolve |

Everything measured along the way is in [`docs/FINDINGS.md`](docs/FINDINGS.md), from wheel
units per frame and the shuttle curve to which key sequences give a ripple, roll or slip
trim. Decoded captures are in [`docs/captures`](docs/captures).

## macOS (untested)

> [!CAUTION]
> **Completely untested.** Nobody has started this app on a Mac yet: not with a Speed
> Editor, not with DaVinci Resolve, not at all. Supporters get a macOS build for testing; use
> it only if you want to try it out and report back.

The code is prepared for macOS, but **nobody has run it on a Mac with a Speed Editor and
DaVinci Resolve yet.** The [macOS build](.github/workflows/macos.yml) builds and tests it on a
real Mac on GitHub (one app for Apple Silicon and Intel), and the macOS parts were written to
mirror the Windows ones. Expect rough edges, and please report what works and what doesn't.

What's different on macOS:

- **No pass-through.** usbip-win2 exists only for Windows, and a virtual USB/HID device on
  macOS needs a driver entitlement from Apple. So on a Mac the app always works the way it
  does on Windows without usbip-win2: it imitates the native keys with Resolve's shortcuts and
  menus, with the calibrated wheel. Functions without a Resolve keyboard command (CLOSE UP,
  SYNC BIN, …) only work if you map them.
- **Start order matters there too.** Resolve opens the Speed Editor exclusively on macOS as
  well: whoever starts first owns it. Start Speed Editor Studio first (switch on **Start at
  login**), then Resolve.
- **Permissions.** On first start macOS asks to allow the app under *System Settings › Privacy
  & Security › Accessibility*; without it the app can't send keys to Resolve or use its menus.
  If the device isn't seen, also allow it under *Input Monitoring*.
- **Resolve's menus** are clicked through the Accessibility API, without opening them on
  screen.
- **Script actions** need a Python 3 on the Mac (Resolve's scripting requirement); the app
  looks for one bundled with Resolve, then `/usr/bin/python3`.
- **Not signed.** The app isn't signed by Apple or notarized, so macOS blocks the first start:
  right-click the app › **Open**, or on newer macOS try to open it once, then *System Settings ›
  Privacy & Security › Open Anyway*. If macOS calls the app damaged, run
  `xattr -dr com.apple.quarantine "/Applications/Speed Editor Studio.app"`.
- Settings are kept in `~/Library/Application Support/OpenSpeedEditor`.

To build it yourself on a Mac: install [Rust](https://rustup.rs/) and Node.js, then in
`apps/studio` run `npx @tauri-apps/cli@2 icon icons/icon.png --output icons` and
`npx @tauri-apps/cli@2 build --bundles app --config '{"bundle":{"active":true,"icon":["icons/icon.icns","icons/icon.png"]}}'`.

## Known limits

- Without usbip-win2, some Speed Editor functions do nothing until you map them (see
  [Install usbip-win2 first](#install-usbip-win2-first)). Some shortcuts can't be typed on every
  keyboard layout (for example `[`, `]` and some Alt+F combinations on a German layout); those
  go through Resolve's menus.
- The emulated device doesn't yet give the real device's answer in the authentication
  handshake (the device side uses an unknown calculation). Resolve retries and then accepts it
  anyway; a future Resolve version might be stricter.
- The jog wheel's click is mechanical only. The device reports nothing, so it can't be mapped.
- macOS is completely untested and has no pass-through, see [macOS](#macos-untested).

## License

Copyright 2026 Olivier KOOS. Licensed under the
[PolyForm Noncommercial License 1.0.0](LICENSE).

- **Free for noncommercial use:** personal projects, hobby work, study, research, and
  charities, schools and public institutions.
- **Not for commercial use.** For commercial use, ask Olivier KOOS for a separate license.
- **Credit required:** anyone who passes on any part of the software must include the
  license terms and the `Required Notice` line from [`LICENSE`](LICENSE).

This is a source-available license, not an OSI-approved open-source license.

## Credits

Created by Olivier KOOS.

The device authentication and the key, LED and jog tables are ported from
[bmd.py](https://github.com/smunaut/blackmagic-misc/blob/master/bmd.py) by Sylvain Munaut,
under the Apache License 2.0. See [`NOTICE`](NOTICE).

*DaVinci Resolve and Speed Editor are trademarks of Blackmagic Design. This project is not
affiliated with Blackmagic Design.*
