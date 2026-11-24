use crate::json::{parse, Json};
use crate::discord::{self, Conn};
use crate::net;
use crate::{err, ext_of, find_asset, new_id, now_ms, ok, slug, stamp, App, Bg, IMAGE_EXT};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

pub const SOUND_EXT: &[&str] = &["mp3", "wav", "ogg", "m4a"];
pub const RECOMP_EVERY: Duration = Duration::from_secs(6 * 3600);
const UPDATE_REPO: &str = "TeKSama/Rift-Launcher";
const SGDB: &str = "https://www.steamgriddb.com/api/v2";

fn start_folder(current: &str) -> String {
    let p = Path::new(current);
    if current.is_empty() {
        String::new()
    } else if p.is_dir() {
        current.to_string()
    } else {
        p.parent().filter(|d| d.is_dir()).map(|d| d.to_string_lossy().to_string()).unwrap_or_default()
    }
}

fn remove_matching(dir: &Path, stem: &str, by_slug: bool) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        let s = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let hit = if by_slug { slug(&s) == stem } else { s == stem };
        if p.is_file() && hit {
            let _ = fs::remove_file(p);
        }
    }
}

fn fetch_json(url: &str, headers: &[(&str, &str)]) -> Result<(u16, Json), String> {
    let (code, body) = net::get(url, headers)?;
    Ok((code, parse(&body).unwrap_or(Json::Null)))
}

fn recomp_lookup(repo: &str) -> Option<Json> {
    let gh = [("Accept", "application/vnd.github+json")];
    let (code, list) = fetch_json(&format!("https://api.github.com/repos/{repo}/releases?per_page=15"), &gh).ok()?;
    let best = if code == 200 {
        list.items()
            .iter()
            .filter(|r| r.bool("draft") != Some(true))
            .max_by(|a, b| net::version_parts(&a.str("tag_name")).cmp(&net::version_parts(&b.str("tag_name"))))
            .cloned()
    } else {
        None
    };
    let info = match best {
        Some(r) => Json::obj()
            .with("repo", repo)
            .with("latest", r.str("tag_name"))
            .with("url", r.str("html_url"))
            .with("date", r.str("published_at"))
            .with("title", r.str("name")),
        None => {
            let (code, tags) = fetch_json(&format!("https://api.github.com/repos/{repo}/tags?per_page=1"), &gh).ok()?;
            let first = tags.items().first().filter(|_| code == 200)?;
            Json::obj().with("repo", repo).with("latest", first.str("name")).with("url", format!("https://github.com/{repo}/tags")).with("date", "").with("title", "")
        }
    };
    let mut info = info;
    if info.str("latest").is_empty() {
        return None;
    }
    if !info.str("url").starts_with("https://github.com/") {
        info.set("url", format!("https://github.com/{repo}/releases").into());
    }
    Some(info)
}

