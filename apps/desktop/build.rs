//! Links libcef and tells the app where the CEF runtime is. A development build bakes the shared
//! runtime's place in (a default for `CEF_PATH`, and an rpath); a packaged build
//! (`COMPOSER_PACKAGE=1`, scripts/package-desktop.sh) has CEF beside the binary and bakes no
//! path in: it finds libcef through `$ORIGIN`.

fn main() {
    println!("cargo::rerun-if-env-changed=CEF_PATH");
    println!("cargo::rerun-if-env-changed=COMPOSER_PACKAGE");
    let packaged = std::env::var_os("COMPOSER_PACKAGE").is_some();
    let dir = std::env::var("CEF_PATH").unwrap_or_default();
    let dir = std::fs::canonicalize(&dir)
        .map(|p| p.display().to_string())
        .unwrap_or(dir);
    // Beside the binary first (a package), then the development runtime.
    println!("cargo::rustc-link-arg=-Wl,-rpath,$ORIGIN");
    if packaged {
        println!("cargo::rustc-env=COMPOSER_CEF_DIR=");
    } else {
        println!("cargo::rustc-env=COMPOSER_CEF_DIR={dir}");
    }
    if !dir.is_empty() {
        if !packaged {
            println!("cargo::rustc-link-arg=-Wl,-rpath,{dir}");
        }
        // libcef must load before libc: Chromium finds the real close() with
        // dlsym(RTLD_NEXT) from libcef and aborts at start-up when libc came first ("close symbol
        // missing"). This crate's own link-lib goes ahead of its dependencies' (std's -lc too).
        println!("cargo::rustc-link-search=native={dir}");
        println!("cargo::rustc-link-lib=dylib=cef");
    }
}
