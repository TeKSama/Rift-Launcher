use crate::json::{parse, Json};
use std::io::{Read, Write};

pub const BUILTIN_ID: &str = "1568357665183375550";

pub trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

pub struct Conn {
    pipe: Box<dyn Pipe>,
}

fn open(i: u32) -> Option<Box<dyn Pipe>> {
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{i}")).ok().map(|f| Box::new(f) as Box<dyn Pipe>)
    }
    #[cfg(unix)]
    {
        let dir = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().find_map(std::env::var_os).unwrap_or_else(|| "/tmp".into());
        let s = std::os::unix::net::UnixStream::connect(std::path::Path::new(&dir).join(format!("discord-ipc-{i}"))).ok()?;
        let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
        Some(Box::new(s))
    }
}

impl Conn {
    pub fn connect(client_id: &str) -> Result<Conn, String> {
        for i in 0..10 {
            let Some(pipe) = open(i) else { continue };
            let mut c = Conn { pipe };
            c.send(0, &Json::obj().with("v", 1.0).with("client_id", client_id))?;
            let (op, reply) = c.read()?;
            if op == 2 || reply.str("evt") == "ERROR" {
                return Err(format!("Discord refuse la connexion : {}", reply.to_string()));
            }
            return Ok(c);
        }
        Err("Discord introuvable (Discord est-il ouvert ?)".into())
    }

    fn send(&mut self, op: u32, v: &Json) -> Result<(), String> {
        let data = v.to_string().into_bytes();
        let mut frame = Vec::with_capacity(8 + data.len());
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(data.len() as u32).to_le_bytes());
        frame.extend_from_slice(&data);
        self.pipe.write_all(&frame).and_then(|_| self.pipe.flush()).map_err(|e| e.to_string())
    }

    fn read(&mut self) -> Result<(u32, Json), String> {
        let mut head = [0u8; 8];
        self.pipe.read_exact(&mut head).map_err(|e| e.to_string())?;
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let mut body = vec![0u8; len.min(1 << 20)];
        self.pipe.read_exact(&mut body).map_err(|e| e.to_string())?;
        Ok((op, parse(&String::from_utf8_lossy(&body)).unwrap_or(Json::Null)))
    }

    pub fn set_activity(&mut self, activity: Json, nonce: &str) -> Result<(), String> {
        let args = Json::obj().with("pid", std::process::id() as f64).with("activity", activity);
        self.send(1, &Json::obj().with("cmd", "SET_ACTIVITY").with("args", args).with("nonce", nonce))?;
        let (op, reply) = self.read()?;
        if op == 2 || reply.str("evt") == "ERROR" {
            return Err(format!("Discord refuse le statut : {}", reply.to_string()));
        }
        Ok(())
    }
}

pub fn clip(t: &str, max: usize) -> String {
    let mut t = t.to_string();
    if t.chars().count() < 2 {
        t.push_str("  ");
    }
    if t.chars().count() > max {
        t = t.chars().take(max - 1).collect::<String>() + "…";
    }
    t
}
