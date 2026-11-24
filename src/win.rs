use crate::net;
use crate::pad::{png, shutter_wav, Shared};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

type H = isize;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

#[repr(C)]
struct SpDid {
    cb_size: u32,
    guid: Guid,
    flags: u32,
    reserved: usize,
}

#[repr(C)]
#[derive(Default)]
struct HiddAttributes {
    size: u32,
    vid: u16,
    pid: u16,
    ver: u16,
}

#[repr(C)]
struct HidpCaps {
    usage: u16,
    usage_page: u16,
    input_len: u16,
    rest: [u16; 29],
}

#[repr(C)]
#[derive(Default)]
struct XState {
    packet: u32,
    buttons: u16,
    lt: u8,
    rt: u8,
    lx: i16,
    ly: i16,
    rx: i16,
    ry: i16,
}

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default)]
struct MonitorInfo {
    cb_size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    xppm: i32,
    yppm: i32,
    clr_used: u32,
    clr_important: u32,
    colors: [u32; 1],
}

#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    dow: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    ms: u16,
}

type EnumProc = extern "system" fn(H, isize) -> i32;

#[cfg_attr(windows, link(name = "user32", kind = "raw-dylib"))]
extern "system" {
    fn SetProcessDPIAware() -> i32;
    fn EnumWindows(f: EnumProc, l: isize) -> i32;
    fn GetWindowTextW(h: H, s: *mut u16, n: i32) -> i32;
    fn GetClassNameW(h: H, s: *mut u16, n: i32) -> i32;
    fn IsWindowVisible(h: H) -> i32;
    fn IsWindow(h: H) -> i32;
    fn IsIconic(h: H) -> i32;
    fn GetForegroundWindow() -> H;
    fn SetForegroundWindow(h: H) -> i32;
    fn BringWindowToTop(h: H) -> i32;
    fn ShowWindow(h: H, cmd: i32) -> i32;
    fn GetWindow(h: H, cmd: u32) -> H;
    fn GetWindowThreadProcessId(h: H, pid: *mut u32) -> u32;
    fn AttachThreadInput(a: u32, b: u32, attach: i32) -> i32;
    fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    fn MonitorFromWindow(h: H, flags: u32) -> H;
    fn GetMonitorInfoW(m: H, info: *mut MonitorInfo) -> i32;
    fn GetDC(h: H) -> H;
    fn ReleaseDC(h: H, dc: H) -> i32;
}

#[cfg_attr(windows, link(name = "kernel32", kind = "raw-dylib"))]
extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn LoadLibraryW(name: *const u16) -> H;
    fn GetProcAddress(lib: H, name: *const u8) -> usize;
    fn CreateFileW(name: *const u16, access: u32, share: u32, sec: usize, disposition: u32, flags: u32, template: H) -> H;
    fn ReadFile(h: H, buf: *mut u8, len: u32, read: *mut u32, overlapped: usize) -> i32;
    fn CloseHandle(h: H) -> i32;
    fn GetLocalTime(t: *mut SystemTime);
}

#[cfg_attr(windows, link(name = "gdi32", kind = "raw-dylib"))]
extern "system" {
    fn CreateCompatibleDC(dc: H) -> H;
    fn CreateCompatibleBitmap(dc: H, w: i32, h: i32) -> H;
    fn SelectObject(dc: H, obj: H) -> H;
    fn BitBlt(dc: H, x: i32, y: i32, w: i32, h: i32, src: H, sx: i32, sy: i32, rop: u32) -> i32;
    fn GetDIBits(dc: H, bmp: H, start: u32, lines: u32, bits: *mut u8, info: *mut BitmapInfo, usage: u32) -> i32;
    fn DeleteObject(obj: H) -> i32;
    fn DeleteDC(dc: H) -> i32;
}

#[cfg_attr(windows, link(name = "winmm", kind = "raw-dylib"))]
extern "system" {
    fn PlaySoundW(sound: *const u16, module: H, flags: u32) -> i32;
}

#[cfg_attr(windows, link(name = "hid", kind = "raw-dylib"))]
extern "system" {
    fn HidD_GetHidGuid(g: *mut Guid);
    fn HidD_GetAttributes(h: H, a: *mut HiddAttributes) -> u8;
    fn HidD_GetPreparsedData(h: H, p: *mut isize) -> u8;
    fn HidD_FreePreparsedData(p: isize) -> u8;
    fn HidP_GetCaps(p: isize, c: *mut HidpCaps) -> i32;
}

