use std::path::Path;

fn main() {
    // Bundle training exported from the app, if present (see starter/README.md).
    let starter = Path::new("../starter/kit-organizer-config.json");
    println!("cargo:rerun-if-changed=../starter");
    println!("cargo:rerun-if-changed=../starter/kit-organizer-config.json");
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("starter.json");
    let content = std::fs::read_to_string(starter).unwrap_or_else(|_| "null".into());
    if content.trim() != "null" {
        // Fail the build early on a broken file rather than at the user's first launch.
        serde_json::from_str::<serde_json::Value>(&content).expect("starter/kit-organizer-config.json is not valid JSON");
        println!("cargo:warning=Bundling learned rules from starter/kit-organizer-config.json");
    }
    std::fs::write(&out, content).expect("write starter bundle");
    tauri_build::build()
}
