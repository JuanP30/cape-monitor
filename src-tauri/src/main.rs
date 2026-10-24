// CAPE Monitor — un cohete en la barra. Mide el Mac, avisa cuando algo es raro, limpia solo lo «olvidado»
// (servidores y navegadores de pruebas huérfanos) y deja el estado en ~/.cape/monitor/estado.json para
// que cualquier Claude lo lea. Claude puede PEDIR cerrar algo (cliente/cape-monitor); la persona aprueba aquí.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod disco;
mod historial;
mod medir;

use medir::{Estado, Medidor, Proceso};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

const CPU_ALTA: f32 = 80.0;
const CPU_ALTA_MIN: u64 = 10;
const LIMPIEZA_CADA: Duration = Duration::from_secs(15 * 60);

#[derive(Serialize, Deserialize, Clone)]
struct Solicitud {
    #[serde(default)]
    id: String,
    accion: String, // «cerrar» (la persona aprueba) o «limpiar» (solo olvidados: se hace sin preguntar)
    #[serde(default)]
    pid: u32,
    #[serde(default)]
    nombre: String,
    #[serde(default)]
    motivo: String,
}

#[derive(Default)]
struct Compartido {
    estado: Estado,
    solicitudes: Vec<Solicitud>,
    limpiados: Vec<String>, // últimas limpiezas, para el panel
}

struct App {
    datos: Mutex<Compartido>,
    visible: AtomicBool,
}

fn carpeta() -> PathBuf {
    let d = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".cape/monitor");
    let _ = std::fs::create_dir_all(d.join("solicitudes"));
    let _ = std::fs::create_dir_all(d.join("respuestas"));
    d
}

fn avisar(app: &AppHandle, titulo: &str, cuerpo: &str) {
    let _ = app.notification().builder().title(titulo).body(cuerpo).show();
}

fn anotar(linea: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(carpeta().join("limpieza.log")) {
        let _ = writeln!(f, "{} {linea}", hora_texto());
    }
}

fn hora_texto() -> String {
    String::from_utf8_lossy(&std::process::Command::new("date").arg("+%Y-%m-%d %H:%M").output().map(|o| o.stdout).unwrap_or_default()).trim().to_string()
}

fn responder(id: &str, ok: bool, detalle: &str) {
    let r = serde_json::json!({ "ok": ok, "detalle": detalle });
    let _ = std::fs::write(carpeta().join("respuestas").join(format!("{id}.json")), r.to_string());
    let _ = std::fs::remove_file(carpeta().join("solicitudes").join(format!("{id}.json")));
}

fn limpiar_olvidados(app: &AppHandle, olvidados: &[Proceso], quien: &str) -> Vec<String> {
    let mut hechos = Vec::new();
    for p in olvidados {
        if medir::cerrar(p.pid, &p.nombre).is_ok() {
            let t = format!("{} ({}) · {}", p.nombre, p.pid, p.detalle);
            anotar(&format!("[{quien}] cerrado {t}"));
            hechos.push(t);
        }
    }
    if !hechos.is_empty() {
        // Resumen por tipo: «255 servidor PHP de pruebas, 1 Chrome de pruebas automáticas».
        let mut tipos: Vec<(String, usize)> = Vec::new();
        for p in olvidados {
            let t = p.detalle.split(" · ").next().unwrap_or("").to_string();
            match tipos.iter_mut().find(|(x, _)| *x == t) { Some((_, n)) => *n += 1, None => tipos.push((t, 1)) }
        }
        let txt: Vec<_> = tipos.iter().map(|(t, n)| format!("{n} {t}")).collect();
        avisar(app, &format!("Limpié {} procesos olvidados", hechos.len()), &txt.join(", "));
        let st = app.state::<App>();
        let mut d = st.datos.lock().unwrap();
        for h in &hechos {
            d.limpiados.insert(0, format!("{} · {h}", hora_texto()));
        }
        d.limpiados.truncate(8);
    }
    hechos
}