#[cfg_attr(windows, link(name = "setupapi", kind = "raw-dylib"))]
extern "system" {
    fn SetupDiGetClassDevsW(g: *const Guid, enumerator: *const u16, parent: H, flags: u32) -> H;
    fn SetupDiEnumDeviceInterfaces(set: H, dev: usize, g: *const Guid, index: u32, d: *mut SpDid) -> i32;
    fn SetupDiGetDeviceInterfaceDetailW(set: H, d: *mut SpDid, detail: *mut u8, size: u32, need: *mut u32, dev: usize) -> i32;
    fn SetupDiDestroyDeviceInfoList(set: H) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn text(h: H, class: bool) -> String {
    let mut b = [0u16; 512];
    let n = unsafe { if class { GetClassNameW(h, b.as_mut_ptr(), 512) } else { GetWindowTextW(h, b.as_mut_ptr(), 512) } };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}

fn each_window(mut f: impl FnMut(H) -> bool) {
    extern "system" fn cb(h: H, l: isize) -> i32 {
        let f = unsafe { &mut *(l as *mut &mut dyn FnMut(H) -> bool) };
        f(h) as i32
    }
    let mut d: &mut dyn FnMut(H) -> bool = &mut f;
    unsafe { EnumWindows(cb, &mut d as *mut _ as isize) };
}

fn find_launcher() -> H {
    let mut found = 0;
    each_window(|h| {
        if unsafe { IsWindowVisible(h) } == 0 || text(h, false) != "Rift Launcher" || !text(h, true).starts_with("Chrome_WidgetWin") {
            return true;
        }
        found = h;
        false
    });
    found
}

fn game_window(pid: u32) -> H {
    if pid == 0 {
        return 0;
    }
    let mut found = 0;
    each_window(|h| {
        let mut p = 0u32;
        unsafe { GetWindowThreadProcessId(h, &mut p) };
        if p != pid || unsafe { IsWindowVisible(h) } == 0 || unsafe { GetWindow(h, 4) } != 0 {
            return true;
        }
        found = h;
        false
    });
    found
}

fn focus(h: H) {
    unsafe {
        if IsIconic(h) != 0 {
            ShowWindow(h, 9);
        }
        let fg = GetForegroundWindow();
        let mut pid = 0u32;
        let fg_thread = GetWindowThreadProcessId(fg, &mut pid);
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        BringWindowToTop(h);
        SetForegroundWindow(h);
        if attached {
            AttachThreadInput(me, fg_thread, 0);
        }
        if GetForegroundWindow() != h {
            keybd_event(0x12, 0, 0, 0);
            keybd_event(0x12, 0, 2, 0);
            SetForegroundWindow(h);
        }
    }
}

pub fn toggle(s: &Shared) -> String {
    let mut g = s.gate.lock().unwrap();
    if g.last_toggle.map_or(false, |t| t.elapsed() < Duration::from_millis(450)) {
        return "ignore".into();
    }
    g.last_toggle = Some(Instant::now());
    let launcher = find_launcher();
    let fg = unsafe { GetForegroundWindow() };
    let pid = s.game_pid.load(Ordering::Relaxed);
    if launcher == 0 {
        if pid != 0 {
            g.back_to = fg;
            s.want_window.store(true, Ordering::Relaxed);
            return "ouverture".into();
        }
        return "fenetre introuvable".into();
    }
    if fg == launcher {
        let mut target = game_window(pid);
        if target == 0 && g.back_to != 0 && unsafe { IsWindow(g.back_to) } != 0 {
            target = g.back_to;
        }
        if target == 0 {
            return "aucun jeu".into();
        }
        focus(target);
        return "jeu".into();
    }
    g.back_to = fg;
    focus(launcher);
    "launcher".into()
}

pub fn press_f11() -> String {
    let launcher = find_launcher();
    if launcher == 0 {
        return "fenetre introuvable".into();
    }
    unsafe {
        if GetForegroundWindow() != launcher {
            focus(launcher);
        }
        keybd_event(0x7A, 0, 0, 0);
        keybd_event(0x7A, 0, 2, 0);
    }
    "ok".into()
}

fn local_stamp() -> String {
    let mut t = SystemTime::default();
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
}

fn shot_gdi(file: &Path, r: &Rect) -> Result<(), String> {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w <= 0 || h <= 0 {
        return Err("écran introuvable".into());
    }
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    unsafe {
        let screen = GetDC(0);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);
        BitBlt(mem, 0, 0, w, h, screen, r.left, r.top, 0x00CC_0020 | 0x4000_0000);
        SelectObject(mem, old);
        let mut info = BitmapInfo { size: 40, width: w, height: -h, planes: 1, bit_count: 32, compression: 0, size_image: 0, xppm: 0, yppm: 0, clr_used: 0, clr_important: 0, colors: [0] };
        let lines = GetDIBits(mem, bmp, 0, h as u32, bgra.as_mut_ptr(), &mut info, 0);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(0, screen);
        if lines == 0 {
            return Err("copie de l'écran impossible".into());
        }
    }
    let rgb: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
    std::fs::write(file, png(w as u32, h as u32, &rgb)).map_err(|e| e.to_string())
}

