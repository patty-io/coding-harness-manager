fn main() {
    std::process::exit(coding_harness_manager_lib::credential_helper::run(
        std::env::args().skip(1),
    ));
}