// El corazón: mide, guarda, avisa y limpia. Cada 2 s con el panel abierto, cada 15 s cerrado.
fn bucle(app: AppHandle) {
    let mut m = Medidor::new();
    let mut cpu_alta: HashMap<u32, (Instant, bool)> = HashMap::new();
    let mut aviso_disco = Instant::now() - Duration::from_secs(86400);
    let mut aviso_presion = Instant::now() - Duration::from_secs(3600);
    let mut ultima_limpieza = Instant::now() - LIMPIEZA_CADA + Duration::from_secs(120); // la primera, a los 2 min
    let mut ultima = Instant::now() - Duration::from_secs(60);
    let mut vistas: Vec<String> = Vec::new();
    let archivo_hist = carpeta().join("historial.jsonl");
    let mut ultimo_punto = Instant::now() - Duration::from_secs(120);
    let mut ultimo_recorte = Instant::now() - Duration::from_secs(7200);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let st = app.state::<App>();
        let cada = if st.visible.load(Ordering::Relaxed) { 2 } else { 15 };
        if ultima.elapsed() < Duration::from_secs(cada) {
            continue;
        }
        ultima = Instant::now();
        let e = m.medir();

        // Para Claude: el estado en un archivo (escritura atómica, solo tu usuario lo lee).
        let dir = carpeta();
        if let Ok(j) = serde_json::to_string_pretty(&e) {
            let tmp = dir.join("estado.json.tmp");
            if std::fs::write(&tmp, j).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
                }
                let _ = std::fs::rename(&tmp, dir.join("estado.json"));
            }
        }

        // Historial: un punto por minuto; se recorta a 7 días cada hora.
        if ultimo_punto.elapsed() >= Duration::from_secs(60) {
            ultimo_punto = Instant::now();
            historial::agregar(&archivo_hist, &historial::punto(&e));
        }
        if ultimo_recorte.elapsed() >= Duration::from_secs(3600) {
            ultimo_recorte = Instant::now();
            historial::recortar(&archivo_hist, e.hora);
        }

        // Alerta: un proceso al +80 % de CPU por 10 minutos (una vez por proceso).
        let ahora: Vec<u32> = e.top.iter().filter(|p| p.cpu > CPU_ALTA).map(|p| p.pid).collect();
        cpu_alta.retain(|pid, _| ahora.contains(pid));
        for p in e.top.iter().filter(|p| p.cpu > CPU_ALTA) {
            let (desde, avisado) = cpu_alta.entry(p.pid).or_insert((Instant::now(), false));
            if !*avisado && desde.elapsed() >= Duration::from_secs(CPU_ALTA_MIN * 60) {
                *avisado = true;
                avisar(&app, &format!("{} lleva {CPU_ALTA_MIN} min al {:.0} %", p.nombre, p.cpu), "Abre CAPE Monitor (el cohete) si quieres cerrarlo.");
            }
        }
        if e.disco_total > 0 && (e.disco_libre as f64) < e.disco_total as f64 * 0.10 && aviso_disco.elapsed() > Duration::from_secs(86400) {
            aviso_disco = Instant::now();
            avisar(&app, "El disco está casi lleno", &format!("Quedan {:.0} GB libres.", e.disco_libre as f64 / 1e9));
        }
        if e.presion >= 4 && aviso_presion.elapsed() > Duration::from_secs(3600) {
            aviso_presion = Instant::now();
            avisar(&app, "Memoria al límite", "El Mac se está quedando sin memoria. Cierra apps o sesiones que no uses.");
        }

        // Limpieza automática: SOLO olvidados.
        if ultima_limpieza.elapsed() >= LIMPIEZA_CADA && !e.olvidados.is_empty() {
            ultima_limpieza = Instant::now();
            limpiar_olvidados(&app, &e.olvidados, "automática");
        } else if ultima_limpieza.elapsed() >= LIMPIEZA_CADA {
            ultima_limpieza = Instant::now();
        }

        // Pedidos de Claude (cliente/cape-monitor). «limpiar» se hace ya; «cerrar» espera a la persona.
        let mut pendientes = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir.join("solicitudes")) {
            for f in rd.flatten() {
                let Ok(txt) = std::fs::read_to_string(f.path()) else { continue };
                let Ok(mut s) = serde_json::from_str::<Solicitud>(&txt) else { continue };
                s.id = f.path().file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
                if s.accion == "limpiar" {
                    let n = limpiar_olvidados(&app, &e.olvidados, "pedido de Claude");
                    responder(&s.id, true, &format!("{} procesos olvidados cerrados", n.len()));
                    continue;
                }
                if !vistas.contains(&s.id) {
                    vistas.push(s.id.clone());
                    avisar(&app, "Claude pide cerrar un proceso", &format!("{} ({}). {} — Abre CAPE Monitor para aprobar.", s.nombre, s.pid, s.motivo));
                }
                pendientes.push(s);
            }
        }

        let mut d = st.datos.lock().unwrap();
        d.estado = e;
        d.solicitudes = pendientes;
        drop(d);
        let _ = app.emit("estado", ());
    }
}

#[derive(Serialize)]
struct Vista {
    estado: Estado,
    solicitudes: Vec<Solicitud>,
    limpiados: Vec<String>,
}

#[tauri::command]
fn estado(st: State<App>) -> Vista {
    let d = st.datos.lock().unwrap();
    Vista { estado: d.estado.clone(), solicitudes: d.solicitudes.clone(), limpiados: d.limpiados.clone() }
}

#[tauri::command]
fn cerrar(pid: u32, nombre: String) -> Result<(), String> {
    let r = medir::cerrar(pid, &nombre);
    if r.is_ok() {
        anotar(&format!("[a mano] cerrado {nombre} ({pid})"));
    }
    r
}

#[tauri::command]
fn limpiar(app: AppHandle, st: State<App>) -> usize {
    let olv = st.datos.lock().unwrap().estado.olvidados.clone();
    limpiar_olvidados(&app, &olv, "a mano").len()
}

