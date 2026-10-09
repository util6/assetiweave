use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=migrations");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let target_dir = manifest_dir.join("../builtin-assets/targets");
    println!("cargo:rerun-if-changed={}", target_dir.display());

    let mut target_files = fs::read_dir(&target_dir)
        .unwrap_or_else(|error| panic!("read target descriptors {}: {error}", target_dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    target_files.sort();

    let generated = target_files
        .iter()
        .map(|path| {
            println!("cargo:rerun-if-changed={}", path.display());
            format!("    include_str!({:?}),\n", path.to_string_lossy())
        })
        .collect::<String>();
    let generated_source =
        format!("const BUILTIN_TARGET_DESCRIPTORS: &[&str] = &[\n{generated}];\n");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("out dir"));
    fs::write(
        out_dir.join("builtin_target_descriptors.rs"),
        generated_source,
    )
    .expect("write generated target descriptor registry");

    tauri_build::build();
}
