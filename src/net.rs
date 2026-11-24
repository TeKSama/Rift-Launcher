use std::path::Path;
use std::process::Command;

fn curl() -> Command {
    let mut c = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    hide(&mut c);
    c
}

pub fn hide(c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = c;
}

pub fn get(url: &str, headers: &[(&str, &str)]) -> Result<(u16, String), String> {
    let mut c = curl();
    c.args(["-sS", "-L", "--max-time", "20", "-w", "\n%{http_code}", "-H", "User-Agent: RiftLauncher"]);
    for (k, v) in headers {
        c.arg("-H").arg(format!("{k}: {v}"));
    }
    let out = c.arg(url).output().map_err(|e| format!("curl introuvable : {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    let code: u16 = code.trim().parse().unwrap_or(0);
    if code == 0 {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok((code, body.to_string()))
}

pub fn download(url: &str, dest: &Path) -> Result<(), String> {
    let out = curl()
        .args(["-sS", "-L", "-f", "--max-time", "120", "-H", "User-Agent: RiftLauncher", "-o"])
        .arg(dest)
        .arg(url)
        .output()
        .map_err(|e| format!("curl introuvable : {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn sha256(path: &Path) -> Option<String> {
    let out = if cfg!(windows) {
        let mut c = Command::new("certutil.exe");
        hide(&mut c);
        c.arg("-hashfile").arg(path).arg("SHA256").output().ok()?
    } else {
        Command::new("sha256sum").arg(path).output().ok()?
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.lines()
        .map(|l| l.split_whitespace().collect::<String>())
        .map(|l| l.chars().take_while(|c| c.is_ascii_hexdigit()).collect::<String>())
        .find(|h| h.len() == 64)
        .map(|h| h.to_uppercase())
}

pub fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn pick(folder: bool, title: &str, filter: &str, start: &str) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let dialog = if folder {
        "$d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = $env:RIFT_TITLE; if ($env:RIFT_START) { $d.SelectedPath = $env:RIFT_START }; if ($d.ShowDialog($o) -eq 'OK') { $d.SelectedPath }"
    } else {
        "$d = New-Object System.Windows.Forms.OpenFileDialog; $d.Title = $env:RIFT_TITLE; $d.Filter = $env:RIFT_FILTER; if ($env:RIFT_START) { $d.InitialDirectory = $env:RIFT_START }; if ($d.ShowDialog($o) -eq 'OK') { $d.FileName }"
    };
    let script = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $o = New-Object System.Windows.Forms.Form -Property @{{ TopMost = $true; ShowInTaskbar = $false; Opacity = 0 }}; $o.Show(); $o.Activate(); {dialog}; $o.Dispose()"
    );
    let mut c = Command::new("powershell.exe");
    hide(&mut c);
    let out = c
        .args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .env("RIFT_TITLE", title)
        .env("RIFT_FILTER", filter)
        .env("RIFT_START", start)
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().trim_start_matches('\u{feff}').to_string();
    (!path.is_empty()).then_some(path)
}

pub fn zip(dir: &Path, dest: &Path) -> Result<(), String> {
    let mut c = Command::new(if cfg!(windows) { "tar.exe" } else { "tar" });
    hide(&mut c);
    let out = c.arg("-a").arg("-c").arg("-f").arg(dest).arg("-C").arg(dir).arg(".").output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn version_parts(v: &str) -> Vec<u64> {
    let start = v.find(|c: char| c.is_ascii_digit()).unwrap_or(v.len());
    let num: String = v[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let mut parts: Vec<u64> = num.split('.').filter(|p| !p.is_empty()).take(4).map(|p| p.parse().unwrap_or(0)).collect();
    while parts.len() < 4 {
        parts.push(0);
    }
    parts
}

pub fn newer(remote: &str, current: &str) -> bool {
    let (r, c) = (version_parts(remote), version_parts(current));
    r > c || (r == c && current.contains('-') && !remote.contains('-'))
}

pub fn repo_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://github.com/")?;
    let mut it = rest.split('/');
    let owner = it.next()?;
    let repo = it.next()?.trim_end_matches(".git");
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    (ok(owner) && ok(repo)).then(|| format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_et_depots() {
        assert!(newer("2.0.0", "2.0.0-alpha.1"));
        assert!(!newer("2.0.0-alpha.1", "2.0.0"));
        assert!(!newer("1.7.1", "2.0.0-alpha.1"));
        assert!(newer("2.0.1", "2.0.0"));
        assert!(version_parts("v0.3.7") > version_parts("0.3.1"));
        assert!(version_parts("1.10") > version_parts("1.9.9"));
        assert_eq!(version_parts("rien"), vec![0, 0, 0, 0]);
        assert_eq!(repo_of("https://github.com/valentderah/totk-pc-recomp"), Some("valentderah/totk-pc-recomp".into()));
        assert_eq!(repo_of("https://github.com/a/b.git/tree/x"), Some("a/b".into()));
        assert_eq!(repo_of("https://evil.com/a/b"), None);
        assert_eq!(encode("Zelda: TotK"), "Zelda%3A%20TotK");
    }
}
