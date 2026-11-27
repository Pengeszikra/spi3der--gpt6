use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub fn root() -> PathBuf {
    std::env::var_os("SPI3DER_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".spi3derr/store")
        })
}
fn name(n: &str) -> io::Result<&str> {
    if n.is_empty()
        || n == "."
        || n == ".."
        || n.chars()
            .any(|c| !(c.is_alphanumeric() || " -_.".contains(c)))
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid store name",
        ))
    } else {
        Ok(n)
    }
}
pub fn path(n: &str) -> io::Result<PathBuf> {
    Ok(root().join(format!("{}.3draft", name(n)?)))
}
pub fn atomic(p: &Path, data: &str) -> io::Result<()> {
    atomic_bytes(p, data.as_bytes())
}
pub fn atomic_bytes(p: &Path, data: &[u8]) -> io::Result<()> {
    let dir = p
        .parent()
        .filter(|x| !x.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = dir.join(format!(".save-{}-{stamp}", std::process::id()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        fs::rename(&tmp, p)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
pub fn list() -> io::Result<Vec<String>> {
    let mut a = vec![];
    if !root().exists() {
        return Ok(a);
    }
    for e in fs::read_dir(root())? {
        let p = e?.path();
        if p.extension().and_then(|x| x.to_str()) == Some("3draft") {
            a.push(p.file_stem().unwrap().to_string_lossy().into_owned());
        }
    }
    a.sort();
    Ok(a)
}
pub fn catalog() -> Vec<(String, PathBuf)> {
    let mut map = std::collections::BTreeMap::new();
    let mut dirs = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            dirs.push(p.join("models"))
        }
    }
    dirs.extend(["models".into(), root()]);
    for dir in dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(n) = p.file_name().and_then(|s| s.to_str()).and_then(|s| {
                    s.strip_suffix(".scene.json")
                        .or_else(|| s.strip_suffix(".3draft"))
                }) {
                    map.insert(n.to_owned(), p);
                }
            }
        }
    }
    map.into_iter().collect()
}
pub fn resolve(n: &str) -> Result<PathBuf, String> {
    let p = Path::new(n);
    if p.is_file() {
        return Ok(p.into());
    }
    catalog()
        .into_iter()
        .find(|(name, p)| name.eq_ignore_ascii_case(n) || p.file_name().is_some_and(|f| f == n))
        .map(|(_, p)| p)
        .ok_or_else(|| format!("unknown scene: {n}"))
}
