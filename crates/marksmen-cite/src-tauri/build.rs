fn main() {
    if let Ok(path) = std::env::var("PATH") {
        std::env::set_var("PATH", format!("D:\\msys64\\ucrt64\\bin;{}", path));
    }

    // Statically link C/C++ runtimes to avoid DLL conflicts with Miniforge at runtime
    println!("cargo:rustc-link-arg=-static");
    println!("cargo:rustc-link-arg=-static-libgcc");
    println!("cargo:rustc-link-arg=-static-libstdc++");

    tauri_build::build();
}
