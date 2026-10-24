// Historial de 7 días: una línea JSON por minuto en ~/.cape/monitor/historial.jsonl (~1 MB a la semana).
// ponytail: archivo plano reescrito cada hora; SQLite si algún día hace falta más de una semana o consultas.
use crate::medir::Estado;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub const SIETE_DIAS: u64 = 7 * 86400;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct Punto {
    pub t: u64,
    pub cpu: f32,
    pub mem: f32, // % de memoria usada
    #[serde(default)]
    pub bat: Option<u8>,
    #[serde(default)]
    pub enchufado: bool,
    #[serde(default)]
    pub temp: Option<f32>,
    pub bajada: u64,
    pub subida: u64,
}

pub fn punto(e: &Estado) -> Punto {
    Punto {
        t: e.hora,
        cpu: e.cpu,
        mem: if e.mem_total > 0 { e.mem_usada as f32 / e.mem_total as f32 * 100.0 } else { 0.0 },
        bat: e.bateria.as_ref().map(|b| b.pct),
        enchufado: e.bateria.as_ref().map_or(true, |b| b.enchufado),
        temp: e.temp_chip,
        bajada: e.red.bajada,
        subida: e.red.subida,
    }
}

pub fn agregar(archivo: &Path, p: &Punto) {
    if let (Ok(mut f), Ok(l)) = (std::fs::OpenOptions::new().create(true).append(true).open(archivo), serde_json::to_string(p)) {
        let _ = writeln!(f, "{l}");
    }
}

pub fn leer(archivo: &Path) -> Vec<Punto> {
    std::fs::read_to_string(archivo).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// Deja solo los últimos 7 días.
pub fn recortar(archivo: &Path, ahora: u64) {
    let quedan: Vec<String> = leer(archivo).iter().filter(|p| p.t + SIETE_DIAS >= ahora).filter_map(|p| serde_json::to_string(p).ok()).collect();
    let tmp = archivo.with_extension("tmp");
    if std::fs::write(&tmp, quedan.join("\n") + if quedan.is_empty() { "" } else { "\n" }).is_ok() {
        let _ = std::fs::rename(tmp, archivo);
    }
}

/// Las últimas `horas`, promediadas en a lo sumo `max` puntos (para dibujar sin pesar).
pub fn resumir(puntos: &[Punto], ahora: u64, horas: u64, max: usize) -> Vec<Punto> {
    let desde = ahora.saturating_sub(horas * 3600);
    let ventana = (horas * 3600 / max as u64).max(60);
    let mut out: Vec<Punto> = Vec::new();
    let mut grupo: Vec<&Punto> = Vec::new();
    let cerrar = |g: &mut Vec<&Punto>, out: &mut Vec<Punto>| {
        if g.is_empty() { return; }
        let n = g.len() as f32;
        let prom = |f: &dyn Fn(&Punto) -> f32| g.iter().map(|p| f(p)).sum::<f32>() / n;
        let bats: Vec<u8> = g.iter().filter_map(|p| p.bat).collect();
        let temps: Vec<f32> = g.iter().filter_map(|p| p.temp).collect();
        out.push(Punto {
            t: g[g.len() - 1].t,
            cpu: prom(&|p| p.cpu),
            mem: prom(&|p| p.mem),
            bat: if bats.is_empty() { None } else { Some((bats.iter().map(|&b| b as u32).sum::<u32>() / bats.len() as u32) as u8) },
            enchufado: g.iter().all(|p| p.enchufado),
            temp: if temps.is_empty() { None } else { Some(temps.iter().sum::<f32>() / temps.len() as f32) },
            bajada: prom(&|p| p.bajada as f32) as u64,
            subida: prom(&|p| p.subida as f32) as u64,
        });
        g.clear();
    };
    let mut caja = u64::MAX;
    for p in puntos.iter().filter(|p| p.t >= desde) {
        let c = (p.t - desde) / ventana;
        if c != caja { cerrar(&mut grupo, &mut out); caja = c; }
        grupo.push(p);
    }
    cerrar(&mut grupo, &mut out);
    out
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn guarda_recorta_y_resume() {
        let dir = std::env::temp_dir().join(format!("cape-hist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("h.jsonl");
        let ahora = 10 * 86400;
        for i in 0..(8 * 24) {  // un punto por hora durante 8 días
            agregar(&f, &Punto { t: ahora - i * 3600, cpu: 10.0, mem: 50.0, bat: Some(80), ..Default::default() });
        }
        recortar(&f, ahora);
        assert!(leer(&f).iter().all(|p| p.t + SIETE_DIAS >= ahora));
        assert_eq!(leer(&f).len(), 7 * 24 + 1);
        let r = resumir(&leer(&f), ahora, 24, 12);
        assert!(r.len() <= 13 && !r.is_empty());
        assert!(r.iter().all(|p| p.cpu == 10.0 && p.bat == Some(80)));
        std::fs::remove_dir_all(dir).ok();
    }
}
