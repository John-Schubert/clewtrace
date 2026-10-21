use std::process::Command;

// @verifies CT-TRC-008
#[test]
fn version_flag_prints_version_and_exits_zero() {
    for flag in ["--version", "-V"] {
        let out = Command::new(env!("CARGO_BIN_EXE_clew"))
            .arg(flag)
            .output()
            .unwrap();
        assert!(out.status.success(), "{flag} should exit 0");
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(stdout, format!("clew {}\n", env!("CARGO_PKG_VERSION")));
    }
}

// @verifies CT-TRC-008
#[test]
fn unknown_argument_exits_nonzero() {
    let out = Command::new(env!("CARGO_BIN_EXE_clew"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty());
}
