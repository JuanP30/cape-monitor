// Mide el Mac (CPU, memoria, disco, red, batería, procesos) y decide qué está «olvidado».
// Todo lo que lee la app y lo que lee Claude sale de aquí, en un solo `Estado`.
use serde::Serialize;
use std::process::Command;
use sysinfo::{Components, Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use std::time::{Duration, Instant};

#[derive(Serialize, Clone, Default)]
pub struct Estado {
    pub hora: u64,
    pub cpu: f32,
    pub nucleos: Vec<f32>,
    pub mem_usada: u64,
    pub mem_total: u64,
    pub swap_usada: u64,
    pub presion: u8, // 1 normal · 2 aviso · 4 crítica (lo que macOS llama «presión de memoria»)
    pub disco_libre: u64,
    pub disco_total: u64,
    pub red: Red,
    pub bateria: Option<Bateria>,
    pub top: Vec<Proceso>,
    pub olvidados: Vec<Proceso>,
    pub claude: Grupo,
    pub docker: Grupo,
    pub temp_chip: Option<f32>,
    pub temp_bateria: Option<f32>,
    pub temp_disco: Option<f32>,
    pub dispositivos: Vec<Dispositivo>, // ratón, teclado, AirPods… con su batería
}

#[derive(Serialize, Clone, Default)]
pub struct Dispositivo {
    pub nombre: String,
    pub pct: u8,
}

#[derive(Serialize, Clone, Default)]
pub struct Red {
    pub interfaz: String,
    pub ip_local: String,
    pub bajada: u64, // bytes por segundo
    pub subida: u64,
}

#[derive(Serialize, Clone, Default)]
pub struct Bateria {
    pub pct: u8,
    pub cargando: bool,
    pub enchufado: bool,
    pub restante: String,
    pub ciclos: Option<u32>,
    pub salud: Option<u8>,
}

#[derive(Serialize, Clone, Default)]
pub struct Proceso {
    pub pid: u32,
    pub nombre: String,
    pub cpu: f32,
    pub mem: u64,
    pub edad_s: u64,
    pub detalle: String, // para olvidados: qué es y de dónde salió
}

#[derive(Serialize, Clone, Default)]
pub struct Grupo {
    pub activo: bool,
    pub sesiones: u32,
    pub mem: u64,
    pub cpu: f32,
}

const DOS_HORAS: u64 = 2 * 3600;

/// ¿Es un servidor o navegador de pruebas que quedó huérfano? Devuelve qué es, o None.
/// Huérfano = su padre es launchd (pid 1): la sesión que lo abrió ya no existe.
/// ponytail: lista fija de patrones; se agrega uno aquí cuando aparezca otro tipo de resto.
pub fn olvidado(cmd: &str, padre: u32, edad_s: u64) -> Option<&'static str> {
    if padre != 1 || edad_s < DOS_HORAS {
        return None;
    }
    let c = cmd.to_lowercase();
    if c.contains("php") && c.contains(" -s ") {
        Some("servidor PHP de pruebas")
    } else if c.contains("python") && c.contains("http.server") {
        Some("servidor web de Python")
    } else if c.contains("google chrome for testing.app/contents/macos/") || c.contains("headless_shell") || c.contains("chrome-headless-shell") {
        Some("Chrome de pruebas automáticas")
    } else if c.contains("node") && (c.contains("vite") || c.contains("http-server") || c.contains("webpack serve")) {
        Some("servidor de desarrollo de Node")
    } else {
        None
    }
}

pub struct Medidor {
    sys: System,
    redes: Networks,
    discos: Disks,
    sensores: Components,
    ultima: Instant,
    bateria: Option<Bateria>,
    presion: u8,
    temps: (Option<f32>, Option<f32>, Option<f32>),
    dispositivos: Vec<Dispositivo>,
    salud: Option<u8>,
    cada_minuto: Option<Instant>,
    cada_10min: Option<Instant>,
}