fn shot_dxgi(s: &Shared, file: &Path, x: i32, y: i32) -> bool {
    let Some(exe) = s.shot_exe.lock().unwrap().clone() else { return false };
    if s.dxgi_broken.load(Ordering::Relaxed) || !exe.is_file() {
        return false;
    }
    let mut c = Command::new(&exe);
    net::hide(&mut c);
    let Ok(mut child) = c.arg(file).arg(x.to_string()).arg(y.to_string()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn() else {
        s.log("capture DXGI impossible : lancement refusé");
        s.dxgi_broken.store(true, Ordering::Relaxed);
        return false;
    };
    let until = Instant::now() + Duration::from_secs(8);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(30)),
            _ => break None,
        }
    };
    let Some(status) = status else {
        let _ = child.kill();
        s.log("capture DXGI trop longue, capture classique");
        return false;
    };
    if status.success() && file.is_file() {
        return true;
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut e, &mut err);
    }
    s.log(&format!("capture DXGI impossible ({}), capture classique", err.trim()));
    if status.code() != Some(2) {
        s.dxgi_broken.store(true, Ordering::Relaxed);
    }
    false
}

fn shutter() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    let wav = WAV.get_or_init(shutter_wav);
    unsafe { PlaySoundW(wav.as_ptr() as *const u16, 0, 0x0004 | 0x0001 | 0x0002) };
}

pub fn capture(s: &Shared) -> String {
    {
        let mut g = s.gate.lock().unwrap();
        if g.last_shot.map_or(false, |t| t.elapsed() < Duration::from_millis(700)) {
            return String::new();
        }
        g.last_shot = Some(Instant::now());
    }
    let mut info = MonitorInfo { cb_size: std::mem::size_of::<MonitorInfo>() as u32, ..Default::default() };
    unsafe { GetMonitorInfoW(MonitorFromWindow(GetForegroundWindow(), 2), &mut info) };
    let r = info.monitor;
    let game = s.game_name.lock().unwrap().clone();
    let dir = s.captures.join(crate::safe_name(if game.is_empty() { "Rift Launcher" } else { &game }));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        s.log(&format!("capture impossible : {e}"));
        return String::new();
    }
    let stamp = local_stamp();
    let mut file = dir.join(format!("{stamp}.png"));
    let mut i = 2;
    while file.exists() {
        file = dir.join(format!("{stamp} ({i}).png"));
        i += 1;
    }
    let how = if shot_dxgi(s, &file, r.left + (r.right - r.left) / 2, r.top + (r.bottom - r.top) / 2) {
        "DXGI"
    } else if let Err(e) = shot_gdi(&file, &r) {
        s.log(&format!("capture impossible : {e}"));
        return String::new();
    } else {
        "classique"
    };
    shutter();
    s.log(&format!("capture ({how})"));
    let path = file.to_string_lossy().to_string();
    {
        let mut c = s.capture.lock().unwrap();
        c.0 += 1;
        c.1 = path.clone();
    }
    s.log(&format!("capture enregistrée : {path}"));
    path
}

