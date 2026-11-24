#![cfg_attr(not(windows), allow(dead_code))]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod discord;
mod features;
mod franchises;
mod json;
mod net;
mod pad;
mod welcome;
#[cfg(any(windows, feature = "wincheck"))]
mod win;

use json::{parse, Json};
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PORT: u16 = 47903;
const DEV_PORT: u16 = 47913;
const IDLE_EXIT_SECS: u64 = 150;
const MAX_BACKUPS: usize = 10;
const DISABLED_SUFFIX: &str = "_desactives";
const EDIT_FIELDS: &[&str] = &["name", "exePath", "args", "env", "saveDir", "modsDir", "version", "repoUrl", "franchise"];
const PREF_SLIDERS: &[(&str, f64)] = &[("volume", 80.0), ("uiVolume", 70.0), ("gameVolume", 100.0)];
const PREF_TOGGLES: &[(&str, bool)] = &[
    ("hoverSounds", true),
    ("intro", true),
    ("debugLog", false),
    ("discord", true),
    ("recompCheck", true),
    ("homeToggle", true),
    ("captureButton", true),
];
const LANGS: &[&str] = &["auto", "fr", "en", "es", "zh", "hi", "ar"];
const THEMES: &[&str] = &["rift", "hyrule", "nuit"];
pub(crate) const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "svg"];
const NOT_PORTED: &str = "Pas encore disponible dans la version Rust de Rift Launcher.";

struct Run {
    started_ms: i64,
    child: Child,
    last_tick: Instant,
    stop_requested: bool,
}

enum Bg {
    Update(Json),
    Recomp(String, Json),
}

struct App {
    dev: bool,
    pad: Option<pad::Pad>,
    capture: Json,
    discord: Option<discord::Conn>,
    discord_shown: String,
    discord_err: String,
    discord_retry: Instant,
    started_ms: i64,
    update: Json,
    recomp: HashMap<String, Json>,
    tx: Sender<Bg>,
    rx: Receiver<Bg>,
    next_recomp: Instant,
    quit: bool,
    root: PathBuf,
    data: PathBuf,
    version: String,
    token: String,
    games: Vec<Json>,
    running: HashMap<String, Run>,
    last_seen: Instant,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400) as u32;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe as i64 + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, rem / 3600, rem / 60 % 60, rem % 60)
}

pub(crate) fn stamp(fmt_day: bool) -> String {
    let (y, m, d, h, mi, s) = civil(now_ms() / 1000);
    if fmt_day {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02}_{h:02}-{mi:02}-{s:02}")
    }
}

pub(crate) fn new_id() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_i64(now_ms());
    let a = h.finish();
    let mut h2 = RandomState::new().build_hasher();
    h2.write_u64(a ^ std::process::id() as u64);
    let b = h2.finish();
    let x = ((a as u128) << 64) | b as u128;
    let s = format!("{x:032x}");
    format!("{}-{}-4{}-{}-{}", &s[0..8], &s[8..12], &s[13..16], &s[16..20], &s[20..32])
}

pub(crate) fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        let c = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub(crate) fn safe_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || (c as u32) < 0x20 { '_' } else { c }).collect();
    let n = n.trim().trim_end_matches('.').to_string();
    if n.is_empty() {
        "jeu".to_string()
    } else {
        n
    }
}

