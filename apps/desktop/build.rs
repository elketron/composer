//! Bakes the CEF runtime's place into the binary (a default for `CEF_PATH` at run time, and the
//! rpath libcef.so is found by), and links libcef first. Packaging (0187) puts CEF beside the binary instead.

fn main() {
    println!("cargo::rerun-if-env-changed=CEF_PATH");
    let dir = std::env::var("CEF_PATH").unwrap_or_default();
    let dir = std::fs::canonicalize(&dir)
        .map(|p| p.display().to_string())
        .unwrap_or(dir);
    println!("cargo::rustc-env=COMPOSER_CEF_DIR={dir}");
    if !dir.is_empty() {
        println!("cargo::rustc-link-arg=-Wl,-rpath,{dir}");
        // libcef must load before libc: Chromium finds the real close() with
        // dlsym(RTLD_NEXT) from libcef and aborts at start-up when libc came first ("close symbol
        // missing"). This crate's own link-lib goes ahead of its dependencies' (std's -lc too).
        println!("cargo::rustc-link-search=native={dir}");
        println!("cargo::rustc-link-lib=dylib=cef");
    }
}
