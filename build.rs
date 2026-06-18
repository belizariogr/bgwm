fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon/bgwm.ico");
        if let Err(error) = res.compile() {
            eprintln!("failed to embed Windows application icon: {error}");
            std::process::exit(1);
        }
    }
}
