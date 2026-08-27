use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;

fn main() {
    #[cfg(target_os = "macos")]
    let platform = Platform::MacOs;
    #[cfg(target_os = "windows")]
    let platform = Platform::Windows;
    #[cfg(all(unix, not(target_os = "macos")))]
    let platform = Platform::Linux;

    let inv = scan(platform, None, None);
    for i in &inv.installations {
        println!(
            "{} | {} | exe={:?} | version={:?} | config={:?}",
            i.harness_type.as_str(),
            i.status_v(),
            i.executable_path,
            i.version,
            i.config_path
        );
    }
    println!("total: {}", inv.installations.len());
}
