use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../assets/techbase_icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // A window icon alone does not give the executable a File Explorer icon.
    // Embed the same multi-resolution TechBase asset as its first icon group.
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../assets/techbase_icon.ico");
    let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("sleufbase-icon.rc");
    let icon_path = icon.to_string_lossy().replace('\\', "/");
    fs::write(&resource, format!("1 ICON \"{icon_path}\"\n"))
        .expect("Could not prepare the TechBase icon resource");
    embed_resource::compile_for(&resource, ["sleufbase-rust"], embed_resource::NONE)
        .manifest_required()
        .expect("The Windows executable must contain the TechBase icon");
}
