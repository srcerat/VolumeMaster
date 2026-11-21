fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let res_path = std::path::Path::new(&manifest_dir).join("resources").join("icon.res");
    let res_str = res_path.to_str().unwrap().replace('\\', "/");
    println!("cargo:rustc-link-arg={}", res_str);
    println!("cargo:rerun-if-changed=resources/icon.res");
}
