fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/open-bim.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("../../assets/icons/open-bim.ico")
            .set("ProductName", "Open BIM")
            .set("FileDescription", "Open BIM")
            .set("OriginalFilename", "open-bim-desktop.exe")
            .compile_for("open-bim-desktop")
            .expect("failed to embed the Open BIM Windows icon");
    }
}
