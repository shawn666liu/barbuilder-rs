fn main() {
    cxx_build::bridge("src/barbuilderpp.rs")
        .std("c++14")
        .compile("barbuilderpp");

    println!("cargo:rerun-if-changed=src/barbuilderpp.rs");
}
