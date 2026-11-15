//! Intègre l'icône et les informations de version dans l'exécutable Windows
//! (Explorateur, barre des tâches, propriétés du fichier).

const ICON: &str = "../../assets/icons/icon.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(ICON)
        .set("ProductName", "SSH Manager")
        .set(
            "FileDescription",
            "SSH Manager — connexions SSH, SFTP et FTP",
        )
        .set("OriginalFilename", "ssh-manager.exe");
    if let Err(error) = resource.compile() {
        // Sans outil de ressources (rc.exe), l'exécutable reste utilisable :
        // seule son icône manque. On le signale sans bloquer la compilation.
        println!("cargo:warning=icône non intégrée à l'exécutable : {error}");
    }
}
