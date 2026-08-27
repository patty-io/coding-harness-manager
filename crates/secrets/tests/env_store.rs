use chm_secrets::{EnvStore, SecretStore};

#[test]
fn env_store_reads_process_environment() {
    unsafe { std::env::set_var("CHM_TEST_SECRET", "hello") };
    let store = EnvStore;
    assert_eq!(
        store.get("CHM_TEST_SECRET").unwrap(),
        Some("hello".to_string())
    );
    assert_eq!(store.get("CHM_TEST_MISSING").unwrap(), None);
}

#[test]
fn env_store_is_read_only() {
    let store = EnvStore;
    assert!(
        store.set("CHM_TEST_SECRET", "x").is_err(),
        "env refs are user-managed"
    );
    assert!(store.delete("CHM_TEST_SECRET").is_err());
}