fn x_loop(s: Arc<Shared>) {
    let lib = ["xinput1_4.dll", "xinput1_3.dll"].iter().map(|n| unsafe { LoadLibraryW(wide(n).as_ptr()) }).find(|&h| h != 0);
    let proc_addr = lib.map(|l| unsafe { GetProcAddress(l, 100usize as *const u8) }).unwrap_or(0);
    if proc_addr == 0 {
        s.log("XInput indisponible : bouton Xbox désactivé");
        return;
    }
    let get: extern "system" fn(u32, *mut XState) -> u32 = unsafe { std::mem::transmute(proc_addr) };
    let mut was = [false; 4];
    let mut high = [0u16; 4];
    loop {
        let mut any = false;
        for (i, w) in was.iter_mut().enumerate() {
            let mut st = XState::default();
            if get(i as u32, &mut st) != 0 {
                *w = false;
                continue;
            }
            any = true;
            let extra = st.buttons & 0xFC00;
            if extra != high[i] {
                high[i] = extra;
                s.log(&format!("XInput manette {} : boutons 0x{:04X}", i + 1, st.buttons));
            }
            let guide = st.buttons & 0x0400 != 0;
            if guide && !*w && s.home_enabled.load(Ordering::Relaxed) {
                let r = toggle(&s);
                s.log(&format!("bouton Xbox (XInput) : {r}"));
            }
            *w = guide;
        }
        thread::sleep(Duration::from_millis(if any { 25 } else { 1500 }));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Ds4,
    DualSense,
    SwitchPro,
    XboxBt,
}

fn name_of(k: Kind) -> &'static str {
    match k {
        Kind::Ds4 => "PlayStation 4",
        Kind::DualSense => "PlayStation 5",
        Kind::SwitchPro => "Switch Pro",
        Kind::XboxBt => "Xbox Series (Bluetooth)",
    }
}

fn kind_of(vid: u16, pid: u16) -> Option<Kind> {
    match (vid, pid) {
        (0x054C, 0x05C4 | 0x09CC | 0x0BA0) => Some(Kind::Ds4),
        (0x054C, 0x0CE6 | 0x0DF2) => Some(Kind::DualSense),
        (0x057E, 0x2009) => Some(Kind::SwitchPro),
        (0x045E, 0x0B13 | 0x0B20 | 0x0B22) => Some(Kind::XboxBt),
        _ => None,
    }
}

