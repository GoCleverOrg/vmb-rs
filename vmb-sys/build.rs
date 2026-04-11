use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VIMBA_X_HOME");

    // The `sdk` feature gates actual SDK discovery + linking.
    // Without it, this build script is a no-op so workspace builds succeed
    // even when the Vimba SDK is not installed.
    if env::var("CARGO_FEATURE_SDK").is_err() {
        return;
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if target_os == "macos" {
        // On macOS, VmbC ships as a framework. Default lookup is
        // `/Library/Frameworks`, but users on Nix or with non-standard
        // installs can override via `VIMBA_X_HOME`, which is treated as a
        // directory that contains `VmbC.framework`.
        let framework_dir = env::var("VIMBA_X_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/Library/Frameworks"));
        if !framework_dir.join("VmbC.framework").exists() {
            panic!(
                "VmbC.framework not found at {}/VmbC.framework. \
                 Install the Vimba X SDK from Allied Vision, or set VIMBA_X_HOME \
                 to a directory that contains `VmbC.framework`.",
                framework_dir.display()
            );
        }
        println!(
            "cargo:rustc-link-search=framework={}",
            framework_dir.display()
        );
        println!("cargo:rustc-link-lib=framework=VmbC");
        return;
    }

    // Linux (and other unix): locate SDK via VIMBA_X_HOME with a default fallback.
    let vimba_home =
        env::var("VIMBA_X_HOME").unwrap_or_else(|_| "/usr/local/VimbaX_2023-4".to_string());
    let vimba_home = PathBuf::from(vimba_home);

    if !vimba_home.exists() {
        panic!(
            "Vimba X SDK not found at {}. Set VIMBA_X_HOME to the SDK install root.",
            vimba_home.display()
        );
    }

    let arch_dir = match target_arch.as_str() {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => panic!("unsupported target architecture for VmbC: {other}"),
    };

    // Vimba X SDK layouts vary across versions and installers:
    //   * Older / multi-arch installers: `${VIMBA_X_HOME}/api/lib/${arch}/libVmbC.so`
    //   * Newer / single-arch installers (e.g. Vimba X 2023-4 Linux x86_64):
    //     `${VIMBA_X_HOME}/api/lib/libVmbC.so` (flat, no arch subdir)
    //
    // Probe the arch-nested layout first for backward compatibility, then
    // fall back to the flat layout. If neither exists, panic with both
    // candidate paths so the operator can see what was tried.
    let arch_lib_dir = vimba_home.join("api").join("lib").join(arch_dir);
    let flat_lib_dir = vimba_home.join("api").join("lib");

    let lib_dir = if arch_lib_dir.join("libVmbC.so").exists() {
        arch_lib_dir
    } else if flat_lib_dir.join("libVmbC.so").exists() {
        flat_lib_dir
    } else {
        panic!(
            "libVmbC.so not found in {} or {}. \
             Verify the Vimba X SDK install and set VIMBA_X_HOME to the SDK install root.",
            arch_lib_dir.display(),
            flat_lib_dir.display()
        );
    };

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=VmbC");
}
