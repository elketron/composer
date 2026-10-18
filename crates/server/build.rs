// Rebuild when the embedded seed sites or app templates change (include_dir! doesn't track them).
fn main() {
    println!("cargo:rerun-if-changed=../../seed");
    println!("cargo:rerun-if-changed=../../templates");
}
