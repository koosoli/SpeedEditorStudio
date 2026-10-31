# Open Speed Editor Mapper
## Exploratory implementation brief for an autonomous coding agent

**Date:** 30 October 2026  
**Status:** Proposal and research starting point — not a prescribed architecture  
**Target initially:** Windows 10/11, an original Blackmagic Design DaVinci Resolve Speed Editor, and DaVinci Resolve

> **Working philosophy:** Please challenge the assumptions in this document. Investigate the repositories and the real device/OS behaviour independently, propose a better technical approach if one exists, and explain any trade-offs. The aim is a genuinely useful, robust tool — not strict adherence to the implementation ideas below.

## 1. Product vision

The user likes the Speed Editor exactly as it currently behaves in DaVinci Resolve, particularly the exceptionally responsive and nuanced jog wheel. The aim is to *add* customisation, not replace that experience with an approximate keyboard-shortcut simulation.

Imagine a **free, preferably fully open-source Speed Editor Mapper** with a clear visual representation of the physical controller. A user could click a button, or drag an action onto it, then assign a keyboard shortcut, application command, script, or another action. There could be an additional **Fn/Shift layer**, perhaps activated by a configurable existing button, so combinations such as `Fn + CAM 1` perform extra commands. The user should also be able to control applications other than Resolve when useful.

The two fundamental priorities are:

1. **Native Resolve experience:** In the ordinary/default mode, all original buttons, jog/shuttle/scroll modes, timing, and any relevant feedback should behave exactly as they do without our application.
2. **Non-destructive and reversible:** No modification or flashing of the Speed Editor's firmware. Turning off or uninstalling our software should allow normal stock operation again, allowing for a device reconnect or Resolve restart if actually necessary.

Please do not assume that either requirement is automatically satisfied by existing open-source mapping applications. A remapper that takes exclusive control and sends generic hotkeys is valuable in *custom mode*, but does not by itself meet the *native-while-extended* goal.

## 2. Desired user experience

The following features describe the intended result, not a mandatory first-release checklist:

- A recognisable, accurate, interactive diagram of the Speed Editor, including all buttons, the wheel and its three modes, plus useful visual status indicators.
- Click-to-assign and ideally drag-and-drop mapping, with a searchable action palette; optionally a **Learn** mode: press a physical button or combination and see it highlighted in the UI.
- Shortcut recording (for example `Ctrl+Shift+M`), with sensible handling of modifier presses, releases, focus, and app-specific keymaps.
- Normal/default layer, one or more custom layers, and a configurable **hold-for-Fn**, toggle, or other activation behaviour. An existing key repurposed as Fn may need special handling to suppress its normal Resolve action.
- Per-application profiles and a clearly visible active profile; examples include Resolve, OBS, other editors and generic Windows operations.
- Possible action types: keyboard shortcuts, shortcuts/sequences, press-and-hold, app launching, OBS commands, and supported Resolve integrations. Only offer an action when an appropriate execution path exists; do not imply that arbitrary Resolve operations are available via its scripting API.
- A **Native / Extended Native / Custom** mode concept, *if the underlying technology can support it safely*. Distinguish modes visibly so users understand whether Resolve retains native ownership.
- Local configuration (for example versioned JSON), profile import/export, undo, reset to defaults, conflict warnings, backup and recovery, and an easy way to pause/disable the mapper.
- Optional diagnostic screen exposing raw input reports and decoded events to help us study the protocol. Recording should be opt-in and privacy-conscious.
- Reliable disconnection/reconnection, sensible error reporting, and no stuck synthetic modifier keys after exit, errors or focus changes.

The interface could be inspired by **Central Control Studio's** visual mapping and layer/zone approach, but no dependence on that commercial product is desired. Its generic mapping UI is documented at https://centralcontrol.io/studio/ ; whether Central Control actually supports the *Speed Editor* or coexistence with Resolve remains **unverified**.

## 3. Open-source projects worth evaluating (source code, not just README files)

The repositories below are **candidate references**, not a predetermined technology stack. Please confirm current revisions, platform behaviour, maintenance status and license terms independently.

### A. Unbound — desktop remapper (Python / PyQt6)
https://github.com/PuzzleEmptyM/Unbound-editor-device-customizer

Potentially relevant source areas:

- `hid_layer.py`: HID device access, authentication, events and jog-wheel reports.
- `main.py`: pressed/released key-state tracking, active layer stack, dispatch and dial-event processing.
- `app.py`: existing PyQt interface and action selection.
- `config.py` and `actions/`: mappings, persistence and execution backends.

