# Findings log

Verified facts only, with the date and how they were established.

## 2026-10-31 — Windows device layout (this PC)

Established with `Get-PnpDevice` and `se-probe list`.

The Speed Editor (USB `1EDB:DA0E`) is a composite device with two relevant interfaces:

| Interface | Driver | Used by |
|---|---|---|
| `MI_00` | Blackmagic `DaVinciKeyboards` (oem150.inf, class MEDIA) | presumably Resolve |
| `MI_02` | Microsoft `HidUsb`, vendor usage page `ff00` | hidapi tools (se-probe, Unbound, Companion) |

## 2026-10-31 — Behaviour with Resolve closed

- `se-probe watch` (read-only, `MI_02`): opens fine, receives **no reports**. That fits the
  protocol notes: the device stays silent until a host authenticates.
- `se-probe control`: the handshake on `MI_02` succeeds; jog reports start arriving straight away.
  The first report after setting relative mode had value +104065, which looks like a stale counter
  and needs a closer look.
- Jog reports arrive as 13 bytes on Windows (id, mode, le32 value, `ff`, zero padding).
- Battery reports (id 7) also arrive over USB: `07 01 13 96 01 00 ff …` = charging, 19%. Bytes after
  the level repeat the `96 01 00 ff` seen in the first jog report, so the device may pad reports with
  stale buffer bytes. Until that's checked, don't trust bytes beyond each report's documented length.

## 2026-10-31 — Hands-on control test (Resolve closed)

Log: `docs/captures/resolve-closed-control.jsonl`.

- All 43 keys from the key table reported correctly; no unknown key codes appeared.
- Holding several keys gives several codes in one report (e.g. `held [IN, OUT]`).
- **Pressing the jog wheel sends nothing.** The user pressed it several times and no report of any
  kind appeared, so the click is mechanical only and software cannot map it.
- Wheel, relative mode (0, JOG): one step = 360 units (values are multiples of 360 at slow speed);
  fast spins send larger deltas such as -9362.
- Wheel, mode 3 (SHTL): absolute position clamped to -4096..+4096.
- Wheel, mode 2 (SCRL): relative, also in 360-unit steps.
- Switching modes and the key LEDs worked from the host side.

## 2026-10-31 — Bluetooth (Resolve closed)

Log: `docs/captures/bluetooth-control.jsonl`.

- Paired through Windows Settings, the Speed Editor appears as a Bluetooth LE HID device
  (HID over GATT) with the same VID/PID `1EDB:DA0E`, one HID collection, vendor usage page `ff00`,
  interface number -1. hidapi finds it with no changes.
- The same feature-report handshake authenticates it; keys, multi-key holds, wheel modes and
  LED output reports all work as over USB.
- Battery reports (id 7) arrive by themselves, e.g. `07 00 55` = not charging, 85%.
- Reports sometimes pause for more than 200 ms (32 times in about a minute of use), which is normal
  Bluetooth batching; nothing was lost.
- The runtime prefers USB when the device is connected both ways.

## 2026-10-31 — The deciding test: Resolve 21.1 open, Speed Editor on USB

- With Resolve running, hidapi lists **no** Speed Editor HID interface, although Windows shows the
  USB `MI_02` HID device as present and OK.
- Opening `\\?\HID#VID_1EDB&PID_DA0E&MI_02#…` with zero access rights and full sharing fails
  with Windows error 32 (ERROR_SHARING_VIOLATION). **Resolve opens the HID interface exclusively.**
- Consequence: no program can listen to the Speed Editor while Resolve owns it, so "native
  Resolve plus extras on the same keys at the same time" is not possible over USB without a
  driver or hardware in between. Ownership is all-or-nothing: either Resolve or our app.
- Bluetooth (USB unplugged, Resolve still open): the Bluetooth HID collection is present, but opening
  it fails the same way (error 32). **Resolve takes the Bluetooth connection exclusively too.**
- Still open: what happens when our app owns the device first and Resolve starts afterwards?

## 2026-10-31 — Driving Resolve's playhead (Resolve 21.1, 30 fps timeline)

