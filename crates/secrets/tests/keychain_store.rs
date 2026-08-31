#[cfg(target_os = "macos")]
use chm_secrets::{KeychainStore, SecretStore};

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires a macOS login keychain session; run manually"]
fn keychain_set_get_delete_roundtrip() {
    let store = KeychainStore::new("chm-test");
    let key = format!("test/{}", std::process::id());
    store.set(&key, "supersecret").unwrap();
    assert_eq!(store.get(&key).unwrap(), Some("supersecret".to_string()));
    store.delete(&key).unwrap();
    assert_eq!(store.get(&key).unwrap(), None);
}
