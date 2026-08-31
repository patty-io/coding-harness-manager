use std::process::Command;
use uuid::Uuid;

#[test]
fn helper_rejects_unknown_credential_without_leaking_identifier() {
    let data_dir = tempfile::tempdir().expect("temp data directory");
    let output = Command::new(env!("CARGO_BIN_EXE_chm-credential-helper"))
        .args(["read", "--credential-ref", Uuid::new_v4().to_string().as_str()])
        .env("CHM_DATA_DIR", data_dir.path())
        .output()
        .expect("run credential helper");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("credential reference"));
    assert!(!stderr.contains("credential_ref"));
}

#[test]
fn helper_rejects_malformed_requests() {
    let output = Command::new(env!("CARGO_BIN_EXE_chm-credential-helper"))
        .args(["write", "--credential-ref", &Uuid::new_v4().to_string()])
        .output()
        .expect("run credential helper");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
