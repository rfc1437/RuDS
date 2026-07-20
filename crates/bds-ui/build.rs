fn main() {
    println!("cargo:rerun-if-changed=assets/app-icons/bds.ico");

    // Wry and bds-core both reach objc2's native exception helper across
    // separate Rust dylibs, so retain the static helper on the UI link line.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-lobjc2_exception_helper_0_1");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/app-icons/bds.ico")
            .compile()
            .expect("compile Windows application icon");
    }
}