fn hid_paths() -> Vec<String> {
    let mut list = Vec::new();
    let mut g = Guid::default();
    unsafe {
        HidD_GetHidGuid(&mut g);
        let set = SetupDiGetClassDevsW(&g, std::ptr::null(), 0, 0x12);
        if set == 0 || set == -1 {
            return list;
        }
        for i in 0..512 {
            let mut d = SpDid { cb_size: std::mem::size_of::<SpDid>() as u32, guid: Guid::default(), flags: 0, reserved: 0 };
            if SetupDiEnumDeviceInterfaces(set, 0, &g, i, &mut d) == 0 {
                break;
            }
            let mut need = 0u32;
            SetupDiGetDeviceInterfaceDetailW(set, &mut d, std::ptr::null_mut(), 0, &mut need, 0);
            if need < 8 {
                continue;
            }
            let mut buf = vec![0u32; (need as usize + 3) / 4];
            buf[0] = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
            if SetupDiGetDeviceInterfaceDetailW(set, &mut d, buf.as_mut_ptr() as *mut u8, need, &mut need, 0) != 0 {
                let chars = std::slice::from_raw_parts((buf.as_ptr() as *const u8).add(4) as *const u16, (need as usize - 4) / 2);
                let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
                list.push(String::from_utf16_lossy(&chars[..end]));
            }
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    list
}

fn open(path: &str, access: u32) -> H {
    unsafe { CreateFileW(wide(path).as_ptr(), access, 3, 0, 3, 0, 0) }
}

fn probe(s: &Shared, path: &str) -> Option<Kind> {
    let h = open(path, 0);
    if h == -1 {
        return None;
    }
    let mut a = HiddAttributes { size: std::mem::size_of::<HiddAttributes>() as u32, ..Default::default() };
    let ok = unsafe { HidD_GetAttributes(h, &mut a) } != 0;
    unsafe { CloseHandle(h) };
    if ok && a.vid == 0x045E {
        s.log(&format!("appareil Microsoft vu en HID : VID 045E PID {:04X}", a.pid));
    }
    if ok {
        kind_of(a.vid, a.pid)
    } else {
        None
    }
}

fn bit(b: &[u8], i: usize, bit: u8) -> bool {
    b.get(i).map_or(false, |v| v & (1 << bit) != 0)
}

fn parse(k: Kind, b: &[u8], len: usize) -> Option<(bool, bool)> {
    if b.len() < 3 {
        return None;
    }
    let id = b[0];
    match k {
        Kind::Ds4 => {
            let o = match id {
                0x01 => 0,
                0x11 => 2,
                _ => return None,
            };
            Some((bit(b, 7 + o, 0), bit(b, 6 + o, 4)))
        }
        Kind::DualSense => match id {
            0x01 if len == 64 => Some((bit(b, 10, 0), bit(b, 9, 4))),
            0x31 => Some((bit(b, 11, 0), bit(b, 10, 4))),
            0x01 => Some((bit(b, 7, 0), bit(b, 6, 4))),
            _ => None,
        },
        Kind::SwitchPro => match id {
            0x30 | 0x21 => Some((bit(b, 4, 4), bit(b, 4, 5))),
            0x3F => Some((bit(b, 2, 4), bit(b, 2, 5))),
            _ => None,
        },
        Kind::XboxBt if id == 0x01 && b.len() > 16 => Some((false, bit(b, 16, 0))),
        Kind::XboxBt => None,
    }
}

fn reader(s: Arc<Shared>, path: String, kind: Kind) {
    let h = open(&path, 0x8000_0000);
    if h == -1 {
        s.log(&format!("manette {} : lecture refusée (un autre programme la garde pour lui ?)", name_of(kind)));
        s.hid.lock().unwrap().1.insert(path.clone());
        s.hid.lock().unwrap().0.remove(&path);
        return;
    }
    let mut len = 64usize;
    unsafe {
        let mut pp = 0isize;
        if HidD_GetPreparsedData(h, &mut pp) != 0 {
            let mut c = HidpCaps { usage: 0, usage_page: 0, input_len: 0, rest: [0; 29] };
            if HidP_GetCaps(pp, &mut c) == 0x0011_0000 && c.input_len > 0 {
                len = c.input_len as usize;
            }
            HidD_FreePreparsedData(pp);
        }
    }
    let what = if kind == Kind::XboxBt { "bouton Partager actif" } else { "boutons Home et capture actifs" };
    s.log(&format!("manette {} trouvée : {what}", name_of(kind)));
    let mut buf = vec![0u8; len];
    let (mut home, mut cap) = (false, false);
    loop {
        let mut n = 0u32;
        if unsafe { ReadFile(h, buf.as_mut_ptr(), len as u32, &mut n, 0) } == 0 || n == 0 {
            break;
        }
        let Some((h_now, c_now)) = parse(kind, &buf[..n as usize], len) else { continue };
        if h_now && !home && s.home_enabled.load(Ordering::Relaxed) {
            toggle(&s);
        }
        if c_now && !cap && s.capture_enabled.load(Ordering::Relaxed) {
            capture(&s);
        }
        home = h_now;
        cap = c_now;
    }
    unsafe { CloseHandle(h) };
    s.log(&format!("manette {} déconnectée", name_of(kind)));
    s.hid.lock().unwrap().0.remove(&path);
}

fn hid_scan(s: Arc<Shared>) {
    loop {
        for p in hid_paths() {
            {
                let hid = s.hid.lock().unwrap();
                if hid.0.contains(&p) || hid.1.contains(&p) {
                    continue;
                }
            }
            let Some(k) = probe(&s, &p) else {
                s.hid.lock().unwrap().1.insert(p);
                continue;
            };
            s.hid.lock().unwrap().0.insert(p.clone());
            let s2 = s.clone();
            thread::spawn(move || reader(s2, p, k));
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn build_shot(s: Arc<Shared>, src: PathBuf, exe: PathBuf) {
    let stale = match (std::fs::metadata(&src).and_then(|m| m.modified()), std::fs::metadata(&exe).and_then(|m| m.modified())) {
        (Ok(a), Ok(b)) => a > b,
        (Ok(_), Err(_)) => true,
        _ => return,
    };
    if stale {
        let _ = std::fs::remove_file(&exe);
        let mut c = Command::new("powershell.exe");
        net::hide(&mut c);
        let ok = c
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "Add-Type -Path $env:RIFT_SRC -OutputAssembly $env:RIFT_EXE -OutputType WindowsApplication -ReferencedAssemblies System.Drawing"])
            .env("RIFT_SRC", &src)
            .env("RIFT_EXE", &exe)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|st| st.success())
            .unwrap_or(false);
        if !ok || !exe.is_file() {
            s.log("compilation de RiftShot.exe impossible : capture classique seulement");
            return;
        }
        s.log("capture fidèle : RiftShot.exe compilé");
    }
    *s.shot_exe.lock().unwrap() = Some(exe);
}

pub fn start(s: Arc<Shared>, root: &Path, bin: &Path) {
    unsafe { SetProcessDPIAware() };
    let _ = std::fs::create_dir_all(bin);
    let (s1, s2, s3) = (s.clone(), s.clone(), s);
    let (src, exe) = (root.join("RiftShot.cs"), bin.join("RiftShot.exe"));
    thread::spawn(move || build_shot(s3, src, exe));
    thread::spawn(move || x_loop(s1));
    thread::spawn(move || hid_scan(s2));
}
