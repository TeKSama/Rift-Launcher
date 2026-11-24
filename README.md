# Rift Launcher 2.0 (alpha)

*made by TeK_Sama*

A Windows launcher for your **recomp** games (native PC ports such as TotK recomp or Savage Wind): a poster library, play time, mods, save backups, full controller support and Discord status.

> This branch is the **2.0 alpha**, rewritten in Rust. The stable version is still 1.x on the `main` branch. Expect bugs, and keep a copy of `%APPDATA%\RiftLauncher` if your library matters to you.

## Download

Get `RiftLauncherSetup-2.0.0-alpha.x.exe` from the [releases page](https://github.com/TeKSama/Rift-Launcher/releases) (it is marked as a pre-release) and run it. It works on Windows 10 and 11 and only needs Microsoft Edge, which Windows already has.

The alpha is never offered as an automatic update: 1.x users stay on 1.x until 2.0 is final. If you install the alpha over 1.x, your library, play time and settings are kept, and the alpha will update itself to the final 2.0 when it is out.

Rift Launcher does not include or download any game, ROM or key. Use your own dumps of games you own.

## What changes in 2.0

- The app is now a single Rust program. PowerShell is no longer used, so it starts faster and nothing is compiled on your PC at startup.
- On first launch, a welcome screen offers to preload known ports with their cover, sound and GitHub link. You only pick each game's `.exe`. You can open it again from Settings.
- Controllers are read natively: Xbox button, Home and capture buttons on Switch Pro, PS4 and PS5, and the Share button of Xbox Series controllers over Bluetooth.
- The installer checks GitHub first and installs the newest version if the one you downloaded is outdated.
- 1.x installs move to 2.0 by themselves when 2.0 is final, through the usual update button.

## Features

- **Poster library** (2:3 covers): built-in covers, your own images, or online search through SteamGridDB.
- **Play time** and last session.
- **Mods**: turn each mod on or off in one click. Nothing is deleted, disabled mods are moved to a folder next to it.
- **Save backups** in one click.
- **Port updates**: a badge appears when a port's GitHub page publishes a new version.
- **Controllers** (Xbox, PlayStation 4/5, Switch Pro): full navigation, matching button icons, **Big Picture** mode, **Home** button to switch between the game and the launcher, **capture** button for screenshots.
- **Discord status**: "Browsing the menus", then the game you launch.
- **6 languages**: English, Français, Español, 中文, हिन्दी, العربية.
- Log and one-click **bug report**.

## Is it safe? Why does Windows warn me?

The setup is **not code-signed** (a signing certificate costs money for a small free project), so Windows SmartScreen may show *"Windows protected your PC"*. Click **More info → Run anyway**.

Everything the app runs is in this repository:

| Path | What it does |
| --- | --- |
| `src/` | The app itself, in Rust: a small **local** web server (only reachable from your own PC, `localhost`, protected by a random token) that shows the interface in an Edge app window, launches your games, counts play time, manages mods and backups, reads controllers and sets the Discord status. |
| `app/web/` | The interface (HTML, CSS, JavaScript) and its translations. |
| `app/preset/preset.json` | The games offered by the welcome screen. |
| `app/RiftShot.cs` | Takes accurate HDR screenshots (DXGI desktop duplication). Compiled by Windows on your PC the first time it is needed. Without it, a regular screenshot is taken. |

**What it installs:** files in `%LOCALAPPDATA%\Programs\Rift Launcher`, a Start menu and Desktop shortcut, and an uninstall entry (Settings → Apps). No admin rights, no driver, no background service: it closes when you close its window and no game is running.

**Your data stays on your PC** in `%APPDATA%\RiftLauncher` (library, settings, covers, logs). Nothing is collected or sent anywhere.

**Network access**, only for these features:
- `github.com` / `api.github.com`: check for Rift Launcher updates and new versions of the ports you added.
- `steamgriddb.com`: online cover search, only if you add your own free API key.
- Discord: only through Discord's local connection on your PC (no internet request), only if Discord is open.

Each release lists the **SHA-256** of its setup in `latest.json`. Rift Launcher and its installer check it before installing; you can check it too with `Get-FileHash RiftLauncherSetup-x.y.z.exe` in PowerShell.

## Build from source

Install Rust from [rustup.rs](https://rustup.rs), then in this folder:

```
cargo build --release
```

Run `target\release\RiftLauncher.exe`. A build started from the `target` folder is a developer build: it keeps its data in a `dev-data` folder next to the project and uses port 47913, so it never touches an installed Rift Launcher.

## Uninstall

Settings → Apps → **Rift Launcher** → Uninstall. Your library in `%APPDATA%\RiftLauncher` is kept; delete that folder too if you want to remove everything.

## License

MIT, see [LICENSE](LICENSE). Game names and trademarks belong to their owners.
