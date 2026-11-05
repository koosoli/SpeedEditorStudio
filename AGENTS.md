# Agent instructions

Speed Editor Studio: Rust workspace, Tauri 2 app in `apps/studio` (binary `se-studio`).
Windows is the main platform; macOS is untested. See README.md for the architecture.

## Builds are for supporters only

Ready-made builds go **only** into the private repository
`SpeedEditorStudio/SpeedEditorStudio-Builds` (releases tagged `v<version>`, e.g. `v0.2.0`).
Never attach a build to this public repo: no GitHub release here, no committed binaries, and
don't leave a workflow artifact on a public Actions run (anyone signed in can download it).

## Every new version: make a Windows and a macOS build

When a new version is made (or the user asks for builds), produce both zips in `dist/`
(gitignored) and hand them to the user, who adds them to the private release. Upload them
yourself only if the user asks (`gh release upload v<version> <zip> -R
SpeedEditorStudio/SpeedEditorStudio-Builds --clobber`).

GitHub CLI: `D:\tools\gh\bin\gh.exe`, config in `D:\tools\gh\config` (set
`$env:GH_CONFIG_DIR='D:\tools\gh\config'` if a shell doesn't have it). Logged in as koosoli.
Install new tools on drive D:, not C: (C: is nearly full).

### 1. Version bump

Set the same version in `Cargo.toml` (`[workspace.package] version`) and
`apps/studio/tauri.conf.json` (`version`), run `cargo build` so `Cargo.lock` follows, and
commit ("Version X.Y.Z; ..."). Push before the macOS build: it builds what's on GitHub.

### 2. Windows build (locally, on this PC)

```powershell
cargo test --workspace
cargo build --release -p se-studio
```

Then create `dist/SpeedEditorStudio-<version>-windows-x64/` with:

- `target/release/se-studio.exe`, renamed to `Speed Editor Studio.exe`
- `LICENSE` and `NOTICE` from the repo root
- `READ ME.txt`: copy it from the previous version's folder in `dist/`, change the version in
  the first line and replace the "New in" section with this version's user-visible changes

Zip the folder (with the folder inside) to `dist/SpeedEditorStudio-<version>-windows-x64.zip`
and report its SHA-256 (`Get-FileHash -Algorithm SHA256`).

### 3. macOS build (GitHub Actions, on a real Mac)

The workflow `.github/workflows/macos.yml` builds a universal app (Apple Silicon and Intel),
runs the tests, packages `SpeedEditorStudio-<version>-macos-universal.zip` (app, READ ME.txt,
LICENSE, NOTICE; its READ ME text lives in the workflow) and keeps it as an artifact for one
day. Free on this public repo; it takes about 15 minutes.

```powershell
gh workflow run macos.yml -R koosoli/SpeedEditorStudio --ref main
gh run list -R koosoli/SpeedEditorStudio --workflow macos.yml -L 1      # get the run id
gh run watch <run-id> -R koosoli/SpeedEditorStudio --exit-status
gh run download <run-id> -R koosoli/SpeedEditorStudio -D dist/macos-<version>
# then delete the artifact from the public run right away:
gh api repos/koosoli/SpeedEditorStudio/actions/runs/<run-id>/artifacts --jq '.artifacts[].id'
gh api -X DELETE repos/koosoli/SpeedEditorStudio/actions/artifacts/<artifact-id>
```

Check the zip contains `Speed Editor Studio.app` (with `Contents/_CodeSignature`, the ad-hoc
signature), `READ ME.txt`, `LICENSE` and `NOTICE`, and report its SHA-256. If the run fails,
read the log (`gh run view <run-id> --log-failed`), fix and run again. macOS code can't be
compiled on Windows (hidapi and objc2 need Apple's toolchain), so CI is the only check.

### 4. Hand over

Tell the user the paths of both zips and their SHA-256, and suggest release notes for the
private release: what's new, and that the macOS build is completely untested and not signed
by Apple (right-click > Open, then allow it under Accessibility).