impl Medidor {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        Medidor {
            sys,
            redes: Networks::new_with_refreshed_list(),
            discos: Disks::new_with_refreshed_list(),
            sensores: Components::new_with_refreshed_list(),
            ultima: Instant::now(),
            bateria: None,
            presion: 1,
            temps: (None, None, None),
            dispositivos: Vec::new(),
            salud: None,
            cada_minuto: None,
            cada_10min: None,
        }
    }

    pub fn medir(&mut self) -> Estado {
        let seg = self.ultima.elapsed().as_secs_f64().max(0.5);
        self.ultima = std::time::Instant::now();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory().with_cmd(UpdateKind::OnlyIfNotSet).with_exe(UpdateKind::OnlyIfNotSet),
        );
        self.redes.refresh(true);
        // Lo que cuesta lanzar un programa (batería, presión, sensores, disco): cada minuto.
        if self.cada_minuto.map_or(true, |t| t.elapsed() >= Duration::from_secs(60)) {
            self.cada_minuto = Some(Instant::now());
            self.discos.refresh(true);
            self.presion = leer_presion();
            self.sensores.refresh(true);
            self.temps = temperaturas(&self.sensores);
            self.dispositivos = parse_dispositivos(&salida("ioreg", &["-r", "-k", "BatteryPercent"]));
            // La salud que muestra macOS («Capacidad máxima» en Ajustes) sale de system_profiler: lento, cada 10 min.
            if self.cada_10min.map_or(true, |t| t.elapsed() >= Duration::from_secs(600)) {
                self.cada_10min = Some(Instant::now());
                self.salud = parse_salud(&salida("system_profiler", &["SPPowerDataType"]));
            }
            self.bateria = leer_bateria().map(|mut b| {
                if self.salud.is_some() {
                    b.salud = self.salud;
                }
                b
            });
        }

        let mio = std::process::id();
        let mut todos: Vec<Proceso> = Vec::new();
        let mut olvidados = Vec::new();
        let (mut claude, mut docker) = (Grupo::default(), Grupo::default());
        for (pid, p) in self.sys.processes() {
            let cmd = p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ");
            let exe = p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            let nombre = p.name().to_string_lossy().into_owned();
            let padre = p.parent().map(|x| x.as_u32()).unwrap_or(0);
            let pr = Proceso { pid: pid.as_u32(), nombre: nombre.clone(), cpu: p.cpu_usage(), mem: p.memory(), edad_s: p.run_time(), detalle: String::new() };
            if exe.contains("/Claude.app/") || exe.contains("/claude-code/") {
                claude.mem += pr.mem;
                claude.cpu += pr.cpu;
                claude.activo = true;
                if cmd.contains("stream-json") && !exe.contains("Helper") {
                    claude.sesiones += 1;
                }
            } else if exe.contains("/Docker.app/") || nombre.contains("Virtualization.VirtualMachine") {
                docker.mem += pr.mem;
                docker.cpu += pr.cpu;
                docker.activo = true;
            }
            if let Some(que) = olvidado(&format!("{exe} {cmd}"), padre, pr.edad_s) {
                olvidados.push(Proceso { detalle: format!("{que} · {}", resumir(&cmd)), ..pr.clone() });
            }
            if pr.pid != mio {
                todos.push(pr);
            }
        }
        todos.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.mem.cmp(&a.mem)));
        todos.truncate(6);

        let (disco_total, disco_libre) = self
            .discos
            .list()
            .iter()
            .find(|d| d.mount_point() == std::path::Path::new("/"))
            .map(|d| (d.total_space(), d.available_space()))
            .unwrap_or_default();

        Estado {
            hora: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            cpu: self.sys.global_cpu_usage(),
            nucleos: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            mem_usada: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            swap_usada: self.sys.used_swap(),
            presion: self.presion,
            disco_libre,
            disco_total,
            red: self.red(seg),
            bateria: self.bateria.clone(),
            top: todos,
            olvidados,
            claude,
            docker,
            temp_chip: self.temps.0,
            temp_bateria: self.temps.1,
            temp_disco: self.temps.2,
            dispositivos: self.dispositivos.clone(),
        }
    }

    fn red(&self, seg: f64) -> Red {
        // La interfaz con IPv4 que más tráfico movió (en un MacBook casi siempre en0 = Wi-Fi).
        let mut mejor: Option<(&String, &sysinfo::NetworkData)> = None;
        for (n, d) in self.redes.list() {
            let tiene_ip = d.ip_networks().iter().any(|i| i.addr.is_ipv4() && !i.addr.is_loopback());
            if !tiene_ip || !n.starts_with("en") {
                continue;
            }
            if mejor.map_or(true, |(_, m)| d.total_received() > m.total_received()) {
                mejor = Some((n, d));
            }
        }
        let Some((n, d)) = mejor else { return Red::default() };
        Red {
            interfaz: if n == "en0" { "Wi-Fi".into() } else { format!("Cable ({n})") },
            ip_local: d.ip_networks().iter().find(|i| i.addr.is_ipv4()).map(|i| i.addr.to_string()).unwrap_or_default(),
            bajada: (d.received() as f64 / seg) as u64,
            subida: (d.transmitted() as f64 / seg) as u64,
        }
    }
}

