#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--credential-helper") {
        std::process::exit(coding_harness_manager_lib::credential_helper::run(args));
    }
    coding_harness_manager_lib::run();
}
