use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=../../assets/aegis-vault.ico");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let icon_path = manifest_dir.join("../../assets/aegis-vault.ico");
    let icon_path = icon_path
        .canonicalize()
        .unwrap_or_else(|error| panic!("icon not found at {}: {error}", icon_path.display()));

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let rc_path = out_dir.join("aegis-vault.rc");
    let res_path = out_dir.join("aegis-vault.res");
    let icon_path_for_rc = icon_path.to_string_lossy().replace('\\', "/");

    fs::write(&rc_path, format!("1 ICON \"{icon_path_for_rc}\"\n"))
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", rc_path.display()));

    let rc_compiler = find_resource_compiler().unwrap_or_else(|| {
        panic!("could not find Windows resource compiler. Install LLVM with llvm-rc.exe or set RC.")
    });

    let output = Command::new(&rc_compiler)
        .arg("/FO")
        .arg(&res_path)
        .arg(&rc_path)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", rc_compiler.display()));

    if !output.status.success() {
        panic!(
            "resource compiler failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!(
        "cargo:rustc-link-arg-bin=encrypt-app={}",
        res_path.display()
    );
}

fn find_resource_compiler() -> Option<PathBuf> {
    if let Ok(path) = env::var("RC") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    find_on_path("llvm-rc.exe")
        .or_else(|| find_on_path("rc.exe"))
        .or_else(common_llvm_rc_path)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|path| path.join(name))
            .find(|candidate| candidate.exists())
    })
}

fn common_llvm_rc_path() -> Option<PathBuf> {
    [
        env::var_os("ProgramFiles"),
        env::var_os("ProgramFiles(x86)"),
    ]
    .into_iter()
    .flatten()
    .map(PathBuf::from)
    .map(|path| path.join(Path::new("LLVM").join("bin").join("llvm-rc.exe")))
    .find(|candidate| candidate.exists())
}
