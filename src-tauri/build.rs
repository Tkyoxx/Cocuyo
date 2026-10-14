use std::process::Command;

fn main() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    for library in ["overlay", "barra"] {
        println!("cargo:rerun-if-changed=../{library}/src");
        println!("cargo:rerun-if-changed=../{library}/Cargo.toml");
        let built = Command::new(&cargo)
            .args(["build", "--release", "--lib", "--manifest-path", &format!("../{library}/Cargo.toml")])
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("RUSTFLAGS")
            .status()
            .unwrap_or_else(|_| panic!("no se pudo ejecutar cargo para compilar {library}/"));
        assert!(built.success(), "la librería de {library}/ no compiló");
    }

    tauri_build::build()
}
