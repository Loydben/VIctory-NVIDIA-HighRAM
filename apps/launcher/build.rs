//! Embeds a manifest that asks for the Windows common controls 6 (the modern look of buttons, boxes and checkboxes)
//! and per-monitor DPI awareness, with the linker's own switches: no resource compiler and no extra crate.

fn main() {
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|e| e == "msvc");
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") && msvc {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
             version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    println!("cargo:rerun-if-changed=build.rs");
}
