use std::path::PathBuf;

// The .proto files live at workspace_root/proto/, outside this crate's manifest dir. Cargo's
// default "rerun if any file inside the package changes" therefore does NOT cover them, and neither
// prost-build nor tonic-prost-build emits rerun-if-changed for us.
//
// We have to declare the dependency ourselves; pointing at the directory means new .proto files get
// picked up automatically.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Detect the workspace root by finding the parent Cargo.toml that has [workspace] defined.
    // Stunningly, cargo#3946 from 2017 exists for this but has never been implemented.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .ancestors()
        .find_map(|parent| {
            if let Ok(contents) = std::fs::read_to_string(parent.join("Cargo.toml"))
                && contents.lines().any(|line| line.trim() == "[workspace]")
            {
                return Some(parent.to_path_buf());
            }
            None
        })
        .expect("could not locate workspace Cargo.toml above this crate");
    let proto_root = workspace_root.join("proto");

    println!("cargo:rerun-if-changed={}", proto_root.display());

    let mut proto_files: Vec<PathBuf> = std::fs::read_dir(&proto_root)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("proto"))
        .collect();
    proto_files.sort();

    let fds = protox::compile(&proto_files, [&proto_root])?;
    tonic_prost_build::configure().compile_fds(fds)?;
    Ok(())
}
