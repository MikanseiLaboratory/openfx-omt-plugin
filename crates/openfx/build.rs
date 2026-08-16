use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=vendor/openfx");
    println!("cargo:rerun-if-env-changed=LIBCLANG_PATH");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg("-Ivendor/openfx")
        .blocklist_function("OfxGetNumberOfPlugins")
        .blocklist_function("OfxGetPlugin")
        .blocklist_type("OfxStatus")
        .blocklist_var("kOfxStat.+")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate_cstr(true)
        .layout_tests(false)
        .generate()
        .expect("Unable to generate OpenFX bindings (install LLVM/libclang and set LIBCLANG_PATH)");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