A useful code-level observation: `main.py` calculates `newly_pressed` and `newly_released` from the currently held key set, and has a layer stack. That is a good foundation for reasoning about chord/held-modifier behaviour. **It does not establish that reliable multi-button combination semantics, native passthrough, or coexistence with running Resolve already work.** Its jog processing currently uses its own threshold/action mechanism; treating that as identical to native Resolve wheel behaviour would be a mistake.

Unbound's published use case is controlling other software without Resolve being open. Its overall license was not clearly established in the repo overview inspected for this brief. **Do not assume public source means permission to copy, fork and redistribute it.** Verify permission before incorporating code.

### B. speed-editor-client — reusable Go HID library
https://github.com/JamesBalazs/speed-editor-client

This is an especially interesting alternative to a Python-first implementation. It advertises an API for connecting/authenticating, polling buttons and jog values, addressing buttons by name/ID/layout, accessing LEDs and changing jog modes. It includes examples and an **Apache-2.0** license in the repository. The author explicitly presents it as reusable library code. Assess the actual implementation quality, test coverage, Windows behaviour, Bluetooth support, maintenance, and suitability for a packaged app. It might be a cleaner base for a small backend than forking a full GUI remapper.

### C. Bitfocus Companion — mature control/action ecosystem
https://github.com/bitfocus/companion  
https://companion.free/user-guide/v4.2/surfaces/blackmagic-controllers/

The official Companion guide lists the **Resolve Speed Editor as supported from v4.2**, via USB and Bluetooth. Explore the actual surface integration, event dispatch, connection lifecycle, module/action system, GUI and licensing. It may offer reusable ideas, an integration target, or a faster path to broad application actions. **Device support alone does not prove transparent concurrent control while Resolve runs.**

### D. Blackmacro-lib — experimental native Resolve protocol emulation
https://github.com/KipJM/blackmacro-lib

A particularly relevant research reference: MicroPython firmware demonstrates that a compatible microcontroller can emulate *parts* of the Speed Editor's native HID protocol to communicate with Resolve. This is **not** an off-the-shelf Windows software proxy, and the README itself describes limits. Study its documented observations, packet formats and constraints before concluding what a software-only transparent bridge can achieve.

The repository labels the work **AGPLv3** and states additional project-specific restrictions/requests in its README. Evaluate the actual license text and reuse permissions carefully. Avoid copying implementation until license compatibility and permissions are understood; a clean-room implementation based on independently measured interoperability behaviour may be preferable.

### E. Additional references

- **CommandPost:** https://commandpost.fcp.cafe/control-surfaces/resolve/ ; source at https://github.com/CommandPost/CommandPost . It explicitly states that a running Resolve instance overrides its Speed Editor integration. This is important evidence that simultaneous ownership is a real issue (not proof that every possible Windows approach fails). Mostly a macOS-oriented reference.
- **bmdkey:** https://github.com/davidgiven/bmdkey — a small BSD-licensed X11 macro-pad implementation; explicitly works only when Resolve is closed. Useful for a simple comparison, not a Windows co-control solution.
- **Sylvain Munaut's interoperability work:** https://github.com/smunaut/blackmagic-misc/blob/master/bmd.py — an early HID/authentication reference, with notices to inspect before reuse.

A productive first deliverable might be a **short comparative source-code audit**: exact useful modules/functions, supported OS paths, licenses, native compatibility, evidence, risks and a reasoned recommendation of reuse versus rewrite.

## 4. The key engineering unknown: device ownership

The fundamental question is not whether we can read Speed Editor button reports. We have several examples of that. It is:

**Can an extension observe and, when necessary, suppress or reinterpret selected physical inputs while Resolve continues to receive everything else exactly as before — including the native jog wheel and host-to-device commands?**

Please investigate alternatives rather than committing immediately to one:

| Approach | Why it is attractive | Main unknown or downside |
|---|---|---|
| **Passive monitoring alongside Resolve** | Easiest to preserve native wheel behaviour; no normal-event translation | Whether the relevant Windows HID access is shareable; observing does not necessarily let us *suppress* a native action |
| **Read-only USB traffic capture** (e.g. USBPcap / Wireshark) | Helps characterise real traffic while Resolve owns the device | Diagnostic observation only; not a production interposition/remapping solution; USB only; may require capture-driver installation |
| **Exclusive controller + shortcut/API translation** | Existing tools already approximate this; useful for non-Resolve applications | Changes native Resolve interaction and wheel feel; inadequate for a guaranteed default-native mode |
| **Transparent software bridge / virtual-device route** | Could, in principle, transform only selected reports while forwarding the rest | Windows virtual HID/device compatibility, host-side output and authentication, driver ownership, permissions, stability and latency; substantial feasibility risk |
| **External USB-host-to-device hardware proxy** | May provide a practical separation between the physical device and the device presented to Resolve | Additional hardware and considerable protocol engineering; not desirable unless software-only options fail |

