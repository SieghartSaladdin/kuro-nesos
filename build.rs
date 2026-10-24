// Embeds the app icon and version info (kuro-nesos.rc) into the executable.
fn main() {
    println!("cargo:rerun-if-changed=kuro-nesos.rc");
    println!("cargo:rerun-if-changed=assets/kuro-nesos.ico");
    embed_resource::compile("kuro-nesos.rc", embed_resource::NONE);
}
