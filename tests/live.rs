// The editor must receive terminal escape sequences on a real PTY on both supported OSes.
#[test]
fn editor_and_mcp_share_the_same_scene() {
    let result = std::process::Command::new("python3")
        .args(["tests/workshop.py", env!("CARGO_BIN_EXE_spi3derr")])
        .output()
        .expect("python3 is required for the PTY acceptance test");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