fn resumir(cmd: &str) -> String {
    let c: String = cmd.chars().take(90).collect();
    if cmd.chars().count() > 90 { format!("{c}…") } else { c }
}

fn salida(prog: &str, args: &[&str]) -> String {
    Command::new(prog).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Chip = el sensor de núcleo («tdie») más caliente; batería = «gas gauge»; disco = «NAND».
fn temperaturas(c: &Components) -> (Option<f32>, Option<f32>, Option<f32>) {
    let mut chip: Option<f32> = None;
    let (mut bat, mut disco) = (None, None);
    for s in c.list() {
        let (l, t) = (s.label().to_lowercase(), s.temperature());
        let Some(t) = t.filter(|t| *t > 0.0 && *t < 130.0) else { continue };
        if l.contains("tdie") {
            chip = Some(chip.map_or(t, |c| c.max(t)));
        } else if l.contains("battery") {
            bat = Some(t);
        } else if l.contains("nand") {
            disco = Some(t);
        }
    }
    (chip, bat, disco)
}

pub fn parse_salud(sp: &str) -> Option<u8> {
    sp.lines().find(|l| l.contains("Maximum Capacity"))?.split(':').nth(1)?.trim().trim_end_matches('%').trim().parse().ok()
}

pub fn parse_dispositivos(ioreg: &str) -> Vec<Dispositivo> {
    // Cada bloque de ioreg trae "Product" y "BatteryPercent"; se emparejan en orden.
    let mut out = Vec::new();
    let mut nombre: Option<String> = None;
    for l in ioreg.lines() {
        let l = l.trim();
        if let Some(v) = l.strip_prefix("\"Product\" = ") {
            nombre = Some(v.trim_matches('"').to_string());
        } else if let Some(v) = l.strip_prefix("\"BatteryPercent\" = ") {
            if let (Some(n), Ok(p)) = (nombre.take(), v.trim().parse()) {
                out.push(Dispositivo { nombre: n, pct: p });
            }
        }
    }
    out
}

fn leer_presion() -> u8 {
    salida("sysctl", &["-n", "kern.memorystatus_vm_pressure_level"]).trim().parse().unwrap_or(1)
}

pub fn leer_bateria() -> Option<Bateria> {
    parse_bateria(&salida("pmset", &["-g", "batt"]), &salida("ioreg", &["-rn", "AppleSmartBattery"]))
}

pub fn parse_bateria(pmset: &str, ioreg: &str) -> Option<Bateria> {
    let linea = pmset.lines().find(|l| l.contains("InternalBattery"))?;
    let pct = linea.split('\t').nth(1)?.split('%').next()?.trim().parse().ok()?;
    let num = |clave: &str| -> Option<u32> {
        ioreg.lines().find(|l| l.trim_start().starts_with(&format!("\"{clave}\" =")))?.split('=').nth(1)?.trim().parse().ok()
    };
    let salud = match (num("AppleRawMaxCapacity").or(num("NominalChargeCapacity")), num("DesignCapacity")) {
        (Some(m), Some(d)) if d > 0 => Some(((m as f32 / d as f32) * 100.0).round().min(100.0) as u8),
        _ => None,
    };
    let restante = linea.split(';').find(|s| s.contains("remaining")).map(|s| s.trim().split(' ').next().unwrap_or("").to_string()).filter(|s| s.contains(':') && s != "0:00").unwrap_or_default();
    Some(Bateria {
        pct,
        cargando: linea.contains("; charging"),
        enchufado: pmset.contains("AC Power"),
        restante,
        ciclos: num("CycleCount"),
        salud,
    })
}

/// Cierra un proceso con SIGTERM. Comprueba que el pid sigue siendo el mismo programa (los pid se reciclan).
pub fn cerrar(pid: u32, nombre: &str) -> Result<(), String> {
    let mut s = System::new();
    let p = Pid::from_u32(pid);
    s.refresh_processes_specifics(ProcessesToUpdate::Some(&[p]), true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    let proc_ = s.process(p).ok_or("ese proceso ya no existe")?;
    let n = proc_.name().to_string_lossy().into_owned();
    let exe = proc_.exe().and_then(|e| e.file_name()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    // Mismo programa = mismo nombre o mismo ejecutable (macOS a veces recorta el nombre).
    let mismo = !nombre.is_empty() && (n == nombre || exe == nombre || n.starts_with(nombre) || nombre.starts_with(n.as_str()));
    if pid <= 1 || !mismo {
        return Err("ese número ya es de otro programa; no lo cierro".into());
    }
    match proc_.kill_with(sysinfo::Signal::Term) {
        Some(true) => Ok(()),
        _ => Err("macOS no dejó cerrarlo (puede ser del sistema)".into()),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn olvidado_solo_si_es_huerfano_viejo_y_de_pruebas() {
        let php = "php -d post_max_size=16M -S 127.0.0.1:9227 -t /var/folders/x/T/grid";
        assert_eq!(olvidado(php, 1, 11 * 86400), Some("servidor PHP de pruebas"));
        assert_eq!(olvidado(php, 4321, 11 * 86400), None, "con dueño vivo no se toca");
        assert_eq!(olvidado(php, 1, 600), None, "recién abierto no se toca");
        assert!(olvidado("/opt/homebrew/bin/python3 -m http.server 48732", 1, 3 * 3600).is_some());
        assert!(olvidado("/Users/u/Library/Caches/ms-playwright/chromium-1229/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing --x", 1, 3 * 3600).is_some());
        // Programas propios que viven con padre 1 y NO son de pruebas.
        for cmd in ["/Users/u/.hermes/node/bin/node /Users/u/.hermes/node/lib/node_modules/openclaw/dist/index.js gateway --port 18789",
                    "/Applications/Docker.app/Contents/MacOS/com.docker.backend", "php artisan queue:work"] {
            assert_eq!(olvidado(cmd, 1, 30 * 86400), None, "{cmd}");
        }
    }

    #[test]
    fn bateria_de_pmset_e_ioreg() {
        let pm = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=7471203)\t82%; discharging; 4:12 remaining present: true\n";
        let io = "  \"CycleCount\" = 123\n  \"AppleRawMaxCapacity\" = 4380\n  \"DesignCapacity\" = 4563\n";
        let b = parse_bateria(pm, io).unwrap();
        assert_eq!((b.pct, b.cargando, b.enchufado, b.restante.as_str(), b.ciclos, b.salud), (82, false, false, "4:12", Some(123), Some(96)));
        let ac = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t95%; AC attached; not charging present: true\n";
        let b = parse_bateria(ac, "").unwrap();
        assert_eq!((b.pct, b.enchufado, b.cargando, b.restante.as_str()), (95, true, false, ""));
        assert!(parse_bateria("Now drawing from 'AC Power'\n", "").is_none(), "Mac de escritorio: sin batería");
    }

    #[test]
    fn salud_como_la_muestra_macos_y_dispositivos() {
        assert_eq!(parse_salud("      Health Information:\n          Cycle Count: 60\n          Condition: Normal\n          Maximum Capacity: 99 %\n"), Some(99));
        assert_eq!(parse_salud("sin batería"), None);
        let io = "+-o Mouse\n    {\n      \"Product\" = \"Juan Mouse\"\n      \"BatteryPercent\" = 72\n    }\n      \"Product\" = \"AirPods\"\n      \"BatteryPercent\" = 40\n";
        let d = parse_dispositivos(io);
        assert_eq!(d.iter().map(|x| (x.nombre.as_str(), x.pct)).collect::<Vec<_>>(), vec![("Juan Mouse", 72), ("AirPods", 40)]);
    }
}
