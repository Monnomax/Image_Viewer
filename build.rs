use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=data/com.example.ImgViewer.gschema.xml");

    let status = Command::new("glib-compile-schemas")
        .arg("data")
        .status()
        .expect("не вдалося запустити glib-compile-schemas — перевір, що пакет glib2.0-dev/libglib2.0-bin встановлено");

    if !status.success() {
        panic!("glib-compile-schemas завершився з помилкою — перевір data/*.gschema.xml");
    }
}