Another possible workaround might use separate USB and Bluetooth paths, Windows filter-level approaches or application-level integrations. These are **hypotheses for investigation**, not established solutions. Evaluate whether they genuinely offer the required semantics without introducing unsafe driver changes or breaking native operation.

Especially important: **A passive observer can trigger `Fn+CAM1` but probably cannot prevent Resolve from also reacting to `Fn` and/or `CAM1`.** This is why simple observation is not enough to promise clean remapping of every existing key. Distinguish truly additive shortcuts from substitutions that require interception.

A virtual keyboard sending Resolve shortcuts is not equivalent to emulating a Speed Editor that Resolve recognises. And cloning USB reports is not necessarily enough: authentication, report descriptors, host commands, LED state and timing may matter.

## 5. A protocol recorder as a useful investigative tool

A small separate **Speed Editor Protocol Recorder / Event Inspector** seems worthwhile, even if a full proxy is later rejected. It could help answer precise questions rather than relying on assumptions.

Possible features:

- Read or import USB captures, show timestamped HID reports, and display a human-readable decoding when available.
- A guided recording sequence: each button down/up, simultaneous keys, button held while turning the wheel, slow/fast jog, shuttle across positions and centre, scroll mode, LED changes, reconnect and authentication states.
- Separate **device-to-host** from **host-to-device** traffic; distinguish raw packets from decoded button events and from the *resulting Resolve action*.
- Save annotated test sessions, export redacted JSON/CSV, and compare baseline versus experimental configurations.
- Support mock/recorded packet replay for automated development without hardware.

USB captures taken while Resolve is running can reveal **communications**, but they cannot automatically reveal all of Resolve's internal contextual behaviours or prove a proxy will work. Only the real controller plus real Resolve can establish end-to-end fidelity. Prefer passive measurements and avoid altering drivers or firmware to obtain captures.

## 6. Suggested architecture, conditional on findings

The following division is a thought experiment, not a directive to adopt specific frameworks:

```
Physical Speed Editor
       |
       v
Transport / protocol adapter
  - USB HID, optionally Bluetooth
  - authentication, packets, lifecycle
       |
       v
Normalised events (button down/up, wheel mode/value, LEDs)
       |
       +--> Optional diagnostics / recorder
       |
       v
Ownership / routing policy
  - original-only safe behaviour
  - extended-native, only if proven possible
  - exclusive custom-controller mode
       |
       v
Mapping + chord/layer engine
  - precedence, holds, conflicts, app profiles
       |
       +--> Supported action backends (shortcuts, apps, OBS, etc.)
       |
       +--> Native Resolve path or proxy, only if feasible

Independent GUI <--> versioned profile/configuration model
```

Separating device protocol, routing, mapping, action execution and GUI might make it easier to substitute **Unbound's Python techniques**, **the Go library**, or **Companion integrations** as the evidence warrants. Consider Python/PyQt, Go plus an appropriate UI, or another lightweight architecture; justify the choice with packaging, latency, extensibility and maintenance evidence.

The user prefers a polished, simple Windows application rather than a collection of scripts, but should not be forced into a heavyweight architecture if a straightforward existing component works.

## 7. UX details that merit careful design

Suggested interaction:

1. Open the app; see a detailed Speed Editor layout and its current connection/mode.
2. Select **Default** or **Fn** layer, click a physical key in the diagram, or press **Learn** and press it on the device.
3. Choose an action from an action palette, drag it onto the key, or record a shortcut.
4. See a description of what the action will do *and* whether the original Resolve action will still fire. Flag any mapping that cannot be cleanly isolated in the chosen device mode.
5. Test with a live monitor, then save a named profile. An **Original / Disable extensions** control should always be accessible.

It may be helpful to show native mappings as *read-only* unless a verified interception path exists. Permit additive combinations only when they are safe. Offer the option to preserve the wheel entirely in every layer by default; advanced dial remapping should be explicitly opted into.

Consider the subtleties of chords and layers: order of key presses, press-versus-release triggers, stuck holds after device disconnect, tap-versus-hold Fn semantics, debounce, simultaneous keys, keyboard-layout differences, focused windows, and what happens if Resolve changes pages or keyboard shortcuts.

## 8. Conservative safety and recovery expectations

