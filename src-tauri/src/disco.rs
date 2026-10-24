// «Qué ocupa el disco»: mide las carpetas que suelen crecer sin que uno se dé cuenta. Bajo pedido (du tarda).
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize, Clone)]
pub struct Carpeta {
    pub nombre: String,
    pub ruta: String,
    pub bytes: u64,
    pub consejo: String,
}

// ponytail: lista fija de sitios conocidos; se agrega uno aquí cuando aparezca otro que crezca.
const SITIOS: &[(&str, &str, &str)] = &[
    ("Descargas", "Downloads", "Instaladores y archivos viejos: casi siempre se pueden borrar."),
    ("Escritorio", "Desktop", ""),
    ("Documentos", "Documents", "Incluye proyectos con node_modules y compilados (target/)."),
    ("Imágenes", "Pictures", "La fototeca de Fotos vive aquí."),
    ("Películas", "Movies", ""),
    ("Música", "Music", ""),
    ("Papelera", ".Trash", "Vaciarla libera esto de inmediato."),
    ("Cachés de apps", "Library/Caches", "macOS y las apps las vuelven a crear; se pueden borrar."),
    ("Datos de apps", "Library/Application Support", "Datos de cada app (Claude, navegadores, Slack…)."),
    ("Apps con sandbox y Docker", "Library/Containers", "Docker guarda aquí sus imágenes: «docker system prune» libera."),
    ("Desarrollo de Apple", "Library/Developer", "Xcode y simuladores: DerivedData se puede borrar."),
    ("Rust (.cargo / .rustup)", ".cargo", ""),
    ("Rust toolchains", ".rustup", ""),
    ("npm", ".npm", "Caché de npm: «npm cache clean --force»."),
];

pub fn analizar() -> Vec<Carpeta> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let hilos: Vec<_> = SITIOS
        .iter()
        .map(|(nombre, rel, consejo)| {
            let ruta = home.join(rel);
            let (nombre, consejo) = (nombre.to_string(), consejo.to_string());
            std::thread::spawn(move || {
                let o = std::process::Command::new("du").arg("-skx").arg(&ruta).output().ok()?;
                let kb: u64 = String::from_utf8_lossy(&o.stdout).split_whitespace().next()?.parse().ok()?;
                Some(Carpeta { nombre, ruta: ruta.to_string_lossy().into_owned(), bytes: kb * 1024, consejo })
            })
        })
        .collect();
    let mut out: Vec<Carpeta> = hilos.into_iter().filter_map(|h| h.join().ok().flatten()).filter(|c| c.bytes > 0).collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

/// Solo abre en Finder rutas de la lista (nada que venga de fuera).
pub fn mostrar(ruta: &str) -> Result<(), String> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    if !SITIOS.iter().any(|(_, rel, _)| home.join(rel).to_string_lossy() == ruta) {
        return Err("ruta desconocida".into());
    }
    std::process::Command::new("open").arg(ruta).spawn().map(|_| ()).map_err(|e| e.to_string())
}
