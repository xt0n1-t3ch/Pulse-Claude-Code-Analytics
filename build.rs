fn main() {
    #[cfg(windows)]
    {
        if std::env::var("CARGO_BIN_NAME").is_ok() {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.compile().expect("failed to compile Windows resources");
        }
    }
}