#[tauri::command]
fn decidir(st: State<App>, id: String, aprobar: bool) -> Result<(), String> {
    let s = st.datos.lock().unwrap().solicitudes.iter().find(|s| s.id == id).cloned().ok_or("esa solicitud ya no está")?;
    if !aprobar {
        responder(&id, false, "la persona lo rechazó");
        return Ok(());
    }
    let r = medir::cerrar(s.pid, &s.nombre);
    match &r {
        Ok(()) => {
            anotar(&format!("[aprobado, pedido de Claude] cerrado {} ({}) · {}", s.nombre, s.pid, s.motivo));
            responder(&id, true, "cerrado");
        }
        Err(e) => responder(&id, false, e),
    }
    st.datos.lock().unwrap().solicitudes.retain(|x| x.id != id);
    r
}

#[tauri::command]
fn historial(horas: u64) -> Vec<historial::Punto> {
    let ahora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    historial::resumir(&historial::leer(&carpeta().join("historial.jsonl")), ahora, horas.clamp(1, 168), 168)
}

#[tauri::command]
async fn analizar_disco() -> Vec<disco::Carpeta> {
    tauri::async_runtime::spawn_blocking(disco::analizar).await.unwrap_or_default()
}

#[tauri::command]
fn mostrar_carpeta(ruta: String) -> Result<(), String> {
    disco::mostrar(&ruta)
}

#[cfg(target_os = "macos")]
fn colocar_mac(w: &tauri::WebviewWindow, desde_icono: bool) -> Option<()> {
    use objc2_app_kit::{NSEvent, NSScreen, NSWindow};
    use objc2_foundation::NSPoint;
    let mtm = objc2::MainThreadMarker::new()?;
    let m = NSEvent::mouseLocation();
    let pantalla = NSScreen::screens(mtm).iter().find(|s| {
        let f = s.frame();
        m.x >= f.origin.x && m.x <= f.origin.x + f.size.width && m.y >= f.origin.y && m.y <= f.origin.y + f.size.height
    })?;
    let v = pantalla.visibleFrame();
    let ventana = unsafe { &*(w.ns_window().ok()? as *const NSWindow) };
    let ancho = ventana.frame().size.width;
    let max_x = v.origin.x + v.size.width - ancho - 10.0;
    let x = if desde_icono { (m.x - ancho / 2.0).clamp(v.origin.x + 10.0, max_x) } else { max_x };
    ventana.setFrameTopLeftPoint(NSPoint::new(x, v.origin.y + v.size.height - 6.0));
    Some(())
}

fn alternar_panel(app: &AppHandle, desde_icono: bool) {
    let Some(w) = app.get_webview_window("panel") else { return };
    let st = app.state::<App>();
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        st.visible.store(false, Ordering::Relaxed);
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = colocar_mac(&w, desde_icono);
    #[cfg(not(target_os = "macos"))]
    let _ = desde_icono;
    st.visible.store(true, Ordering::Relaxed);
    let _ = w.emit("panel-mostrar", ());
    let _ = w.show();
    let _ = w.set_focus();
}

fn main() {
    tauri::Builder::default()
        .manage(App { datos: Mutex::new(Compartido::default()), visible: AtomicBool::new(false) })
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _atajo, evento| {
                    if evento.state() == ShortcutState::Pressed {
                        alternar_panel(app, false);
                    }
                })
                .build(),
        )
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            #[cfg(target_os = "macos")]
            if let Some(w) = app.get_webview_window("panel") {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
                let _ = apply_vibrancy(&w, NSVisualEffectMaterial::Popover, Some(NSVisualEffectState::Active), Some(14.0));
                if let Ok(ns) = w.ns_window() {
                    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior as B};
                    let ventana = unsafe { &*(ns as *const NSWindow) };
                    ventana.setCollectionBehavior(B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::Stationary);
                }
            }
            // Arranca sola al encender el Mac.
            let auto = app.autolaunch();
            if !auto.is_enabled().unwrap_or(true) {
                let _ = auto.enable();
            }
            // ⌘⇧M abre el panel aunque el cohete quede tapado por la muesca.
            app.global_shortcut().register(Shortcut::new(Some((if cfg!(target_os = "macos") { Modifiers::SUPER } else { Modifiers::CONTROL }) | Modifiers::SHIFT), Code::KeyM))?;
            TrayIconBuilder::with_id("cape-monitor")
                .icon(Image::from_bytes(include_bytes!("../icons/barra.png"))?)
                .icon_as_template(true)
                .tooltip("CAPE Monitor")
                .on_tray_icon_event(|tray, evento| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = evento {
                        alternar_panel(tray.app_handle(), true);
                    }
                })
                .build(app)?;
            let h = app.handle().clone();
            std::thread::spawn(move || bucle(h));
            Ok(())
        })
        .on_window_event(|ventana, evento| {
            if let WindowEvent::Focused(false) = evento {
                let _ = ventana.hide();
                ventana.app_handle().state::<App>().visible.store(false, Ordering::Relaxed);
            }
        })
        .invoke_handler(tauri::generate_handler![estado, cerrar, limpiar, decidir, historial, analizar_disco, mostrar_carpeta])
        .run(tauri::generate_context!())
        .expect("no pude arrancar CAPE Monitor");
}