- `Timeline.SetCurrentTimecode` takes about 500 ms per call (median 499 ms over 60 calls), while
  `GetCurrentTimecode` takes about 0.6 ms. Scripting is far too slow to drive a jog wheel; the wheel
  must use Resolve's own step/playback commands (`controlStepForward/Reverse`, J/K/L…).
- While the native Speed Editor wheel turns, `GetCurrentTimecode` updates only sporadically
  (6 distinct values in 55 s of polling every 2 ms). Settled positions after a gesture are reliable,
  positions during a gesture are not.
- Rough native JOG data (hand-turned, so not exact): one slow turn about +61 frames, one fast turn
  about +76 frames. Not precise enough to calibrate; exact wheel units per gesture are needed.
- Resolve's keyboard layout can be exported with `ExportKeyboardPreset`: a custom layout file holds
  only its differences (`import "DaVinci Resolve"` plus changed `command := keys` lines); the base
  layout has 554 commands.

## 2026-10-31 — Native JOG measured with USBPcap (Resolve 21.1, USB, 30 fps)

Files: `docs/captures/native-jog.pcap` / `.jsonl` (wheel reports) and `native-jog-playhead.jsonl`
(playhead positions from `tools/capture/log_playhead.py`), lined up by timestamp.

- In JOG, Resolve runs the wheel in **device mode 2**. In mode 2 the reports are speed-dependent:
  multiples of 360 when slow, smoothly rising and falling values up to about 88,000 when spun fast.
- Resolve's response is **linear in the summed report values**: fast gesture 4,811,002 units →
  234 frames (20,560 units/frame); medium gesture 2,366,493 units → 116 frames (20,401 units/frame).
  So Resolve adds no acceleration of its own; the faster-spin-goes-further feel comes from the
  device's mode 2. Working figure: about 20,500 units per frame (to be refined).
- Implication: switch the wheel to mode 2 like Resolve does, accumulate report values and step
  one frame per ~20,500 units, keeping the remainder. Resolve's exact mode-2 setup report (its
  parameter byte) still has to be captured when a mode key is pressed.

## 2026-10-31 — Resolve's wheel setup and SCRL (USBPcap, `docs/captures/native-modes.*`)

- Pressing the mode keys, Resolve sends (output report 3, then jog LEDs via report 4):
  SHTL → `03 03 00000000 ff` (mode 3, param 255), LED bit 1; JOG → `03 02 00000000 ff` (mode 2),
  LED bit 0; SCRL → `03 02 00000000 ff` (mode 2), LED bit 2. JOG and SCRL use the same device mode;
  only Resolve's handling differs. No key LEDs were sent.
- Resolve re-authenticates now and then (feature report 6 sequence) and sends a random host challenge.
- SCRL: 1,522,783 units → +2,291 frames (665 units/frame); −1,584,710 → −2,289 (692 units/frame).
  About 30× JOG's rate, i.e. about one second per JOG frame at 30 fps.
- JOG refinement: slow gestures with pauses gave about 31,000 units/frame (−158,402 → −5;
  −60,442 → −2), fast ones about 20,500. Likely Resolve drops sub-frame remainders when the wheel
  pauses; to be tuned by A/B comparison.
- SHTL (mode 3) reports an absolute position relative to where the mode was set, ±4,096 per half turn,
  wrapping beyond that. Playback speed versus position still needs hold-and-return measurements;
  the playhead position is reported to scripts only rarely during playback.

## 2026-10-31 — Native SHTL (USBPcap, `docs/captures/native-shuttle.*`)

- Integrating speed over each shuttle excursion (position over time) against settled playhead
  moves, the best simple fit is speed ≈ 780 frames/s × (|pos| / 4096)^2.2 at 30 fps: about 1.2× at a
  quarter deflection, 5.7× at half, 26× at full. The fit is rough (±26 % across three excursions;
  one hold hit the end of the timeline and was excluded).
- Resolve's keyboard offers only J/L speed steps and frame stepping, so an emulated shuttle can
  approximate these speeds but cannot match every in-between speed exactly.

## 2026-10-31 — Trim key sequences in Resolve 21.1 (Edit page, German keyboard)

