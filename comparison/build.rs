//! Builds the jbigkit shim and links the system's jbigkit
//! (`apt-get install libjbig-dev`).

/// Compiles `csrc/jbig_shim.c` and links `libjbig`.
fn main() {
    println!("cargo:rerun-if-changed=csrc/jbig_shim.c");
    cc::Build::new().file("csrc/jbig_shim.c").opt_level(3).compile("jbig_shim");
    println!("cargo:rustc-link-lib=jbig");
}
