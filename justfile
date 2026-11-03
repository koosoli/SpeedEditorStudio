# Speed Editor Studio: build, test and package (macOS). Run `just` to list the recipes.
#
# Needs: Rust (rustup), Node.js (for the Tauri CLI, fetched on demand by npx) and,
# for the universal bundle, both Mac targets:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin

set shell := ["bash", "-euo", "pipefail", "-c"]

app       := "Speed Editor Studio"
bundle_id := "io.github.openspeededitor.studio"
tauri     := "npx --yes @tauri-apps/cli@2"
dist      := "dist"

# The recipes, in the order they are written here.
default:
    @just --list --unsorted

# Debug build of every crate.
build:
    cargo build --workspace

# Optimised build of every crate (target/release).
release:
    cargo build --workspace --release

# Unit tests of every crate.
test:
    cargo test --workspace

# Lints, with warnings as errors.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Tests and lints, as before a commit.
check: test clippy

# Presses the Mac's mute key twice: the keyboard output needs the Accessibility permission.
test-keys:
    cargo test -p runtime -- --ignored media_keys

# Starts the app from source (release build). Accessibility is asked for the terminal you run it from.
run:
    cargo run --release -p se-studio

# Command-line runner without a window, e.g. `just run-mapper profiles/example-macos.toml --seconds 30`.
run-mapper *args="profiles/example-macos.toml":
    cargo run --release -p se-mapper -- {{args}}

# Lists the Speed Editor's USB/Bluetooth interfaces and watches its reports (read-only).
probe *args:
    cargo run --release -p se-probe -- {{args}}

# Rebuilds apps/studio/icons/icon.icns from icon.png (only needed after changing the icon).
icons:
    #!/usr/bin/env bash
    set -euo pipefail
    cd apps/studio/icons
    set_dir="$(mktemp -d)/icon.iconset"
    mkdir -p "$set_dir"
    for spec in 16:icon_16x16 32:icon_16x16@2x 32:icon_32x32 64:icon_32x32@2x \
                128:icon_128x128 256:icon_128x128@2x 256:icon_256x256 512:icon_256x256@2x \
                512:icon_512x512 1024:icon_512x512@2x; do
        sips -z "${spec%%:*}" "${spec%%:*}" icon.png --out "$set_dir/${spec##*:}.png" > /dev/null
    done
    iconutil -c icns "$set_dir" -o icon.icns
    echo "wrote apps/studio/icons/icon.icns"

# The .app for this Mac's architecture, ad-hoc signed, copied to dist/.
bundle:
    cd apps/studio && {{tauri}} build --bundles app
    just _collect target/release/bundle/macos

# The .app for Apple Silicon and Intel in one, ad-hoc signed, copied to dist/.
bundle-universal:
    cd apps/studio && {{tauri}} build --target universal-apple-darwin --bundles app
    just _collect target/universal-apple-darwin/release/bundle/macos

# Zip of the bundle in dist/, for sending to someone (keeps the signature and permissions).
zip: bundle
    cd {{dist}} && rm -f "SpeedEditorStudio-macos.zip" && ditto -c -k --keepParent "{{app}}.app" "SpeedEditorStudio-macos.zip"
    @echo "{{dist}}/SpeedEditorStudio-macos.zip"

# Copies the built bundle to dist/ and checks its signature.
_collect from:
    mkdir -p "{{dist}}"
    rm -rf "{{dist}}/{{app}}.app"
    ditto "{{from}}/{{app}}.app" "{{dist}}/{{app}}.app"
    codesign --verify --deep --strict "{{dist}}/{{app}}.app"
    @echo "{{dist}}/{{app}}.app"

# Opens the bundle from dist/.
open:
    open "{{dist}}/{{app}}.app"

# Installs the bundle from dist/ into /Applications.
install:
    rm -rf "/Applications/{{app}}.app"
    ditto "{{dist}}/{{app}}.app" "/Applications/{{app}}.app"
    @echo "installed /Applications/{{app}}.app"

# Removes the app from /Applications, its login item and its settings (asks first).
uninstall:
    #!/usr/bin/env bash
    set -euo pipefail
    read -r -p "Remove {{app}}, its login item and ~/Library/Application Support/OpenSpeedEditor? [y/N] " answer
    [[ "$answer" == [yY]* ]] || exit 0
    launchctl bootout "gui/$(id -u)/{{bundle_id}}" 2> /dev/null || true
    rm -rf "/Applications/{{app}}.app" "$HOME/Library/LaunchAgents/{{bundle_id}}.plist" \
           "$HOME/Library/Application Support/OpenSpeedEditor"
    tccutil reset All "{{bundle_id}}" || true

# Forgets the Accessibility and Input Monitoring grants. A rebuilt, ad-hoc signed app counts as a new app, so do this when macOS keeps saying it is allowed but the keys don't arrive.
reset-permissions:
    tccutil reset Accessibility "{{bundle_id}}"
    tccutil reset ListenEvent "{{bundle_id}}"

# Removes build output and dist/.
clean:
    cargo clean
    rm -rf "{{dist}}"
