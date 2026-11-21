// Rebuild when the embedded seed sites change (include_dir! doesn't track them).
fn main() {
    println!("cargo:rerun-if-changed=../../seed");
}