impl App {
    fn picker_text(&self, key: &str) -> &'static str {
        let fr = !matches!(self.prefs().str("lang").as_str(), "en" | "es" | "zh" | "hi" | "ar");
        match (key, fr) {
            ("exe", true) => "Choisir le jeu",
            ("exe", false) => "Choose the game",
            ("mods", true) => "Choisir le dossier des mods du jeu",
            ("mods", false) => "Choose the game's mods folder",
            ("dir", true) => "Choisir le dossier de sauvegarde du jeu",
            ("dir", false) => "Choose the game's save folder",
            ("cover", true) => "Choisir une jaquette (portrait 600 x 900 conseillé)",
            ("cover", false) => "Choose a cover (portrait 600 x 900 recommended)",
            ("logo", true) => "Choisir un logo (PNG transparent)",
            ("logo", false) => "Choose a logo (transparent PNG)",
            ("sound", true) => "Choisir un son (1 à 3 secondes conseillé)",
            ("sound", false) => "Choose a sound (1 to 3 seconds recommended)",
            _ => "",
        }
    }

    pub fn resolve_sounds(&self, g: &Json) -> Vec<PathBuf> {
        let own = PathBuf::from(g.str("sound"));
        if !g.str("sound").is_empty() && own.is_file() {
            return vec![own];
        }
        let names = self.asset_names(g, true);
        let dirs: Vec<PathBuf> = fs::read_dir(self.sounds())
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.file_name().map_or(false, |n| n != "jeux")).collect())
            .unwrap_or_default();
        for n in names.iter().filter(|n| !n.is_empty()) {
            for d in &dirs {
                if slug(&d.file_name().unwrap_or_default().to_string_lossy()) != *n {
                    continue;
                }
                let mut files: Vec<PathBuf> = fs::read_dir(d)
                    .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && SOUND_EXT.contains(&ext_of(p).as_str())).collect())
                    .unwrap_or_default();
                files.sort();
                if !files.is_empty() {
                    return files;
                }
            }
        }
        find_asset(&self.sounds(), &names, SOUND_EXT)
            .or_else(|| find_asset(&self.web().join("sons"), &names, SOUND_EXT))
            .into_iter()
            .collect()
    }

    pub fn recomp_view(&self, g: &Json) -> Json {
        let Some(info) = self.recomp.get(&g.str("id")) else { return Json::Null };
        if Some(info.str("repo")) != net::repo_of(&g.str("repoUrl")) {
            return Json::Null;
        }
        let ver = g.str("version");
        let is_new = !ver.is_empty() && net::version_parts(&info.str("latest")) > net::version_parts(&ver);
        Json::obj()
            .with("latest", info.str("latest"))
            .with("url", info.str("url"))
            .with("date", info.str("date"))
            .with("title", info.str("title"))
            .with("isNew", is_new)
    }

    fn fetch_update(version: &str) -> Json {
        let url = format!("https://github.com/{UPDATE_REPO}/releases/latest/download/latest.json?t={}", now_ms());
        match fetch_json(&url, &[("Cache-Control", "no-cache")]) {
            Ok((200, m)) if net::newer(&m.str("version"), version) => Json::obj()
                .with("version", m.str("version"))
                .with("notes", m.str("notes"))
                .with("date", m.str("date"))
                .with("url", m.str("url"))
                .with("sha256", m.str("sha256")),
            _ => Json::Null,
        }
    }

    pub fn start_update_check(&self) {
        let (tx, version) = (self.tx.clone(), self.version.clone());
        thread::spawn(move || {
            let _ = tx.send(Bg::Update(App::fetch_update(&version)));
        });
    }

    pub fn start_recomp_checks(&mut self, force: bool) {
        self.next_recomp = Instant::now() + RECOMP_EVERY;
        if self.prefs().bool("recompCheck") == Some(false) {
            return;
        }
        for g in &self.games {
            let Some(repo) = net::repo_of(&g.str("repoUrl")) else { continue };
            let id = g.str("id");
            if !force && self.recomp.get(&id).map_or(false, |i| i.str("repo") == repo) {
                continue;
            }
            let tx = self.tx.clone();
            thread::spawn(move || {
                if let Some(info) = recomp_lookup(&repo) {
                    let _ = tx.send(Bg::Recomp(id, info));
                }
            });
        }
    }

    pub fn poll_bg(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Bg::Update(u) => {
                    if !matches!(u, Json::Null) {
                        self.log("INFO", &format!("Mise à jour disponible : {} -> {}", self.version, u.str("version")));
                    }
                    self.update = u;
                }
                Bg::Recomp(id, info) => {
                    self.recomp.insert(id, info);
                }
            }
        }
    }

    fn check_recomps_now(&mut self) -> Json {
        self.recomp.clear();
        self.start_recomp_checks(true);
        let wanted = self.games.iter().filter(|g| net::repo_of(&g.str("repoUrl")).is_some()).count();
        let until = Instant::now() + Duration::from_secs(10);
        while self.recomp.len() < wanted && Instant::now() < until {
            thread::sleep(Duration::from_millis(150));
            self.poll_bg();
        }
        let updates: Vec<Json> = self
            .games
            .iter()
            .filter(|g| self.recomp_view(g).bool("isNew") == Some(true))
            .map(|g| Json::Str(g.str("name")))
            .collect();
        ok().with("checked", wanted as f64).with("updates", Json::Arr(updates))
    }

    fn install_update(&mut self) -> Json {
        let u = self.update.clone();
        if self.dev {
            return err("Version de dev : installe les mises à jour avec ta version perso.");
        }
        if matches!(u, Json::Null) {
            return err("Aucune mise à jour à installer.");
        }
        if !self.running.is_empty() {
            return err("Ferme ton jeu avant de mettre à jour Rift Launcher.");
        }
        let prefix = format!("https://github.com/{UPDATE_REPO}/releases/download/");
        if !u.str("url").starts_with(&prefix) {
            return err("Adresse de mise à jour refusée.");
        }
        let clean: String = u.str("version").chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        let file = std::env::temp_dir().join(format!("RiftLauncherSetup-{clean}.exe"));
        if let Err(e) = net::download(&u.str("url"), &file) {
            return err(&format!("Mise à jour impossible : {e}"));
        }
        let want = u.str("sha256").to_uppercase();
        if !want.is_empty() && net::sha256(&file).as_deref() != Some(want.as_str()) {
            let _ = fs::remove_file(&file);
            return err("Le fichier téléchargé est abîmé. Réessaie plus tard.");
        }
        if let Err(e) = Command::new(&file).arg("/update").spawn() {
            return err(&format!("Mise à jour impossible : {e}"));
        }
        self.quit = true;
        ok().with("version", u.str("version"))
    }

    fn pick_into(&mut self, id: &str, kind: &str) -> Json {
        let Some(g) = self.game(id).cloned() else { return err("Jeu introuvable.") };
        let (filter, max, dir) = match kind {
            "cover" => ("Images (*.jpg;*.jpeg;*.png;*.webp;*.gif)|*.jpg;*.jpeg;*.png;*.webp;*.gif", 10 << 20, self.covers().join("jeux")),
            "logo" => ("Images (*.png;*.webp;*.svg)|*.png;*.webp;*.svg", 10 << 20, self.logos()),
            _ => ("Sons (*.mp3;*.wav;*.ogg;*.m4a)|*.mp3;*.wav;*.ogg;*.m4a", 2 << 20, self.sounds().join("jeux")),
        };
        let Some(file) = net::pick(false, self.picker_text(kind), filter, "") else { return Json::obj().with("ok", false) };
        let src = PathBuf::from(&file);
        if fs::metadata(&src).map(|m| m.len()).unwrap_or(0) > max {
            return err(if kind == "sound" { "Le son dépasse 2 Mo : prends un extrait plus court." } else { "L'image dépasse 10 Mo." });
        }
        let _ = fs::create_dir_all(&dir);
        let stem = if kind == "logo" { slug(&g.str("name")) } else { id.to_string() };
        remove_matching(&dir, &stem, kind == "logo");
        let dest = dir.join(format!("{stem}.{}", ext_of(&src)));
        if let Err(e) = fs::copy(&src, &dest) {
            return err(&format!("Copie impossible : {e}"));
        }
        if kind != "logo" {
            let path = dest.to_string_lossy().to_string();
            if let Some(g) = self.game_mut(id) {
                g.set(kind, path.into());
                if kind == "cover" {
                    g.set("coverRemote", "".into());
                }
            }
            self.save_library();
        }
        ok()
    }

    fn clear_asset(&mut self, id: &str, kind: &str) -> Json {
        let Some(g) = self.game(id).cloned() else { return err("Jeu introuvable.") };
        match kind {
            "logo" => remove_matching(&self.logos(), &slug(&g.str("name")), true),
            "sound" => {
                remove_matching(&self.sounds().join("jeux"), id, false);
                if let Some(g) = self.game_mut(id) {
                    g.set("sound", "".into());
                }
            }
            _ => {
                remove_matching(&self.covers().join("jeux"), id, false);
                if let Some(g) = self.game_mut(id) {
                    g.set("cover", "".into());
                    g.set("coverRemote", "".into());
                }
            }
        }
        self.save_library();
        ok()
    }

    fn sgdb(&self, path: &str) -> Result<Vec<Json>, String> {
        let key = self.settings().str("sgdbKey");
        if key.is_empty() {
            return Err("NOKEY".into());
        }
        let auth = format!("Bearer {key}");
        let (code, v) = fetch_json(&format!("{SGDB}{path}"), &[("Authorization", &auth)]).map_err(|e| format!("Recherche impossible : {e}"))?;
        if code == 401 || code == 403 {
            return Err("BADKEY".into());
        }
        if code != 200 {
            return Err(format!("Recherche impossible (code {code})."));
        }
        Ok(v.get("data").map(|d| d.items().to_vec()).unwrap_or_default())
    }

    fn search_covers(&self, term: &str, sgdb_id: &str) -> Json {
        let run = || -> Result<Json, String> {
            let mut games = Vec::new();
            if !term.is_empty() {
                games = self
                    .sgdb(&format!("/search/autocomplete/{}", net::encode(term)))?
                    .iter()
                    .take(8)
                    .map(|g| Json::obj().with("id", g.str("id")).with("name", g.str("name")))
                    .collect();
            }
            let id = if sgdb_id.is_empty() { games.first().map(|g| g.str("id")).unwrap_or_default() } else { sgdb_id.to_string() };
            let mut grids = Vec::new();
            if !id.is_empty() {
                grids = self
                    .sgdb(&format!("/grids/game/{id}?dimensions=600x900,1200x1800&types=static"))?
                    .iter()
                    .take(36)
                    .map(|g| Json::obj().with("thumb", g.str("thumb")).with("url", g.str("url")))
                    .collect();
            }
            Ok(Json::obj().with("games", Json::Arr(games)).with("gameId", id).with("grids", Json::Arr(grids)))
        };
        match run() {
            Ok(v) => v,
            Err(e) if e == "NOKEY" => err("NOKEY"),
            Err(e) if e == "BADKEY" => err("Ta clé SteamGridDB est refusée. Vérifie-la dans les réglages."),
            Err(e) => {
                self.log("WARN", &format!("SteamGridDB : {e}"));
                err(&e)
            }
        }
    }

    fn download_cover(&mut self, id: &str, url: &str) -> Json {
        if self.game(id).is_none() {
            return err("Jeu introuvable.");
        }
        let host = url.strip_prefix("https://").and_then(|r| r.split('/').next()).unwrap_or("");
        let host_ok = host.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-".contains(c));
        if !host_ok || !(host == "steamgriddb.com" || host.ends_with(".steamgriddb.com")) {
            return err("Adresse d'image refusée.");
        }
        let path_part = url.split('?').next().unwrap_or(url);
        let mut ext = ext_of(Path::new(path_part));
        if !IMAGE_EXT.contains(&ext.as_str()) {
            ext = "png".into();
        }
        let dir = self.covers().join("jeux");
        let tmp = dir.join(format!("{id}.download"));
        if let Err(e) = net::download(url, &tmp) {
            return err(&format!("Téléchargement impossible : {e}"));
        }
        if fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) > 15 << 20 {
            let _ = fs::remove_file(&tmp);
            return err("Image trop lourde.");
        }
        remove_matching(&dir, id, false);
        let dest = dir.join(format!("{id}.{ext}"));
        if fs::rename(&tmp, &dest).is_err() {
            return err("Enregistrement de la jaquette impossible.");
        }
        let path = dest.to_string_lossy().to_string();
        if let Some(g) = self.game_mut(id) {
            g.set("cover", path.into());
            g.set("coverRemote", url.into());
        }
        self.save_library();
        ok()
    }

    fn save_settings(&self, body: &Json) -> Json {
        let mut s = self.settings();
        if body.get("sgdbKey").is_some() {
            s.set("sgdbKey", body.str("sgdbKey").trim().to_string().into());
        }
        if body.get("discordId").is_some() {
            let d = body.str("discordId").trim().to_string();
            if !d.is_empty() && (!(15..=22).contains(&d.len()) || !d.chars().all(|c| c.is_ascii_digit())) {
                return err("L'ID Discord est un nombre de 17 à 20 chiffres.");
            }
            s.set("discordId", d.into());
        }
        let _ = fs::write(self.settings_file(), s.to_pretty());
        ok().with("hasKey", !s.str("sgdbKey").is_empty())
    }

    fn report(&self) -> Json {
        let tmp = std::env::temp_dir().join(format!("RiftReport-{}", now_ms()));
        let _ = fs::create_dir_all(&tmp);
        let logs = self.data.join("logs");
        let mut files: Vec<PathBuf> = fs::read_dir(&logs).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default();
        files.sort();
        for f in files.iter().rev().take(5) {
            let _ = fs::copy(f, tmp.join(f.file_name().unwrap_or_default()));
        }
        let _ = fs::copy(self.lib_file(), tmp.join("library.json"));
        let state = |p: &str, dir: bool| match (p.is_empty(), Path::new(p).is_dir() == dir && Path::new(p).exists()) {
            (true, _) => "non réglé",
            (false, true) => "OK",
            _ => "INTROUVABLE",
        };
        let mut lines = vec![
            "RAPPORT RIFT LAUNCHER (version Rust)".to_string(),
            format!("Date            : {}", stamp(false)),
            format!("Version         : {}", self.version),
            format!("Dossier appli   : {}", self.root.display()),
            format!("Dossier données : {}", self.data.display()),
            format!("Clé SteamGridDB : {}", if self.settings().str("sgdbKey").is_empty() { "non" } else { "oui (non incluse)" }),
            String::new(),
            format!("JEUX ({})", self.games.len()),
        ];
        for g in &self.games {
            lines.push(format!(
                "- {} | exe : {} | saves : {} | mods : {} | sessions : {}",
                g.str("name"),
                state(&g.str("exePath"), false),
                state(&g.str("saveDir"), true),
                state(&g.str("modsDir"), true),
                g.num("sessions")
            ));
        }
        let _ = fs::write(tmp.join("infos.txt"), lines.join("\r\n"));
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
        let desktop = PathBuf::from(home).join("Desktop");
        let zip = (if desktop.is_dir() { desktop } else { self.data.clone() }).join(format!("Rift Launcher - rapport {}.zip", stamp(false)));
        let result = net::zip(&tmp, &zip);
        let _ = fs::remove_dir_all(&tmp);
        match result {
            Ok(_) => {
                self.log("INFO", &format!("Rapport créé : {}", zip.display()));
                crate::open_path(&zip.to_string_lossy(), true);
                ok().with("path", zip.to_string_lossy().to_string())
            }
            Err(e) => err(&format!("Impossible de créer le rapport : {e}")),
        }
    }

    pub fn api_more(&mut self, name: &str, body: &Json) -> Option<Json> {
        let id = body.str("id");
        Some(match name {
            "pick" => {
                let kind = body.str("kind");
                let start = start_folder(&body.str("current"));
                let path = if kind == "exe" {
                    net::pick(false, self.picker_text("exe"), "Jeux (*.exe;*.bat;*.cmd)|*.exe;*.bat;*.cmd|Tous les fichiers (*.*)|*.*", &start)
                } else {
                    net::pick(true, self.picker_text(if kind == "mods" { "mods" } else { "dir" }), "", &start)
                };
                Json::obj().with("path", path.map(Json::Str).unwrap_or(Json::Null))
            }
            "coverPick" => self.pick_into(&id, "cover"),
            "soundPick" => self.pick_into(&id, "sound"),
            "logoPick" => self.pick_into(&id, "logo"),
            "assetClear" => self.clear_asset(&id, &body.str("kind")),
            "coverSearch" => self.search_covers(&body.str("term"), &body.str("sgdbId")),
            "coverDownload" => self.download_cover(&id, &body.str("url")),
            "settingsSave" => {
                self.discord_reset();
                self.save_settings(body)
            }
            "updateCheck" if self.dev => ok().with("current", self.version.clone()).with("update", Json::Null),
            "updateCheck" => {
                self.update = App::fetch_update(&self.version);
                ok().with("current", self.version.clone()).with("update", self.update.clone())
            }
            "updateInstall" => self.install_update(),
            "recompCheck" => self.check_recomps_now(),
            "recompDone" => {
                let v = self.game(&id).map(|g| self.recomp_view(g)).unwrap_or(Json::Null);
                if matches!(v, Json::Null) {
                    return Some(err("Aucune version trouvée pour ce jeu."));
                }
                let latest = v.str("latest");
                let clean = latest.strip_prefix(['v', 'V']).filter(|r| r.starts_with(|c: char| c.is_ascii_digit())).unwrap_or(&latest).to_string();
                if let Some(g) = self.game_mut(&id) {
                    g.set("version", clean.into());
                }
                self.save_library();
                ok()
            }
            "recompOpen" => {
                let v = self.game(&id).map(|g| self.recomp_view(g)).unwrap_or(Json::Null);
                if v.str("url").starts_with("https://github.com/") {
                    crate::open_url(&v.str("url"));
                }
                Json::obj().with("ok", !matches!(v, Json::Null))
            }
            "report" => self.report(),
            _ => return None,
        })
    }

    pub fn discord_reset(&mut self) {
        self.discord = None;
        self.discord_shown.clear();
        self.discord_retry = Instant::now();
    }

    fn discord_wanted(&self) -> String {
        if self.prefs().bool("discord") == Some(false) {
            return String::new();
        }
        if let Some((id, _)) = self.running.iter().max_by_key(|(_, r)| r.started_ms) {
            return id.clone();
        }
        if self.last_seen.elapsed() < Duration::from_secs(75) {
            return "menu".into();
        }
        String::new()
    }

    pub fn discord_tick(&mut self) {
        let want = self.discord_wanted();
        if want == self.discord_shown && (self.discord.is_some() || want.is_empty()) {
            return;
        }
        if want.is_empty() {
            self.discord_reset();
            return;
        }
        if self.discord.is_none() && Instant::now() < self.discord_retry {
            return;
        }
        self.discord_retry = Instant::now() + Duration::from_secs(60);
        if self.discord.is_none() {
            let own = self.settings().str("discordId");
            let client = if (15..=22).contains(&own.len()) && own.chars().all(|c| c.is_ascii_digit()) { own } else { discord::BUILTIN_ID.to_string() };
            match Conn::connect(&client) {
                Ok(c) => {
                    self.log("INFO", "Discord : connexion acceptée");
                    self.discord = Some(c);
                }
                Err(e) => {
                    if e != self.discord_err {
                        self.log("INFO", &format!("{e} Nouvel essai toutes les 60 s."));
                        self.discord_err = e;
                    }
                    return;
                }
            }
        }
        let rift = || Json::obj().with("large_image", "rift").with("large_text", "Rift Launcher");
        let (activity, label) = if want == "menu" {
            let act = Json::obj().with("details", "Navigue dans les menus").with("timestamps", Json::obj().with("start", self.started_ms)).with("assets", rift());
            (act, "menus".to_string())
        } else {
            let Some(g) = self.game(&want).cloned() else { return };
            let start = self.running.get(&want).map(|r| r.started_ms).unwrap_or_else(now_ms);
            let name = discord::clip(&g.str("name"), 128);
            let remote = g.str("coverRemote");
            let assets = if remote.starts_with("https://") {
                Json::obj().with("large_image", remote).with("large_text", name.clone()).with("small_image", "rift").with("small_text", "Rift Launcher")
            } else {
                rift()
            };
            let act = Json::obj().with("details", name).with("state", "via Rift Launcher").with("timestamps", Json::obj().with("start", start)).with("assets", assets);
            (act, g.str("name"))
        };
        let result = self.discord.as_mut().map(|c| c.set_activity(activity, &new_id())).unwrap_or(Ok(()));
        match result {
            Ok(()) => {
                if self.discord_shown != want {
                    self.log("INFO", &format!("Statut Discord : {label}"));
                }
                self.discord_shown = want;
                self.discord_err.clear();
            }
            Err(e) => {
                self.log("WARN", &format!("Statut Discord impossible : {e}"));
                self.discord = None;
                self.discord_shown.clear();
            }
        }
    }
}