fn haystack(g: &Json) -> String {
    let h = format!(" {} {} ", g.str("name"), g.str("exePath")).to_lowercase();
    let mut out = String::new();
    for c in h.chars() {
        if "_-\\/".contains(c) {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn has_word(hay: &str, words: &[&str]) -> bool {
    let bytes = hay.as_bytes();
    let edge = |i: Option<usize>| match i {
        None => true,
        Some(i) => bytes.get(i).map_or(true, |b| !b.is_ascii_alphanumeric()),
    };
    words.iter().any(|w| {
        hay.match_indices(w).any(|(i, _)| edge(i.checked_sub(1)) && edge(Some(i + w.len())))
    })
}

fn franchise(g: &Json) -> Option<(String, String)> {
    let chosen = g.str("franchise");
    if !chosen.is_empty() {
        return Some(
            franchises::FRANCHISES
                .iter()
                .find(|f| f.0 == chosen)
                .map(|f| (f.0.to_string(), f.1.to_string()))
                .unwrap_or((slug(&chosen), chosen)),
        );
    }
    let hay = haystack(g);
    franchises::FRANCHISES.iter().find(|f| has_word(&hay, f.2)).map(|f| (f.0.to_string(), f.1.to_string()))
}

fn builtin_key(g: &Json) -> Option<&'static str> {
    let hay = haystack(g);
    franchises::BUILTIN_COVERS.iter().find(|c| has_word(&hay, c.1)).map(|c| c.0)
}

pub(crate) fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

pub(crate) fn find_asset(dir: &Path, names: &[String], exts: &[&str]) -> Option<PathBuf> {
    let files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && exts.contains(&ext_of(p).as_str()))
        .collect();
    names.iter().filter(|n| !n.is_empty()).find_map(|n| {
        files.iter().find(|f| f.file_stem().and_then(|s| s.to_str()).map(slug).as_deref() == Some(n.as_str())).cloned()
    })
}

fn mtime_tag(p: &Path) -> String {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.find('=').filter(|&i| i > 0).map(|i| (l[..i].trim().to_string(), l[i + 1..].trim().to_string())))
        .collect()
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

pub(crate) fn open_path(target: &str, select: bool) {
    #[cfg(windows)]
    {
        let mut c = Command::new("explorer.exe");
        if select {
            c.arg(format!("/select,{target}"));
        } else {
            c.arg(target);
        }
        let _ = c.spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = select;
        let _ = Command::new("xdg-open").arg(target).spawn();
    }
}

pub(crate) fn open_url(url: &str) {
    #[cfg(windows)]
    {
        let _ = Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

pub(crate) fn err(msg: &str) -> Json {
    Json::obj().with("error", msg)
}

pub(crate) fn ok() -> Json {
    Json::obj().with("ok", true)
}

impl App {
    fn new() -> App {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
        let root = std::env::var_os("RIFT_APP_DIR")
            .map(PathBuf::from)
            .or_else(|| exe_dir.ancestors().find(|d| d.join("web").is_dir()).map(Path::to_path_buf))
            .or_else(|| exe_dir.ancestors().map(|d| d.join("app")).find(|d| d.join("web").is_dir()))
            .unwrap_or(exe_dir.clone());
        let dev = std::env::var_os("RIFT_DEV").is_some() || exe_dir.components().any(|c| c.as_os_str() == "target");
        let personal = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("RiftLauncher"));
        let data = if dev {
            let d = root.parent().unwrap_or(&root).join("dev-data");
            if !d.exists() {
                if let Some(p) = personal.as_ref().filter(|p| p.is_dir()) {
                    for f in ["library.json", "settings.json"] {
                        let _ = fs::create_dir_all(&d).and_then(|_| fs::copy(p.join(f), d.join(f)));
                    }
                    for sub in ["jaquettes", "sons", "logos"] {
                        let _ = copy_dir(&p.join(sub), &d.join(sub));
                    }
                }
            }
            d
        } else if root.join("portable.txt").exists() || exe_dir.join("portable.txt").exists() {
            root.join("data")
        } else if let Some(p) = personal {
            p
        } else {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".rift-launcher")
        };
        let version = fs::read_to_string(root.join("version.txt")).map(|s| s.trim().to_string()).unwrap_or_else(|_| "0.0.0".into());
        let (tx, rx) = channel();
        let app = App { pad: None, capture: Json::Null, discord: None, discord_shown: String::new(), discord_err: String::new(), discord_retry: Instant::now(), started_ms: now_ms(), update: Json::Null, recomp: HashMap::new(), tx, rx, next_recomp: Instant::now(), quit: false, dev, root, data, version, token: String::new(), games: Vec::new(), running: HashMap::new(), last_seen: Instant::now() };
        for d in [app.data.clone(), app.covers(), app.covers().join("jeux"), app.logos(), app.sounds(), app.data.join("logs")] {
            let _ = fs::create_dir_all(d);
        }
        app
    }

    fn port(&self) -> u16 {
        if self.dev { DEV_PORT } else { PORT }
    }

    fn web(&self) -> PathBuf {
        self.root.join("web")
    }
    fn covers(&self) -> PathBuf {
        self.data.join("jaquettes")
    }
    fn logos(&self) -> PathBuf {
        self.covers().join("logos")
    }
    fn sounds(&self) -> PathBuf {
        self.data.join("sons")
    }
    fn lib_file(&self) -> PathBuf {
        self.data.join("library.json")
    }
    fn settings_file(&self) -> PathBuf {
        self.data.join("settings.json")
    }
    fn backups(&self) -> PathBuf {
        self.data.join("sauvegardes")
    }
    fn captures(&self) -> PathBuf {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
        PathBuf::from(home).join("Pictures").join("Rift Launcher")
    }

    fn log(&self, level: &str, msg: &str) {
        let (_, _, _, h, mi, s) = civil(now_ms() / 1000);
        let line = format!("[{} {h:02}:{mi:02}:{s:02}] [{level:<5}] [rust] {msg}\r\n", stamp(true));
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.data.join("logs").join(format!("rift-{}.log", stamp(true)))) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn new_game(src: &Json, keep_stats: bool) -> Json {
        let mut g = Json::obj();
        let id = src.str("id");
        g.set("id", if id.is_empty() { new_id() } else { id }.into());
        for f in EDIT_FIELDS {
            g.set(f, src.str(f).trim().to_string().into());
        }
        if g.str("name").is_empty() {
            g.set("name", "Sans nom".into());
        }
        for f in ["cover", "sound", "coverRemote"] {
            g.set(f, src.str(f).into());
        }
        let keep = |k: &str| keep_stats && src.num(k) != 0.0;
        g.set("playSeconds", if keep("playSeconds") { src.num("playSeconds") } else { 0.0 }.into());
        g.set("lastPlayed", if keep("lastPlayed") { Json::Num(src.num("lastPlayed")) } else { Json::Null });
        g.set("sessions", if keep("sessions") { src.num("sessions") } else { 0.0 }.into());
        g
    }

    fn load_library(&mut self) {
        if let Ok(raw) = fs::read_to_string(self.lib_file()) {
            match parse(&raw) {
                Ok(v) => {
                    self.games = v.get("games").map(|a| a.items().iter().map(|g| App::new_game(g, true)).collect()).unwrap_or_default();
                    return;
                }
                Err(e) => self.log("ERROR", &format!("library.json illisible : {e}")),
            }
        }
        self.save_library();
    }

    fn save_library(&self) {
        let body = Json::obj().with("games", Json::Arr(self.games.clone())).to_pretty();
        let tmp = self.lib_file().with_extension("json.tmp");
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, self.lib_file());
        }
    }

    fn game(&self, id: &str) -> Option<&Json> {
        self.games.iter().find(|g| g.str("id") == id)
    }

    fn game_mut(&mut self, id: &str) -> Option<&mut Json> {
        self.games.iter_mut().find(|g| g.str("id") == id)
    }

    fn asset_names(&self, g: &Json, with_franchise: bool) -> Vec<String> {
        let mut names = vec![slug(&g.str("name")), builtin_key(g).unwrap_or("").to_string()];
        if with_franchise {
            if let Some((k, _)) = franchise(g) {
                names.push(k);
            }
        }
        names
    }

    fn resolve_cover(&self, g: &Json) -> Option<(PathBuf, &'static str)> {
        let own = PathBuf::from(g.str("cover"));
        if !g.str("cover").is_empty() {
            if own.is_file() {
                return Some((own, "perso"));
            }
            if let Some(name) = own.file_name() {
                let local = self.covers().join("jeux").join(name);
                if local.is_file() {
                    return Some((local, "perso"));
                }
            }
        }
        if let Some(p) = find_asset(&self.covers(), &self.asset_names(g, true), IMAGE_EXT) {
            return Some((p, "dossier"));
        }
        let key = builtin_key(g)?;
        ["webp", "jpg", "png", "svg"].iter().map(|e| self.web().join("covers").join(format!("{key}.{e}"))).find(|p| p.is_file()).map(|p| (p, "integree"))
    }

    fn resolve_logo(&self, g: &Json) -> Option<PathBuf> {
        find_asset(&self.logos(), &self.asset_names(g, false), &["png", "webp", "svg"])
    }

    fn game_shots(&self, g: &Json) -> Vec<PathBuf> {
        let dir = self.captures().join(safe_name(&g.str("name")));
        let mut shots: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && ["png", "jpg", "jpeg"].contains(&ext_of(p).as_str()))
                    .map(|p| (fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH), p))
                    .collect()
            })
            .unwrap_or_default();
        shots.sort_by(|a, b| b.0.cmp(&a.0));
        shots.into_iter().take(12).map(|s| s.1).collect()
    }

    fn snapshot(&self) -> Json {
        let games: Vec<Json> = self
            .games
            .iter()
            .map(|g| {
                let id = g.str("id");
                let mut v = g.clone();
                let f = franchise(g);
                v.set("franchiseKey", f.as_ref().map(|f| f.0.clone()).unwrap_or_default().into());
                v.set("franchiseLabel", f.map(|f| f.1).unwrap_or_default().into());
                let c = self.resolve_cover(g);
                v.set("coverUrl", c.as_ref().map(|c| format!("/img?id={id}&v={}", mtime_tag(&c.0))).unwrap_or_default().into());
                v.set("coverSource", c.map(|c| c.1).unwrap_or("").into());
                let sounds = self.resolve_sounds(g);
                let urls: Vec<Json> = sounds.iter().enumerate().map(|(i, p)| Json::Str(format!("/snd?id={id}&n={i}&v={}", mtime_tag(p)))).collect();
                v.set("soundUrl", urls.first().cloned().unwrap_or(Json::Str(String::new())));
                v.set("soundSource", if sounds.is_empty() { "" } else { "dossier" }.into());
                v.set("soundUrls", Json::Arr(urls));
                v.set("logoUrl", self.resolve_logo(g).map(|p| format!("/logo?id={id}&v={}", mtime_tag(&p))).unwrap_or_default().into());
                let shots = self.game_shots(g);
                v.set("shots", Json::Arr(shots.iter().enumerate().map(|(i, p)| Json::Str(format!("/shot?id={id}&n={i}&v={}", mtime_tag(p)))).collect()));
                v.set("recomp", self.recomp_view(g));
                v.set("playSource", "launcher".into());
                v
            })
            .collect();
        Json::obj()
            .with("games", Json::Arr(games))
            .with("running", Json::Arr(self.running.keys().map(|k| Json::Str(k.clone())).collect()))
            .with("version", self.version.clone())
            .with("update", self.update.clone())
            .with("capture", self.capture.clone())
            .with("welcome", self.welcome_wanted())
    }

    fn add_time(&mut self, id: &str) {
        let Some(r) = self.running.get_mut(id) else { return };
        let secs = r.last_tick.elapsed().as_secs_f64();
        r.last_tick = Instant::now();
        if let Some(g) = self.games.iter_mut().find(|g| g.str("id") == id) {
            let total = g.num("playSeconds") + secs;
            g.set("playSeconds", total.into());
            g.set("lastPlayed", now_ms().into());
        }
    }

    fn tick(&mut self) {
        let mut changed = false;
        for id in self.running.keys().cloned().collect::<Vec<_>>() {
            let exited = self.running.get_mut(&id).map(|r| matches!(r.child.try_wait(), Ok(Some(_)) | Err(_))).unwrap_or(false);
            let due = self.running.get(&id).map(|r| r.last_tick.elapsed() >= Duration::from_secs(30)).unwrap_or(false);
            if exited || due {
                self.add_time(&id);
                changed = true;
            }
            if exited {
                if let Some(r) = self.running.remove(&id) {
                    let name = self.game(&id).map(|g| g.str("name")).unwrap_or_default();
                    self.log("INFO", &format!("Fermeture : {name}{}", if r.stop_requested { " (arrêté depuis Rift Launcher)" } else { "" }));
                }
            }
        }
        if changed {
            self.save_library();
        }
    }

    fn play(&mut self, id: &str) -> Json {
        let Some(g) = self.game(id).cloned() else { return err("Jeu introuvable.") };
        if self.running.contains_key(id) {
            return err("Ce jeu est déjà lancé.");
        }
        let exe = PathBuf::from(g.str("exePath"));
        if g.str("exePath").is_empty() || !exe.is_file() {
            self.log("WARN", &format!("Lancement impossible, fichier introuvable : {}", g.str("exePath")));
            return err("Le fichier du jeu est introuvable. Règle son emplacement dans les paramètres du jeu.");
        }
        let mut cmd = Command::new(&exe);
        cmd.current_dir(exe.parent().unwrap_or(Path::new(".")));
        cmd.envs(parse_env(&g.str("env")));
        let args = g.str("args");
        if !args.is_empty() {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.raw_arg(&args);
            }
            #[cfg(not(windows))]
            cmd.args(args.split_whitespace());
        }
        match cmd.spawn() {
            Ok(child) => {
                self.log("INFO", &format!("Lancement : {} | PID {}", g.str("name"), child.id()));
                self.running.insert(id.to_string(), Run { started_ms: now_ms(), child, last_tick: Instant::now(), stop_requested: false });
                if let Some(g) = self.game_mut(id) {
                    let n = g.num("sessions") + 1.0;
                    g.set("sessions", n.into());
                    g.set("lastPlayed", now_ms().into());
                }
                self.save_library();
                ok()
            }
            Err(e) => err(&format!("Impossible de lancer le jeu : {e}")),
        }
    }

    fn stop(&mut self, id: &str) -> Json {
        if let Some(r) = self.running.get_mut(id) {
            r.stop_requested = true;
            #[cfg(windows)]
            {
                let mut c = Command::new("taskkill.exe");
                net::hide(&mut c);
                let _ = c.args(["/PID", &r.child.id().to_string(), "/T", "/F"]).status();
            }
            #[cfg(not(windows))]
            {
                let _ = r.child.kill();
            }
        }
        ok()
    }

    fn backup(&self, id: &str) -> Json {
        let Some(g) = self.game(id) else { return err("Jeu introuvable.") };
        let save = PathBuf::from(g.str("saveDir"));
        if g.str("saveDir").is_empty() || !save.is_dir() {
            return err("Choisis d'abord le dossier de sauvegarde du jeu dans ses paramètres.");
        }
        let game_dir = self.backups().join(safe_name(&g.str("name")));
        let dest = game_dir.join(stamp(false));
        if let Err(e) = copy_dir(&save, &dest) {
            return err(&format!("Sauvegarde impossible : {e}"));
        }
        let mut all: Vec<PathBuf> = fs::read_dir(&game_dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        all.sort();
        while all.len() > MAX_BACKUPS {
            let _ = fs::remove_dir_all(all.remove(0));
        }
        self.log("INFO", &format!("Sauvegarde : {}", dest.display()));
        ok().with("path", dest.to_string_lossy().to_string())
    }

    fn mod_dirs(g: &Json) -> Option<(PathBuf, PathBuf)> {
        let d = g.str("modsDir");
        let on = PathBuf::from(d.trim_end_matches(['\\', '/']));
        if d.is_empty() {
            return None;
        }
        let leaf = on.file_name()?.to_string_lossy().to_string();
        let off = on.with_file_name(format!("{leaf}{DISABLED_SUFFIX}"));
        Some((on, off))
    }

    fn list_mods(&self, id: &str) -> Json {
        let Some(g) = self.game(id) else { return err("Jeu introuvable.") };
        let Some((on, off)) = App::mod_dirs(g) else { return err("Choisis d'abord le dossier des mods dans les paramètres du jeu.") };
        if !on.is_dir() {
            return err(&format!("Le dossier des mods n'existe pas : {}", on.display()));
        }
        let mut mods = Vec::new();
        for (dir, enabled) in [(&on, true), (&off, false)] {
            for e in fs::read_dir(dir).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let date = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
                mods.push(Json::obj().with("name", name).with("enabled", enabled).with("isDir", e.path().is_dir()).with("date", date));
            }
        }
        mods.sort_by_key(|m| m.str("name").to_lowercase());
        Json::obj().with("dir", on.to_string_lossy().to_string()).with("mods", Json::Arr(mods))
    }

    fn set_mod(&self, id: &str, name: &str, enable: bool) -> Json {
        let Some(g) = self.game(id) else { return err("Jeu introuvable.") };
        let Some((on, off)) = App::mod_dirs(g) else { return err("Aucun dossier des mods.") };
        if name.is_empty() || name == "." || name == ".." || name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
            return err("Nom de mod invalide.");
        }
        let (from, to) = if enable { (off, on) } else { (on, off) };
        let (src, dst) = (from.join(name), to.join(name));
        if !src.exists() {
            return err("Ce mod a été déplacé ou supprimé. Actualise la liste.");
        }
        if dst.exists() {
            return err(&format!("Un mod du même nom existe déjà de l'autre côté : {name}"));
        }
        if self.running.contains_key(id) {
            return err("Ferme le jeu avant de changer ses mods.");
        }
        let _ = fs::create_dir_all(&to);
        match fs::rename(&src, &dst) {
            Ok(_) => ok(),
            Err(e) => err(&format!("Impossible de déplacer le mod : {e}")),
        }
    }

    fn settings(&self) -> Json {
        fs::read_to_string(self.settings_file()).ok().and_then(|t| parse(&t).ok()).filter(|v| matches!(v, Json::Obj(_))).unwrap_or_else(Json::obj)
    }

    fn prefs(&self) -> Json {
        let saved = self.settings().get("prefs").cloned().unwrap_or_else(Json::obj);
        let mut p = Json::obj();
        for (k, d) in PREF_SLIDERS {
            p.set(k, saved.get(k).cloned().unwrap_or(Json::Num(*d)));
        }
        for (k, d) in PREF_TOGGLES {
            p.set(k, saved.bool(k).unwrap_or(*d).into());
        }
        let lang = saved.str("lang");
        p.set("lang", if LANGS.contains(&lang.as_str()) { lang } else { "auto".into() }.into());
        let theme = saved.str("theme");
        p.set("theme", if THEMES.contains(&theme.as_str()) { theme } else { "rift".into() }.into());
        p
    }

    fn save_prefs(&self, incoming: &Json) -> Json {
        let mut p = self.prefs();
        for (k, _) in PREF_SLIDERS {
            if let Some(Json::Num(n)) = incoming.get(k) {
                p.set(k, n.clamp(0.0, 100.0).round().into());
            }
        }
        for (k, _) in PREF_TOGGLES {
            if let Some(b) = incoming.bool(k) {
                p.set(k, b.into());
            }
        }
        for (k, allowed) in [("lang", LANGS), ("theme", THEMES)] {
            let v = incoming.str(k);
            if allowed.contains(&v.as_str()) {
                p.set(k, v.into());
            }
        }
        let mut s = self.settings();
        s.set("prefs", p.clone());
        let _ = fs::write(self.settings_file(), s.to_pretty());
        p
    }

    fn api(&mut self, name: &str, body: &Json) -> Json {
        if let Some(v) = self.api_welcome(name, body) {
            return v;
        }
        if let Some(v) = self.api_more(name, body) {
            return v;
        }
        let id = body.str("id");
        match name {
            "library" => self.snapshot(),
            "save" => {
                let n = body.str("name").trim().to_string();
                if n.is_empty() {
                    return err("Donne un nom au jeu.");
                }
                if let Some(g) = self.game_mut(&id) {
                    for f in EDIT_FIELDS {
                        g.set(f, body.str(f).trim().to_string().into());
                    }
                } else {
                    self.games.push(App::new_game(body, false));
                }
                self.save_library();
                self.start_recomp_checks(false);
                self.snapshot()
            }
            "delete" => {
                if self.running.contains_key(&id) {
                    return err("Ferme le jeu avant de le retirer.");
                }
                self.games.retain(|g| g.str("id") != id);
                self.save_library();
                self.snapshot()
            }
            "play" => self.play(&id),
            "stop" => self.stop(&id),
            "backup" => self.backup(&id),
            "openBackups" => {
                if let Some(g) = self.game(&id) {
                    let dir = self.backups().join(safe_name(&g.str("name")));
                    let _ = fs::create_dir_all(&dir);
                    open_path(&dir.to_string_lossy(), false);
                }
                ok()
            }
            "openFolder" => {
                if let Some(g) = self.game(&id).filter(|g| Path::new(&g.str("exePath")).is_file()) {
                    open_path(&g.str("exePath"), true);
                }
                ok()
            }
            "openRepo" => {
                if let Some(g) = self.game(&id).filter(|g| g.str("repoUrl").starts_with("https://")) {
                    open_url(&g.str("repoUrl"));
                }
                ok()
            }
            "mods" => self.list_mods(&id),
            "modSet" => self.set_mod(&id, &body.str("name"), body.bool("enabled").unwrap_or(false)),
            "openMods" => {
                if let Some((on, _)) = self.game(&id).and_then(App::mod_dirs).filter(|d| d.0.is_dir()) {
                    open_path(&on.to_string_lossy(), false);
                }
                ok()
            }
            "openCaptures" => {
                let dir = self.game(&id).map(|g| self.captures().join(safe_name(&g.str("name")))).unwrap_or_else(|| self.captures());
                let _ = fs::create_dir_all(&dir);
                open_path(&dir.to_string_lossy(), false);
                ok()
            }
            "settings" => {
                let s = self.settings();
                Json::obj()
                    .with("appVersion", self.version.clone())
                    .with("logDir", self.data.join("logs").to_string_lossy().to_string())
                    .with("prefs", self.prefs())
                    .with("hasKey", !s.str("sgdbKey").is_empty())
                    .with("discordId", s.str("discordId"))
                    .with("discordBuiltin", true)
                    .with("discordBuiltinId", discord::BUILTIN_ID)
                    .with("discordConnected", self.discord.is_some())
                    .with("coverDir", self.covers().to_string_lossy().to_string())
                    .with("soundDir", self.sounds().to_string_lossy().to_string())
                    .with(
                        "franchises",
                        Json::Arr(franchises::FRANCHISES.iter().map(|f| Json::obj().with("key", f.0).with("label", f.1)).collect()),
                    )
            }
            "prefsSave" => {
                let p = self.save_prefs(body.get("prefs").unwrap_or(&Json::Null));
                self.discord_reset();
                ok().with("prefs", p)
            }
            "log" => {
                let level = body.str("level");
                let level = if ["INFO", "WARN", "ERROR"].contains(&level.as_str()) { level } else { "ERROR".into() };
                let msg: String = format!("{} {}", body.str("message"), body.str("stack")).chars().take(2000).collect();
                self.log(&level, &format!("[interface] {}", msg.trim()));
                ok()
            }
            "version" => Json::obj().with("version", self.version.clone()),
            "bye" => {
                self.last_seen = Instant::now() - Duration::from_secs(IDLE_EXIT_SECS - 5);
                ok()
            }
            "openLogs" => {
                open_path(&self.data.join("logs").to_string_lossy(), false);
                ok()
            }
            "openAssets" => {
                let dir = if body.str("kind") == "sounds" { self.sounds() } else { self.covers() };
                open_path(&dir.to_string_lossy(), false);
                ok()
            }
            "toggleFocus" | "fullscreen" => {
                let r = self.pad_ask(if name == "toggleFocus" { "toggle" } else { "f11" }, 4).unwrap_or_else(|| "indisponible".into());
                ok().with("result", r)
            }
            "capture" => {
                let f = self.pad_ask("capture", 10).unwrap_or_default();
                let file = Path::new(&f).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                Json::obj().with("ok", !f.is_empty()).with("file", file)
            }
            _ => err(NOT_PORTED),
        }
    }

    fn asset(&self, path: &str, query: &HashMap<String, String>) -> Option<PathBuf> {
        let n: usize = query.get("n").and_then(|n| n.parse().ok()).unwrap_or(0);
        if path == "/preset" {
            return self.preset_asset(n, query.get("k").map(String::as_str).unwrap_or(""));
        }
        let g = self.game(query.get("id").map(String::as_str).unwrap_or(""))?;
        match path {
            "/img" => self.resolve_cover(g).map(|c| c.0),
            "/logo" => self.resolve_logo(g),
            "/shot" => self.game_shots(g).into_iter().nth(n),
            "/snd" => self.resolve_sounds(g).into_iter().nth(n),
            _ => None,
        }
    }
}

