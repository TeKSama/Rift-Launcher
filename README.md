# Rift Launcher

*made by TeK_Sama*

A Windows launcher for your **recomp** games (native PC ports such as TotK recomp, Savage Wind, SplatPC…): a poster library, play time, mods, save backups, full controller support and Discord status.

## Download

Get **`RiftLauncherSetup-x.y.z.exe`** from the [latest release](https://github.com/TeKSama/Rift-Launcher/releases/latest) and run it. Nothing else to install: it runs on Windows 10 and 11 with what Windows already provides (PowerShell and Microsoft Edge).

Rift Launcher does **not** include or download any game, ROM or key. Use your own dumps of games you own.

## Features

- **Poster library** (2:3 covers): built-in covers, your own images, or online search through SteamGridDB.
- **Play time** and last session (or the game's own counter when the port has one).
- **Mods**: turn each mod on or off in one click. Nothing is deleted, disabled mods are just moved to a folder next to it.
- **Save backups** in one click.
- **Port updates**: a badge appears when a port's GitHub page publishes a new version.
- **Controllers** (Xbox, PlayStation 4/5, Switch Pro): full navigation, matching button icons, **Big Picture** mode, **Home** button to switch between the game and the launcher, **capture** button for screenshots.
- **Discord status**: "Browsing the menus", then the game you launch.
- **Automatic updates** of Rift Launcher, offered at startup (you choose when).
- **6 languages**: English, Français, Español, 中文, हिन्दी, العربية.
- Log and one-click **bug report**.

## Is it safe? Why does Windows warn me?

The setup is **not code-signed** (a signing certificate costs money for a small free project), so Windows SmartScreen may show *"Windows protected your PC"*. Click **More info → Run anyway**.

Everything the app runs is in this repository, in plain readable files, so you can check it yourself:

| File | What it does |
| --- | --- |
| `app/RiftLauncher.ps1` | The app itself: a small **local** web server (only reachable from your own PC, `localhost`, protected by a random token) that shows the interface in an Edge app window, launches your games, counts play time, manages mods and backups. |
| `app/web/` | The interface (HTML, CSS, JavaScript) and its translations. |
| `app/RiftPad.cs` | Reads the controller's Home and capture buttons and brings the right window to the front. Compiled by Windows on your PC at startup. |
| `app/RiftShot.cs` | Takes accurate screenshots (DXGI desktop duplication). Compiled by Windows on your PC when first needed. |
| `app/Depannage.bat` | Troubleshooting: starts the app with a visible console. |

**What it installs:** files in `%LOCALAPPDATA%\Programs\Rift Launcher`, a Start menu and Desktop shortcut, and an uninstall entry (Settings → Apps). No admin rights, no driver, no background service: it closes when you close its window and no game is running.

**Your data stays on your PC** in `%APPDATA%\RiftLauncher` (library, settings, covers, logs). Nothing is collected or sent anywhere.

**Network access**, only for these features:
- `github.com` / `api.github.com`: check for Rift Launcher updates and new versions of the ports you added.
- `steamgriddb.com`: online cover search, only if you add your own free API key.
- Discord: only through Discord's local connection on your PC (no internet request), only if Discord is open.

Each release lists the **SHA-256** of its setup in `latest.json`. Rift Launcher checks it before installing an update; you can check it too with `Get-FileHash RiftLauncherSetup-x.y.z.exe` in PowerShell.

## Uninstall

Settings → Apps → **Rift Launcher** → Uninstall. Your library in `%APPDATA%\RiftLauncher` is kept; delete that folder too if you want to remove everything.

## License

MIT, see [LICENSE](LICENSE). Game names and trademarks belong to their owners.
