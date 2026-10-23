// Build logic shared by every `*-sys` crate, `include!`d from each crate's build.rs.
//
// The C side is never built here: it links the prebuilt FFmpeg variant named by
// `CRUSTIFY_FFMPEG_VARIANT` (default `plain`) from `crustify/artifacts/build.json`, or the
// directory in `CRUSTIFY_FFMPEG_BUILD_DIR` for a private build.
//
// Bindings come from `wrapper.h` restricted to the crate's agent-owned
// `allowlist.txt` plus the orchestrator-owned `api-types.txt` (every public type). Items published by the link units in `deps` are blocklisted and
// reached through their crates' re-exports instead, so each C type has one Rust type.
// Types of out-of-tree libraries listed in `external-types.txt` are blocklisted too: the
// crate re-exports, under their C names, the safe layout-compatible types of the
// libraries' wrappers (crustify-rs/<source>-bindings), which its signatures then name.
// Allowlisted `static inline` functions get compiled wrappers (bindgen
// `wrap_static_fns`), and `shim.c` holds the `crustify_<NAME>` macro shims.

use std::env;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // <crate>/ -> repo root
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    manifest.ancestors().nth(1).unwrap().to_path_buf()
}

fn build_dir(root: &Path) -> PathBuf {
    println!("cargo:rerun-if-env-changed=CRUSTIFY_FFMPEG_BUILD_DIR");
    println!("cargo:rerun-if-env-changed=CRUSTIFY_FFMPEG_VARIANT");
    if let Ok(dir) = env::var("CRUSTIFY_FFMPEG_BUILD_DIR") {
        return PathBuf::from(dir);
    }
    let variant = env::var("CRUSTIFY_FFMPEG_VARIANT").unwrap_or_else(|_| "plain".into());
    let build_json = root.join("crustify/artifacts/build.json");
    println!("cargo:rerun-if-changed={}", build_json.display());
    let spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&build_json).expect("crustify/artifacts/build.json"))
            .expect("valid build.json");
    let dir = spec["variants"][&variant]["dir"]
        .as_str()
        .unwrap_or_else(|| panic!("no variant {variant:?} in crustify/artifacts/build.json"));
    root.join(dir)
}

/// Parse `allowlist.txt`: `function|type|var|item <regex>` per line, `#` comments.
fn allowlist(path: &Path) -> Vec<(String, String)> {
    println!("cargo:rerun-if-changed={}", path.display());
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap().trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (kind, pat) = l.split_once(char::is_whitespace).expect("`<kind> <regex>`");
            (kind.to_string(), pat.trim().to_string())
        })
        .collect()
}

/// Parse `api-types.txt`: one type regex per line, `#` comments.
fn allowlist_types(path: &Path) -> Vec<String> {
    println!("cargo:rerun-if-changed={}", path.display());
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// `lib`: link unit (e.g. "libavcodec"); `deps`: link units it links against.
pub fn build(lib: &str, deps: &[&str]) {
    let root = repo_root();
    let build = build_dir(&root);
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    assert!(
        build.join(lib).join(format!("{lib}.so")).exists(),
        "{lib}.so missing under {}: build the variant first (crustify/artifacts/build.json)",
        build.display()
    );

    let includes = [root.join("c"), build.clone(), manifest.clone()];
    let mut b = bindgen::Builder::default()
        .header(manifest.join("wrapper.h").to_str().unwrap())
        .clang_args(includes.iter().map(|i| format!("-I{}", i.display())))
        .allowlist_item("crustify_.*")
        .default_enum_style(bindgen::EnumVariation::NewType { is_bitfield: false, is_global: false })
        .derive_default(true)
        .generate_comments(true)
        .wrap_static_fns(true)
        .wrap_static_fns_path(out.join("static_fns"))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    for (kind, pat) in allowlist(&manifest.join("allowlist.txt")) {
        b = match kind.as_str() {
            "function" => b.allowlist_function(pat),
            "type" => b.allowlist_type(pat),
            "var" => b.allowlist_var(pat),
            "item" => b.allowlist_item(pat),
            k => panic!("allowlist.txt: unknown kind {k:?}"),
        };
    }
    for pat in allowlist_types(&manifest.join("api-types.txt")) {
        b = b.allowlist_type(pat);
    }
    for pat in allowlist_types(&manifest.join("external-types.txt")) {
        b = b.blocklist_type(pat);
    }
    for d in deps {
        // Definitions in the dependency's headers, and forward declarations of its
        // types in ours, both belong to the dependency's crate.
        b = b.blocklist_file(format!(".*/{d}/[^/]*\\.h"));
        for pat in allowlist_types(&manifest.join(format!("../{d}-sys/api-types.txt"))) {
            b = b.blocklist_type(pat);
        }
    }
    // bindgen writes static_fns.c only when the allowlist reaches a static
    // inline function, so drop a previous build's copy rather than compile it.
    let static_fns = out.join("static_fns.c");
    let _ = std::fs::remove_file(&static_fns);
    b.generate()
        .expect("bindgen")
        .write_to_file(out.join("bindings.rs"))
        .expect("write bindings.rs");

    let mut cc = cc::Build::new();
    cc.file(manifest.join("shim.c"))
        .includes(&includes)
        .include(&manifest)
        .warnings(false);
    if static_fns.exists() {
        cc.file(&static_fns);
    }
    cc.compile(&format!("crustify_{lib}_shim"));
    println!("cargo:rerun-if-changed={}", manifest.join("shim.c").display());
    println!("cargo:rerun-if-changed={}", manifest.join("shim.h").display());
    println!("cargo:rerun-if-changed={}", manifest.join("wrapper.h").display());

    let lib_dir = build.join(lib);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib={}", &lib[3..]);
    // DT_RPATH, not DT_RUNPATH: it also resolves the FFmpeg libraries' own NEEDED entries.
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:lib_dir={}", lib_dir.display());
}
