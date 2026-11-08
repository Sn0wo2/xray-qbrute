fn main() {
    let width = acta_build::walk_src_max_width("src", "src/");
    println!("cargo:rustc-env=ACTA_PATH_WIDTH={width}");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
}