Measured with `tools/capture/resolve_key_lab.py` (types keys into Resolve, reads clip start/end/source
offset on V1 through scripting, undoes). Results in `docs/captures/key-lab.json` and `key-lab-2.json`.

- ROLL: `A V . .` (selection mode, select nearest edit, nudge) rolls the cut. Also `T V . .`.
- `A V U .` (selection mode) shortens the incoming clip's start without rippling (leaves a gap),
  so the trim keys follow Resolve's current tool mode like the real Speed Editor does.
- Ripple trim: `T V U` then `.` shortens the incoming clip's start (later clips shift left);
  `T V U` then `,` shortens the outgoing clip's end. Same with the playhead on either side of the cut.
  A second `U` returns to roll. No keyboard way found to select only the outgoing side or to lengthen.
- SLIDE: `T Shift+V S . . S` slides the clip at the playhead.
- SLIP: `T Shift+V . .` shifts the clip's source by 2 frames, position unchanged.
- MOVE: "swap clips" (`editNudgeSwapEditForward`, Ctrl+Shift+.) did nothing with any variant on a
  German keyboard (Shift+. types ":"); `A Shift+V . .` nudges the clip and overwrites its neighbour.
- Shortcuts that need a punctuation key with Shift (`[`, `]`, Ctrl+Shift+.) cannot reach Resolve as
  such on a German layout; Resolve functions with such shortcuts need a different key in Resolve.

## 2026-10-31 — Which tracks "Ripple Start to Playhead" affects (`docs/captures/ripple-lab.json`)

Tested with `tools/capture/resolve_ripple_lab.py` at a frame where V1, V2, A1, A3 all had clips.

- With all settings as they were, every track was trimmed and rippled (V1, V2, A1, A3, A4).
- Toggling sync lock on V2, A1, A3 made no difference.
- Toggling the Auto Track Selector on A1 and A3 (Ctrl+Alt+F1/F3) left those tracks completely alone:
  neither trimmed nor shifted. The Auto Track Selector, not sync lock, decides which tracks a
  playhead ripple affects.
- Toggling V2's Auto Track Selector with Alt+F2 changed nothing; the shortcut probably did not reach
  Resolve, so the video case is not confirmed by this test.

## 2026-10-31 — Setting the Auto Track Selector from the keyboard (`docs/captures/track-lab.json`)

Tested with `tools/capture/resolve_track_lab.py`, probing which tracks a ripple affects (then undone).

- Audio selector toggles (Ctrl+Alt+F1, Ctrl+Alt+F3) work reliably when sent by the app.
- Video selector toggles (Alt+F1, Alt+F2) and "toggle all" (Alt+F10) had no effect when sent with
  SendInput. Every failing shortcut is Alt + function key, which Windows treats as a system key.
- Resolve exposes the selector state neither to scripts nor in any readable way, so a "only this
  track" function has to keep its own record of what it toggled.

## 2026-10-31 — Resolve's menus through Windows UI Automation (no shortcuts needed)

- Resolve's Qt UI exposes the menu bar to UI Automation. A menu's items appear once it is expanded
  (ExpandCollapse pattern); items can be invoked (Invoke pattern) and checkable ones report their
  state (Toggle pattern). Verified: Timeline › Snapping toggled On → Off → On, 76–144 ms per invoke
  including opening and closing the menu.
- Track headers (Auto Track Selector buttons) are custom-drawn and not exposed.
- The menus cover the functions keyboard shortcuts could not reach on this machine:
  Timeline › Auto Select › Auto Select/Deselect V1–V8, A1–A8, **Auto Select All Tracks**,
  **Auto Deselect All Tracks**; Timeline › Sync Lock / Track Lock / Source Track Selector;
  Edit › **Swap Clips Towards Left/Right**; Trim › Select Nearest Edit To › Resize In/Out,
  **Ripple In/Out**, **Roll**; Trim › Select Nearest Clip To › **Move/Slip/Slide**;
  Trim › Ripple › Start/End to Playhead; Trim › Nudge › One Frame Left/Right; tool modes.

## Earlier open question: the deciding test

