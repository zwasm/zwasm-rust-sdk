use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const HEADERS: [&str; 2] = ["zwasm.h", "wasi.h"];

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let zwasm_src_dir = env::current_dir()
        .expect("Failed to get current dir")
        .join("zwasm");

    println!("cargo:rerun-if-env-changed=DOCS_RS");

    // docs.rs builds in an offline sandbox that has no Zig toolchain, so the zwasm C
    // library cannot be built there. rustdoc never links the native library, so we
    // generate bindings straight from the vendored headers and skip the Zig build.
    let zwasm_include_dir = if env::var_os("DOCS_RS").is_some() {
        zwasm_src_dir.join("include")
    } else {
        build_zwasm(&out_dir, &zwasm_src_dir)
    };

    for header in HEADERS {
        let header_path = zwasm_include_dir.join(header);
        if !header_path.exists() {
            panic!(
                "Error: {} not found at {}.\n\
                The header file must be present for bindgen to generate Rust bindings.\n\
                Please ensure the zwasm C build step completed successfully and the headers are copied to the expected location.",
                header,
                header_path.display()
            );
        }
    }

    // Create a wrapper header file to include zwasm.h and wasi.h
    let wrapper = out_dir.join("wrapper.h");
    fs::write(&wrapper, "#include \"zwasm.h\"\n#include \"wasi.h\"\n")
        .expect("Failed to write wrapper.h");

    let target = std::env::var("TARGET").expect("TARGET not set");
    let bindings = bindgen::Builder::default()
        .header(wrapper.to_str().unwrap())
        .clang_arg(format!("-I{}", zwasm_include_dir.display()))
        .clang_arg(format!("--target={target}"))
        .allowlist_function("wasm_.*")
        .allowlist_function("zwasm_.*")
        .allowlist_type("wasm_.*")
        .allowlist_type("zwasm_.*")
        .allowlist_var("wasm_.*")
        .allowlist_var("zwasm_.*")
        .allowlist_var("ZWASM_.*")
        .generate()
        .expect("Unable to generate bindings with bindgen. Please check that the zwasm headers are valid and accessible.");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings.rs! Check write permissions and disk space.");
}

/// Builds the zwasm C library with Zig, emits the link directives, and returns the
/// path to the installed public header directory.
fn build_zwasm(out_dir: &Path, zwasm_src_dir: &Path) -> PathBuf {
    let zig_local_cache_dir = out_dir.join("zig-local-cache");
    let zig_global_cache_dir = out_dir.join("zig-global-cache");
    let zig_install_prefix = out_dir.join("zig-install");

    fs::create_dir_all(&zig_local_cache_dir).expect("Failed to create Zig local cache directory");
    fs::create_dir_all(&zig_global_cache_dir).expect("Failed to create Zig global cache directory");
    fs::create_dir_all(&zig_install_prefix).expect("Failed to create Zig install directory");

    // Check if zig is available
    if Command::new("zig").arg("--version").output().is_err() {
        panic!("Error: 'zig' command not found. Please install Zig and ensure it is in your PATH.");
    }

    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
    let os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not set");
    let abi = env::var("CARGO_CFG_TARGET_ENV").expect("CARGO_CFG_TARGET_ENV not set");
    let triple = if abi.is_empty() {
        format!("{arch}-{os}")
    } else {
        format!("{arch}-{os}-{abi}")
    };

    // Build zwasm C library using zig
    let status = Command::new("zig")
        .current_dir(zwasm_src_dir)
        .env("ZIG_LOCAL_CACHE_DIR", &zig_local_cache_dir)
        .env("ZIG_GLOBAL_CACHE_DIR", &zig_global_cache_dir)
        .arg("build")
        .arg("static-lib")
        .arg(format!("-Dtarget={triple}"))
        .arg("-Dcompiler-rt=true")
        .arg("-Doptimize=ReleaseSafe")
        .arg("-p")
        .arg(zig_install_prefix.to_str().unwrap())
        .status()
        .expect(
            "Failed to execute 'zig build static-lib'. Is Zig installed and zwasm source present?",
        );

    if !status.success() {
        panic!("Error: Failed to build zwasm C library with Zig. Please check the build output for details.");
    }

    let lib_dir = zig_install_prefix.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=zwasm");
    // zwasm references libm (trunc/truncf/...), which is a separate library on
    // glibc older than 2.34.
    println!("cargo:rustc-link-lib=m");
    // Zig-emitted objects carry no `.note.GNU-stack` section, so GNU ld assumes an
    // executable stack and warns (zwasm D-312).
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg=-Wl,-z,noexecstack");
    }

    zig_install_prefix.join("include")
}
