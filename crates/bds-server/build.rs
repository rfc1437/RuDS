fn main() {
    // syntect and fastembed both reach onig_sys across separate Rust dylibs.
    // Keep the vendored static archive on this dylib's final native link line.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=onig.lib");
    } else {
        println!("cargo:rustc-link-arg=-lonig");
    }
}
