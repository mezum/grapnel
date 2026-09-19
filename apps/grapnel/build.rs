fn main() {
    // Rebuild when translations (rust-i18n embeds them), the icons or the resource script change.
    println!("cargo:rerun-if-changed=../../locales");
    println!("cargo:rerun-if-changed=../../assets");
    println!("cargo:rerun-if-changed=grapnel.rc");
    embed_resource::compile("grapnel.rc", embed_resource::NONE).manifest_optional().unwrap();
}
