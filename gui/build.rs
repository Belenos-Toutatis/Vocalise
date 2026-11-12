//! Sous Windows : icône et informations de version intégrées à vocalise-gui.exe.
//! (Compilé depuis Linux pour vérification, l'exécutable n'a pas d'icône : sans importance.)

fn main() {
    println!("cargo:rerun-if-changed=assets/vocalise.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/vocalise.ico")
            .set("ProductName", "Vocalise")
            .set("FileDescription", "Vocalise : synthèse vocale avec votre propre voix");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icône Windows non intégrée : {e}");
        }
    }
}
