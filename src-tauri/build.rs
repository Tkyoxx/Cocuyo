use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../overlay/src");
    println!("cargo:rerun-if-changed=../overlay/Cargo.toml");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let built = Command::new(cargo)
        .args(["build", "--release", "--lib", "--manifest-path", "../overlay/Cargo.toml"])
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .status()
        .expect("no se pudo ejecutar cargo para compilar overlay/");
    assert!(built.success(), "la librería de overlay/ no compiló");

    tauri_build::build()
}