fn mime(p: &Path) -> &'static str {
    match ext_of(p).as_str() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "json" => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn respond(stream: &mut TcpStream, status: u16, ctype: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn send_json(stream: &mut TcpStream, status: u16, v: &Json) {
    respond(stream, status, "application/json; charset=utf-8", v.to_string().as_bytes());
}

fn handle(app: &mut App, mut stream: TcpStream) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 65536 {
            return;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let method = first.next().unwrap_or("").to_string();
    let target = first.next().unwrap_or("/").to_string();
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0).min(10 << 20);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    let (path, qs) = target.split_once('?').unwrap_or((&target, ""));
    let query: HashMap<String, String> = qs.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (url_decode(k), url_decode(v))).collect();

    if !headers.get("host").map(|h| h.starts_with("localhost")).unwrap_or(false) {
        return send_json(&mut stream, 403, &err("forbidden"));
    }
    let statics = ["/", "/index.html", "/style.css", "/app.js", "/i18n.js", "/i18n-dict.js", "/logo.svg"];
    if statics.contains(&path) {
        let file = app.web().join(if path == "/" { "index.html" } else { &path[1..] });
        return match fs::read(&file) {
            Ok(bytes) => respond(&mut stream, 200, mime(&file), &bytes),
            Err(_) => send_json(&mut stream, 404, &err("not found")),
        };
    }
    if ["/img", "/logo", "/shot", "/snd", "/preset"].contains(&path) {
        return match app.asset(path, &query).and_then(|p| fs::read(&p).ok().map(|b| (p, b))) {
            Some((p, bytes)) => respond(&mut stream, 200, mime(&p), &bytes),
            None => send_json(&mut stream, 404, &err("not found")),
        };
    }
    let Some(api) = path.strip_prefix("/api/") else { return send_json(&mut stream, 404, &err("not found")) };
    let token = headers.get("x-token").cloned().or_else(|| query.get("t").cloned()).unwrap_or_default();
    if token != app.token || method != "POST" {
        return send_json(&mut stream, 403, &err("forbidden"));
    }
    app.last_seen = Instant::now();
    let text = String::from_utf8_lossy(&body).to_string();
    let parsed = if text.trim().is_empty() { Json::obj() } else { parse(&text).unwrap_or_else(|_| Json::obj()) };
    let out = app.api(api, &parsed);
    send_json(&mut stream, 200, &out);
}

