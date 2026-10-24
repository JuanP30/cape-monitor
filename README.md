# CAPE Monitor

Un cohete en la barra del Mac. Muestra memoria, procesador, disco, red y batería; avisa cuando algo es raro;
limpia solo lo **olvidado** (servidores y navegadores de pruebas huérfanos de más de 2 h) y deja que Claude
lo revise sin tocar nada sin permiso.

- **Abrir:** clic en el cohete o **⌘⇧M**. Arranca sola al encender el Mac.
- **Panel:** memoria (con presión), procesador por núcleo y temperatura del chip, red, disco, batería (salud como la
  muestra macOS, ciclos, temperatura), batería de ratón/teclado/AirPods, historial de 1 h / 24 h / 7 días
  (`~/.cape/monitor/historial.jsonl`, un punto por minuto) y «Qué ocupa el disco» bajo pedido.
- **Avisos:** un proceso al +80 % de CPU por 10 min, disco con menos de 10 % libre, memoria al límite,
  limpieza hecha, y cuando Claude pide cerrar algo.
- **Limpieza automática** cada 15 min, **solo** de olvidados (reglas y pruebas en `src-tauri/src/medir.rs`).
  Lo demás se cierra a mano desde el panel (botón Cerrar → ¿Seguro?). Registro: `~/.cape/monitor/limpieza.log`.
- **Claude:** `bash ~/.cape/cape-monitor` (resumen), `… json`, `… limpiar`, `… cerrar <pid> "<motivo>"`
  (la persona aprueba en el panel). El estado vive en `~/.cape/monitor/estado.json`.

## Construir

```bash
npx -y @tauri-apps/cli@2.12.1 build
cp -R "src-tauri/target/release/bundle/macos/CAPE Monitor.app" /Applications/
cp cliente/cape-monitor ~/.cape/cape-monitor
cd src-tauri && cargo test --release   # reglas de «olvidado» y batería
```

Plan y decisiones: [PLAN.md](PLAN.md).
