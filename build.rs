use std::process::Command;
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
fn main() {
    for name in [
        "HEAD",
        "packed-refs",
        &git(&["symbolic-ref", "-q", "HEAD"]).unwrap_or_default(),
    ] {
        if !name.is_empty() {
            if let Some(path) = git(&["rev-parse", "--git-path", name]) {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
    println!(
        "cargo:rustc-env=BUILD_HASH={}",
        git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into())
    );
}