pub(crate) fn open_window(app: &App) {
    let url = format!("http://localhost:{}/?t={}", app.port(), app.token);
    #[cfg(windows)]
    {
        let profile = app.data.join("edge-app");
        let args = [
            format!("--app={url}"),
            "--window-size=1200,800".to_string(),
            format!("--user-data-dir={}", profile.display()),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            "--disable-sync".to_string(),
            "--autoplay-policy=no-user-gesture-required".to_string(),
        ];
        let mut c = Command::new("cmd");
        net::hide(&mut c);
        let started = c.args(["/C", "start", "", "msedge.exe"]).args(&args).status().map(|s| s.success()).unwrap_or(false);
        if !started {
            open_url(&url);
        }
    }
    #[cfg(not(windows))]
    println!("Rift Launcher : {url}");
}

fn main() {
    let mut app = App::new();
    let token_file = app.data.join("token.txt");
    let listener = match TcpListener::bind(("127.0.0.1", app.port())) {
        Ok(l) => l,
        Err(_) => {
            app.token = fs::read_to_string(&token_file).map(|t| t.trim().to_string()).unwrap_or_default();
            open_window(&app);
            return;
        }
    };
    let after_update = std::env::args().any(|a| a == "--after-update");
    let kept = fs::read_to_string(&token_file).map(|t| t.trim().to_string()).unwrap_or_default();
    app.token = if after_update && !kept.is_empty() { kept } else { new_id().replace('-', "") };
    let _ = fs::write(&token_file, &app.token);
    app.load_library();
    app.welcome_skip_if_known();
    app.log("INFO", &format!("===== Rift Launcher {} (Rust{}) démarre | données : {} =====", app.version, if app.dev { ", dev" } else { "" }, app.data.display()));
    let started = Instant::now();
    let mut window_deadline = None;
    if after_update {
        app.log("INFO", &format!("Redémarrage après mise à jour en {}", app.version));
        window_deadline = Some(started + Duration::from_secs(20));
    } else if std::env::var_os("RIFT_NO_WINDOW").is_none() {
        open_window(&app);
    }
    app.pad = pad::Pad::start(&app.root, &app.data.join("bin"), &app.captures());
    if std::env::var_os("RIFT_OFFLINE").is_none() {
        if !app.dev {
            app.start_update_check();
        }
        app.start_recomp_checks(false);
    }
    let _ = listener.set_nonblocking(true);
    let mut last_tick = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => handle(&mut app, stream),
            Err(_) => std::thread::sleep(Duration::from_millis(40)),
        }
        app.pad_poll();
        if last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            app.tick();
            app.poll_bg();
            app.discord_tick();
            app.pad_sync();
            if window_deadline.map_or(false, |d| Instant::now() >= d) {
                window_deadline = None;
                if app.last_seen <= started {
                    open_window(&app);
                }
            }
            if Instant::now() >= app.next_recomp && std::env::var_os("RIFT_OFFLINE").is_none() {
                app.start_recomp_checks(true);
            }
            if app.quit {
                break;
            }
            if app.running.is_empty() && app.last_seen.elapsed() >= Duration::from_secs(IDLE_EXIT_SECS) {
                break;
            }
        }
    }
    app.save_library();
    app.log("INFO", "Arrêt de Rift Launcher.");
}
