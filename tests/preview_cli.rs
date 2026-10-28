//! Exercise the exact standalone Preview executable exported to CI artifacts.
#![cfg(feature = "demo")]

use std::process::Command;

#[test]
fn preview_has_its_own_version_and_cannot_intercept_legacy_update_flags() {
    let executable = env!("CARGO_BIN_EXE_doremi-preview");
    let version = Command::new(executable).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        "Doremi Preview 0.1.0"
    );
    for argument in ["--apply-update", "--demo-update-feed", "--control"] {
        let result = Command::new(executable)
            .args([argument, "not-a-job"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
    }
    let help = Command::new(executable).arg("--help").output().unwrap();
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("no music playback")
    );
}
