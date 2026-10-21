fn main() {
    println!("cargo:rerun-if-changed=assets/icons/app.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icons/app.ico");
        res.set("ProductName", "Mechanical Click");
        res.set(
            "FileDescription",
            "Mechanical Click - keyboard sound engine",
        );
        res.set("LegalCopyright", "MIT licensed");
        if let Err(err) = res.compile() {
            println!("cargo:warning=icon resource skipped: {err}");
        }
    }
}
