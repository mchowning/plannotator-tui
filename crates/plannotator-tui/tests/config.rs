//! `plannotator-tui config --json` is how the pi thread runner reads the config, so its shape
//! is an interface: checked through the real binary.

#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use std::process::Command;

#[test]
fn config_json_carries_the_thread_context_from_the_file() {
    let dir = std::env::temp_dir().join(format!("plannotator-tui-config-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("config.toml");
    std::fs::write(&path, "[threads]\ncontext = \"fresh\"\n").expect("config");
    let out = Command::new(env!("CARGO_BIN_EXE_plannotator-tui"))
        .env("PLANNOTATOR_TUI_CONFIG", &path)
        .env_remove("PLANNOTATOR_TUI_THEME")
        .args(["config", "--json"])
        .output()
        .expect("config runs");
    std::fs::remove_dir_all(&dir).expect("cleanup");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is JSON alone");
    assert_eq!(json["threads"]["context"], "fresh");
    assert_eq!(json["herdr"]["placement"], "overlay", "the rest is the effective config");
}
