use crate::{scene::Scene, store};
use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
#[derive(Clone, PartialEq, Eq)]
struct Stamp(u64, i64, i64, u64);
fn stamp(p: &Path) -> io::Result<Stamp> {
    let m = fs::metadata(p)?;
    Ok(Stamp(m.ino(), m.mtime(), m.mtime_nsec(), m.len()))
}
pub struct Session {
    pub path: PathBuf,
    base: String,
    seen: Option<Stamp>,
}
impl Session {
    pub fn default_path() -> PathBuf {
        std::env::var_os("SPI3DERR_SESSION")
            .map(PathBuf::from)
            .unwrap_or_else(|| store::root().join("../workshop.3draft"))
    }
    fn lock(&self) -> Result<fs::File, String> {
        let p = self.path.with_file_name(format!(
            ".{}.lock",
            self.path
                .file_name()
                .ok_or("invalid session path")?
                .to_string_lossy()
        ));
        if let Some(dir) = p.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?
        }
        let f = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(p)
            .map_err(|e| e.to_string())?;
        f.lock().map_err(|e| e.to_string())?;
        Ok(f)
    }
    pub fn open(path: PathBuf, initial: &Scene) -> Result<(Self, Scene), String> {
        let mut session = Self {
            path,
            base: String::new(),
            seen: None,
        };
        let _guard = session.lock()?;
        let data = match fs::read_to_string(&session.path) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let s = initial.write();
                store::atomic(&session.path, &s).map_err(|e| e.to_string())?;
                s
            }
            Err(e) => return Err(e.to_string()),
        };
        let scene = Scene::read(&data)?;
        session.base = data;
        session.seen = stamp(&session.path).ok();
        Ok((session, scene))
    }
    pub fn refresh(&mut self) -> Result<Option<Scene>, String> {
        let meta = stamp(&self.path).map_err(|e| e.to_string())?;
        if self.seen.as_ref() == Some(&meta) {
            return Ok(None);
        }
        let data = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        self.seen = Some(meta);
        if data == self.base {
            return Ok(None);
        }
        let scene = Scene::read(&data)?;
        self.base = data;
        Ok(Some(scene))
    }
    pub fn last_valid(&self) -> Scene {
        Scene::read(&self.base).expect("session baseline is a validated document")
    }
    pub fn publish(&mut self, s: &Scene) -> Result<(), String> {
        let _guard = self.lock()?;
        let now = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        if now != self.base {
            return Err(
                "scene conflict: another writer changed the document; retry after reload".into(),
            );
        }
        let data = s.write();
        store::atomic(&self.path, &data).map_err(|e| e.to_string())?;
        self.base = data;
        self.seen = stamp(&self.path).ok();
        Ok(())
    }
    pub fn revision(&self) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for &b in self.base.as_bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(1099511628211)
        }
        format!("{hash:016x}")
    }
}
