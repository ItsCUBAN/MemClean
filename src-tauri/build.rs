fn main() {
    let mut attributes = tauri_build::Attributes::new();

    // Solo la versión final pide administrador al abrirse. En desarrollo,
    // `npm run tauri dev` no podría lanzar un programa que exige elevación.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        attributes = attributes.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("memclean.manifest")),
        );
    }

    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("Error al preparar la compilación de Tauri: {e:#}");
    }
}
