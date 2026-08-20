use std::fs;
use std::path::PathBuf;

fn main() {
    stage_chrome_host_sidecar();
    tauri_build::build();
}

/// Tauri requires `externalBin` files to exist at compile time. `tauri build`
/// stages the real host in `beforeBuildCommand`. `cargo check` / tests get a
/// placeholder so they can run without bundling.
fn stage_chrome_host_sidecar() {
    let triple = std::env::var("TARGET").unwrap_or_else(|_| {
        std::env::var("HOST").expect("TARGET or HOST must be set for tauri-build")
    });
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dest_dir = manifest_dir.join("binaries");
    let dest = dest_dir.join(format!("crosspond-chrome-host-{triple}"));
    println!("cargo:rerun-if-changed={}", dest.display());

    if dest.is_file() && dest.metadata().is_ok_and(|meta| meta.len() > 0) {
        return;
    }

    let _ = fs::create_dir_all(&dest_dir);
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("../..").join("target"));
    let built = [
        target_dir
            .join(&triple)
            .join(&profile)
            .join("crosspond-chrome-host"),
        target_dir.join(&profile).join("crosspond-chrome-host"),
    ];
    for candidate in built {
        if candidate.is_file() {
            let _ = fs::copy(&candidate, &dest);
            return;
        }
    }

    let _ = fs::write(&dest, []);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&dest) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&dest, perms);
        }
    }
}
