use crate::json::Json;
use crate::{open_window, App};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
pub struct Gate {
    pub last_toggle: Option<Instant>,
    pub last_shot: Option<Instant>,
    pub back_to: isize,
}

pub struct Shared {
    pub home_enabled: AtomicBool,
    pub capture_enabled: AtomicBool,
    pub game_pid: AtomicU32,
    pub game_name: Mutex<String>,
    pub want_window: AtomicBool,
    pub dxgi_broken: AtomicBool,
    pub logs: Mutex<Vec<String>>,
    pub capture: Mutex<(u32, String)>,
    pub gate: Mutex<Gate>,
    pub hid: Mutex<(HashSet<String>, HashSet<String>)>,
    pub shot_exe: Mutex<Option<PathBuf>>,
    pub captures: PathBuf,
}

impl Shared {
    pub fn log(&self, m: &str) {
        let mut l = self.logs.lock().unwrap();
        if l.len() < 200 {
            l.push(m.to_string());
        }
    }
}

pub struct Pad {
    shared: Arc<Shared>,
    synced: String,
    shown: (u32, String),
}

impl Pad {
    pub fn start(root: &Path, bin: &Path, captures: &Path) -> Option<Pad> {
        let shared = Arc::new(Shared {
            home_enabled: AtomicBool::new(true),
            capture_enabled: AtomicBool::new(true),
            game_pid: AtomicU32::new(0),
            game_name: Mutex::new(String::new()),
            want_window: AtomicBool::new(false),
            dxgi_broken: AtomicBool::new(false),
            logs: Mutex::new(Vec::new()),
            capture: Mutex::new((0, String::new())),
            gate: Mutex::new(Gate::default()),
            hid: Mutex::new((HashSet::new(), HashSet::new())),
            shot_exe: Mutex::new(None),
            captures: captures.to_path_buf(),
        });
        #[cfg(windows)]
        {
            crate::win::start(shared.clone(), root, bin);
            Some(Pad { shared, synced: String::new(), shown: (u32::MAX, String::new()) })
        }
        #[cfg(not(windows))]
        {
            let _ = (root, bin, shared);
            None
        }
    }

    fn ask(&self, cmd: &str) -> String {
        #[cfg(windows)]
        {
            match cmd {
                "toggle" => crate::win::toggle(&self.shared),
                "capture" => crate::win::capture(&self.shared),
                _ => crate::win::press_f11(),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (cmd, &self.shared);
            String::new()
        }
    }
}

impl App {
    pub fn pad_ask(&mut self, cmd: &str, _wait: u64) -> Option<String> {
        self.pad.as_ref().map(|p| p.ask(cmd))
    }

    pub fn pad_poll(&mut self) {
        let Some(pad) = self.pad.as_mut() else { return };
        let logs: Vec<String> = std::mem::take(&mut *pad.shared.logs.lock().unwrap());
        let want = pad.shared.want_window.swap(false, Ordering::Relaxed);
        let cap = pad.shared.capture.lock().unwrap().clone();
        let changed = cap != pad.shown;
        if changed {
            pad.shown = cap.clone();
        }
        for m in logs {
            self.log("INFO", &format!("[manette] {m}"));
        }
        if changed {
            let last = Path::new(&cap.1).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            self.capture = Json::obj().with("count", cap.0 as f64).with("last", last);
        }
        if want {
            self.log("INFO", "Bouton Home : réouverture de la fenêtre");
            open_window(self);
        }
    }

    pub fn pad_sync(&mut self) {
        if self.pad.is_none() {
            return;
        }
        let prefs = self.prefs();
        let on = |k: &str| prefs.bool(k) != Some(false);
        let newest = self.running.iter().max_by_key(|(_, r)| r.started_ms).map(|(id, r)| (id.clone(), r.child.id()));
        let (pid, name) = match newest {
            Some((id, pid)) => (pid, self.game(&id).map(|g| g.str("name")).unwrap_or_default()),
            None => (0, String::new()),
        };
        let (home, cap) = (on("homeToggle"), on("captureButton"));
        let key = format!("{home}{cap}{pid}{name}");
        let Some(pad) = self.pad.as_mut() else { return };
        if pad.synced == key {
            return;
        }
        let s = &pad.shared;
        s.home_enabled.store(home, Ordering::Relaxed);
        s.capture_enabled.store(cap, Ordering::Relaxed);
        s.game_pid.store(pid, Ordering::Relaxed);
        *s.game_name.lock().unwrap() = name;
        pad.synced = key;
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

pub fn png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let row = w as usize * 3;
    let mut raw = Vec::with_capacity((row + 1) * h as usize);
    for line in rgb.chunks(row) {
        raw.push(1);
        raw.extend_from_slice(&line[..3]);
        for i in 3..line.len() {
            raw.push(line[i].wrapping_sub(line[i - 3]));
        }
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in raw.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

pub fn shutter_wav() -> Vec<u8> {
    let rate = 22050u32;
    let n = rate * 14 / 100;
    let mut w = Vec::with_capacity(44 + n as usize * 2);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + n * 2).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(n * 2).to_le_bytes());
    let mut seed = 7u32;
    for i in 0..n {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let noise = (seed >> 8) as f64 / (1u32 << 24) as f64 * 2.0 - 1.0;
        let t = i as f64 / rate as f64;
        let env = if t < 0.05 { (-t * 90.0).exp() } else { 0.6 * (-(t - 0.05) * 70.0).exp() };
        w.extend_from_slice(&((noise * env * 0.35 * i16::MAX as f64) as i16).to_le_bytes());
    }
    w
}

#[cfg(test)]
mod tests {
    #[test]
    fn png_et_son() {
        let img = super::png(3, 2, &[255, 0, 0, 0, 255, 0, 0, 0, 255, 10, 20, 30, 40, 50, 60, 70, 80, 90]);
        std::fs::write(std::env::temp_dir().join("rift-test.png"), &img).unwrap();
        assert_eq!(&img[1..4], b"PNG");
        assert_eq!(super::shutter_wav().len(), 44 + 3087 * 2);
    }
}
