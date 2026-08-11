use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_slangc() -> PathBuf {
    // Essaie d'abord slangc dans le PATH, sinon retombe sur le chemin VulkanSDK
    if Command::new("slangc").arg("-version").output().is_ok() {
        return PathBuf::from("slangc");
    }
    let vulkan_sdk =
        env::var("VULKAN_SDK").unwrap_or_else(|_| "C:/VulkanSDK/1.4.357.0".to_string());
    PathBuf::from(vulkan_sdk).join("bin").join("slangc.exe")
}

fn main() {
    let src_dir = Path::new("shaders/src");
    let bin_dir = Path::new("shaders/bin");

    fs::create_dir_all(bin_dir).expect("Failed to create shaders/bin directory");

    let slangc = find_slangc();

    for entry in fs::read_dir(src_dir).expect("Failed to find shaders/src") {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("slang") {
            let file_stem = path.file_stem().unwrap().to_str().unwrap();
            let output = bin_dir.join(format!("{file_stem}.spv"));

            println!("cargo:rerun-if-changed={}", path.display());

            let status = Command::new(&slangc)
                .arg(&path)
                .arg("-target")
                .arg("spirv")
                .arg("-profile")
                .arg("spirv_1_4")
                .arg("-emit-spirv-directly")
                .arg("-fvk-use-entrypoint-name")
                .arg("-entry")
                .arg("vertMain")
                .arg("-entry")
                .arg("fragMain")
                .arg("-o")
                .arg(&output)
                .status()
                .expect("échec de l'exécution de slangc");

            if !status.success() {
                panic!("Compilation du shader {} a échoué", path.display());
            }
        }
    }

    println!("cargo:rerun-if-changed=shaders/src");
}
