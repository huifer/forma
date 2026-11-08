//! RFC-0009 §16：`forma --version` 必须包含编译器版本与 IR 版本。

use std::process::Command;

#[test]
fn version_reports_compiler_and_ir_versions() {
    let out = Command::new(env!("CARGO_BIN_EXE_forma"))
        .arg("--version")
        .output()
        .expect("run forma --version");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "missing compiler version: {stdout}"
    );
    assert!(
        stdout.contains(forma::dsl::IR_VERSION),
        "missing IR version: {stdout}"
    );
    assert!(
        !stdout.contains("Specification Federation"),
        "stale v0.2 federation identity: {stdout}"
    );
}
