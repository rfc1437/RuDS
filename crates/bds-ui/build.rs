fn main() {
    println!("cargo:rerun-if-changed=assets/app-icons/bds.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/app-icons/bds.ico")
            .compile()
            .expect("compile Windows application icon");
    }
}
