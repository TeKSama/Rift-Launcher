use crate::json::{parse, Json};
use crate::{err, ext_of, new_id, ok, App};
use std::fs;
use std::path::PathBuf;

impl App {
    fn preset_games(&self) -> Vec<Json> {
        fs::read_to_string(self.root.join("preset").join("preset.json"))
            .ok()
            .and_then(|t| parse(&t).ok())
            .and_then(|p| p.get("games").map(|g| g.items().to_vec()))
            .unwrap_or_default()
    }

    pub fn preset_file(&self, pg: &Json, kind: &str) -> Option<PathBuf> {
        let keys: &[&str] = if kind == "cover" { &["cover", "coverFallback"] } else { &["sound"] };
        keys.iter().map(|k| pg.str(k)).filter(|r| !r.is_empty() && !r.contains("..")).map(|r| self.root.join(r)).find(|p| p.is_file())
    }

    pub fn preset_asset(&self, n: usize, kind: &str) -> Option<PathBuf> {
        self.preset_games().get(n).and_then(|pg| self.preset_file(pg, kind))
    }

    pub fn welcome_wanted(&self) -> bool {
        self.settings().bool("welcomeDone") != Some(true)
    }

    pub fn welcome_skip_if_known(&self) {
        if self.games.is_empty() || !self.welcome_wanted() {
            return;
        }
        let mut s = self.settings();
        s.set("welcomeDone", true.into());
        let _ = fs::write(self.settings_file(), s.to_pretty());
    }

    fn owned(&self, name: &str) -> bool {
        self.games.iter().any(|g| g.str("name").eq_ignore_ascii_case(name))
    }

    fn welcome_view(&self) -> Json {
        let games: Vec<Json> = self
            .preset_games()
            .iter()
            .enumerate()
            .map(|(i, pg)| {
                let url = |k: &str| self.preset_file(pg, k).map(|_| format!("/preset?n={i}&k={k}")).unwrap_or_default();
                Json::obj()
                    .with("n", i as f64)
                    .with("name", pg.str("name"))
                    .with("repoUrl", pg.str("repoUrl"))
                    .with("coverUrl", url("cover"))
                    .with("soundUrl", url("sound"))
                    .with("owned", self.owned(&pg.str("name")))
            })
            .collect();
        ok().with("games", Json::Arr(games))
    }

    fn welcome_done(&mut self, picks: &Json) -> Json {
        let preset = self.preset_games();
        let mut added = 0.0;
        for n in picks.items().iter().filter_map(|p| if let Json::Num(n) = p { Some(*n as usize) } else { None }) {
            let Some(pg) = preset.get(n) else { continue };
            if self.owned(&pg.str("name")) {
                continue;
            }
            let mut g = App::new_game(pg, false);
            let id = new_id();
            g.set("id", id.clone().into());
            for (kind, dir) in [("cover", self.covers().join("jeux")), ("sound", self.sounds().join("jeux"))] {
                let Some(src) = self.preset_file(pg, kind) else { continue };
                let _ = fs::create_dir_all(&dir);
                let dest = dir.join(format!("{id}.{}", ext_of(&src)));
                if fs::copy(&src, &dest).is_ok() {
                    g.set(kind, dest.to_string_lossy().to_string().into());
                }
            }
            self.log("INFO", &format!("Préchargement : {}", pg.str("name")));
            self.games.push(g);
            added += 1.0;
        }
        let mut s = self.settings();
        s.set("welcomeDone", true.into());
        if fs::write(self.settings_file(), s.to_pretty()).is_err() {
            return err("Impossible d'enregistrer les réglages.");
        }
        if added > 0.0 {
            self.save_library();
            self.start_recomp_checks(false);
        }
        ok().with("added", added)
    }

    pub fn api_welcome(&mut self, name: &str, body: &Json) -> Option<Json> {
        match name {
            "welcome" => Some(self.welcome_view()),
            "welcomeDone" => Some(self.welcome_done(body.get("picks").unwrap_or(&Json::Null))),
            _ => None,
        }
    }
}