Start Resolve, then run `se-probe watch --raw --log docs/captures/resolve-open.jsonl`.

- **Reports arrive** → Resolve's authentication also unlocks `MI_02`, so an add-on can listen alongside
  Resolve ("extended native" mode is possible).
- **Nothing arrives** → Resolve talks over `MI_00` only; add-ons can't see input while Resolve runs, so we
  build custom mode (Resolve closed) only.

Never run `se-probe control` while Resolve is open: it re-authenticates and changes LEDs and wheel mode.

## Wheel pacing, menu focus and swap (2026-10-31)

- **Scrubbing lagged on H.265.** Resolve works through key presses one at a time; on long-GOP footage
  a fast JOG/SCRL turn queued dozens of arrow keys and the playhead kept going after the wheel stopped.
  `Timeline.GetCurrentTimecode()` answers in about 0.4 ms (20 calls: 0.2–2.6 ms), so the runtime now
  paces steps against the real playhead (`runtime/src/pacer.rs`): at most 2 steps in flight, 2 waiting,
  the rest dropped. Without a readable playhead it falls back to sending steps directly.
- **Menu bar kept the keyboard.** After a menu item runs through UI Automation, Resolve's menu bar
  could keep keyboard focus, so the next key (SPLIT, ripple delete, J/K/L) only left the menu. After
  each menu click the runtime checks the focused element and presses Esc while it is a Resolve menu.
- **Swap jumped 3–4 clips** on a gentle touch (each swap is a ~0.2 s menu click; a touch lasts longer).
  Now one swap per touch: the wheel must rest 250 ms before the next swap.
- **RIPL DEL** deletes the clip under the playhead: `editSelectItemAtPlayhead`, then `editDelete`.
- Debug log: set `OPEN_SPEED_EDITOR_LOG` to a file path before starting the app to log every wheel
  report and output.

## 2026-11-01 — Which device interface Resolve opens

Established by listing Resolve.exe's open file handles (NtQuerySystemInformation + NtQueryObject,
read-only) while Resolve owned the Speed Editor over USB, and mapping the device object to its
PnP node (`DEVPKEY_Device_PDOName`).

- Resolve holds exactly one handle to the Speed Editor: `\Device\0000008d` =
  `HID\VID_1EDB&PID_DA0E&MI_02\…`, the plain HID interface (vendor usage page `ff00`), opened
  read/write (access `0x12019f`).
- Resolve does **not** open `MI_00` (Blackmagic's `DaVinciKeyboards` driver).
- So Resolve talks to the Speed Editor as an ordinary HID device. An emulated HID device with the
  same VID/PID and report descriptor is a realistic way to hand native key presses to Resolve
  unchanged. As the device side, an emulator can accept any authentication answer from the host.

## 2026-11-01 — Emulated Speed Editor: Resolve accepts it and runs native functions

Set-up: usbip-win2 0.9.8.1 (Microsoft-signed drivers) plus `tools/se-emu`, a USB/IP server that
presents the real device's USB descriptors. Real Speed Editor unplugged, Resolve 21.1 running.

- **What Resolve does when it adds a Speed Editor** (USBPcap, real device plugged in):
  SET_IDLE; GET feature 1 (`01 4d 02 25 7b 01 05 02`: software version, firmware 1.5.2);
  GET feature 8 (serial number, 32 ASCII bytes); output reports 3 (wheel mode), 4 (wheel LEDs),
  2 (key LEDs); the authentication on feature report 6; GET input report 7 (battery, `07 01 63`).
- **The report descriptor must be the device's own** (308 bytes, now in
  `crates/speed-editor/src/emulator.rs`). The one hidapi rebuilds on Windows (299 bytes) drops
  feature reports 1 and 8, so Resolve's version and serial reads fail inside Windows and Resolve
  logs "Failed to add device." (`davinci_resolve.log`, component SyManager).
- With the original descriptor and answers for reports 1, 8 and 7, Resolve logs
  **"Keyboard connected."**, sets wheel mode and LEDs, and reads the battery.