- **No device firmware flashing**, vendor firmware modification or unofficial firmware updater. No need to promise a hardware factory reset: if firmware was untouched, reverting should normally concern the application, driver state or configuration, not the device's firmware.
- Make any driver install/replacement, Windows filter driver, virtual device, admin privilege or external hardware step **explicit and opt-in**, with documented uninstall/recovery instructions. Avoid such dependencies in early tests if possible.
- Maintain an obvious bypass/stop control and an original-only mode. On crashes or disconnect, release any synthetic held keys; do not claim automatic native fallback without proving it.
- Run first experiments on a disposable Resolve project; avoid destructive keyboard commands during packet replay or compatibility testing.
- Be wary of intercepting shortcuts when an unintended application has focus. Avoid injecting actions until the target is known and authorised by the user.
- Keep recorded traffic local by default. Avoid retaining unique device identifiers, authentication material or other sensitive data unless genuinely needed, and redact shared logs.
- No license-incompatible code copying. Verify third-party project licenses, attributions and distribution obligations; seek permission where needed.

## 9. Suggested evidence-driven milestones

These are helpful investigation checkpoints, not an inflexible waterfall:

- [ ] **Reality check / source audit.** Examine implementation code, licenses, issues, supported devices and OS behaviour; identify the strongest candidate for reuse. Include the newer Go `speed-editor-client`, not only Unbound.
- [ ] **Baseline measurement.** Record Resolve version, firmware, USB/Bluetooth environment, original wheel feel, mode switching and key/LED behaviour. Separate hardware-dependent from mockable tasks.
- [ ] **Safe diagnostic prototype.** Implement or adapt a read-only event inspector, capture tools and repeatable test dataset. Establish what can be observed when Resolve owns the device.
- [ ] **Ownership feasibility proof.** Determine whether simultaneous read is possible and whether unwanted native actions can actually be prevented. Prototype only the minimum needed to answer this question.
- [ ] **Architecture decision.** Report objectively whether native+extensions, a limited additive mode, or exclusive custom mode is realistic on Windows. Give evidence and alternatives rather than promising the impossible.
- [ ] **Small useful vertical slice.** A graphical map, one custom Fn action, persistent profile, safe exit, and a demonstration of *actual* native-wheel preservation if supported.
- [ ] **Broader mapping and integrations.** Additional layers, action backends, shortcuts, GUI improvements and packaging as justified.
- [ ] **Regression and recovery tests.** Repeated launches, unplug/replug, Resolve started before/after the mapper, crashes, focus changes, wheel reversals, shuttle behaviour and recovery to stock.

**Suggested validation matrix:** Cut page and Edit page; jog, shuttle and scroll; slow and rapid wheel rotation; stop/centre transitions; modifier-held wheel; fast repeated button taps; key holds/releases; native LEDs/feedback; app switches; USB unplug/reconnect; startup order; exit/crash; Windows sleep/resume; an unchanged comparison install of Resolve.

If the full native-extension goal turns out to be too risky or expensive, please suggest an honest fallback that still gives value: for example, **perfectly native mode when Resolve is active, and a rich custom macro-pad mode when Resolve is closed**, plus any provably safe additive features. This would be preferable to a "native" label concealing degraded wheel behaviour.

## 10. How to approach decisions and reporting

The collaborating coding agent is invited to:

- Inspect the **actual source code** and current repositories, not simply their README summaries.
- Check whether an existing tool already satisfies the target or could be extended with little work; avoid reinventing a HID framework unnecessarily.
- Challenge USB/Windows/Resolve assumptions and investigate better alternatives not listed here.
- Clearly separate **verified facts**, **plausible hypotheses**, **untested risks** and **rejected paths**.
- Choose the cleanest, safest, maintainable architecture, even if it differs from the suggestions above.
- Build a proof of concept where it can establish feasibility, while respecting the user's physical hardware and avoiding invasive experiments without consent.
- Explain which aspects are independently reproducible from code/tests and which require an on-site test with a real Speed Editor and Resolve.
- Preserve source attribution and licensing; document any uncertain reuse rights.

### Core acceptance criteria

The most important user-facing promise, if technically achievable, is simple:

> **When no custom combination is invoked, the physical Speed Editor behaves exactly as it does in unmodified DaVinci Resolve — particularly the jog wheel. When a user chooses a custom combination, only its intended custom action happens. At any time, the user can remove the extension and return to normal, without touching device firmware.**

Please determine whether that promise is achievable rather than assuming it is. A clear, evidence-backed answer that it is only partly achievable is a successful research outcome; a fragile implementation that silently compromises the original experience is not.
