fn main() {
    // Incorpora l'icona nell'eseguibile Windows (Esplora file, barra delle applicazioni).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().expect("impossibile incorporare l'icona");
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