- **Resolve checks the device's answer to its challenge.** It is not `auth::response()` of the
  challenge (3 captured pairs, none match simple variants). With an all-zero answer Resolve retries
  the handshake 10 times, then **accepts the device anyway**.
- **Native functions work:** IN sent from the emulator set an in-point at the playhead (frame 3992,
  checked through scripting); STOP/PLAY started and stopped playback.
- Windows needed no restart after installing usbip-win2.
- Captured handshakes (host challenge → device answer, device's own challenge), little-endian u64:
  `b3b7b10bdb52eaf8 → 134810687129fb41` (7732a06f1cc3148c),
  `ac1b902b86739fcb → 2101e5b33e9001e9` (4b2760d006451e3c),
  `27c1f347ce0a5fd4 → 1fd49dddfc12bb61` (6287c6d1400df5e0).
- **Pass-through in the app (2026-11-01):** Speed Editor Studio holds the real device, runs the
  emulator (`crates/speed-editor/src/emulator.rs`) and attaches it with usbip-win2. Native keys
  and the DaVinci wheel go to Resolve unchanged; Resolve's LEDs and wheel modes come back to
  the real device. Tested by the user with the real device: works. The app must start before
  Resolve, which otherwise takes the real device first.

## 2026-11-01 — Track headers, Sync Lock and selecting a clip on a track

- **Resolve 21's track header:** the red track label (e.g. `V1`) is the **Auto Track Selector**
  ("Timeline › Auto Select › Auto Select All Tracks" turned every label red). The `<|>` icon next
  to the lock is **Sync Lock**. Neither state can be read through scripting, nor through the menu
  (the Auto Select and Sync Lock items are plain toggles without a check mark).
- **CUT + CAM n works:** after it only V n and A n had the red label (screenshots before and after).
- **Sync Lock decides what else a ripple drags along.** With only V1 + A1 auto-selected, Ripple
  Start to Playhead still changed V2 and A2 while their Sync Lock was on; with Sync Lock off on
  the other tracks they stayed alone (found by the user). Timeline › Sync Lock only offers toggles
  ("Enable/Disable Sync Lock V1…A8 / for All Video / All Audio / All Tracks").
- **Selecting a clip on a track, keyboard only** (`tools/capture/resolve_select_lab.py`, V1+A1
  auto-selected, playhead inside clips on V1, V2, A1, A2, A3): Shift+V (select at playhead) →
  V1 + A1 (linked); Ctrl+Up → V2; Ctrl+Up at the top → V2; Ctrl+Down → V1 + A1; → A2; → A3.
  So "select at playhead", then one track up/down at a time, reaches any track, linked audio
  included. `Timeline.GetSelectedClips()` reads the result, which lets the app check each step.
- **Correction: stepping goes by the selected clip, not the playhead.** "Select clip above/below"
  (Ctrl+Up/Down) picks the clip on the next track at the *start of the selected clip*. Where clips
  on different tracks start at different frames it lands on another clip than the one under the
  playhead (user's timeline: A2 clip from 5329, A1 clip under the playhead from 5396 — Ctrl+Up from
  A2 selected the previous A1 clip). With linked selection off it also does not cross from a video
  track to an audio track (V1 → Ctrl+Down stayed on V1); audio ↔ audio works.
- **"Select at playhead" (Shift+V) takes the clip under the playhead on the topmost auto-selected
  track** (with V1 + A1 auto-selected: V1, plus A1 if linked selection is on). That is what
  DIS + CAM n uses: select at playhead; if that isn't track n, auto-select only track n (or only V n
  / A n) through the Auto Select menu and select again.
- **Only the audio (or video) of a linked clip:** with linked selection on, "select at playhead"
  always takes both. Switching Timeline › Linked Selection off (Ctrl+Shift+L), selecting, and
  switching it back on **keeps the single part selected** (logged: `["A1"]` before and after), the
  same as Alt+click. Linked selection is a timeline-wide switch: leaving it off makes linked clips
  behave as if unlinked, so the app always switches it back on.
- `TimelineItem.GetLinkedItems()` and `GetUniqueId()` work, so the helper reports linked parts
  that are not selected — the sign that linked selection is off.
