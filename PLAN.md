# CAPE Monitor: plan para revisar

Una app nuestra para la barra de arriba que reemplaza a «Usage» (de pago). Muestra cómo está el Mac,
encuentra lo que se queda gastando batería y **se conecta con Claude** para que cualquier sesión pueda
revisarlo y arreglarlo con tu permiso.

---

## 1. En la barra: casi nada de espacio

Un solo ícono pequeño. Tú eliges qué mostrar al lado (o nada):

```
 ◐ 21%          ← solo CPU
 ◐ 21% · 69%    ← CPU y memoria
 ◐              ← solo el ícono; se pinta ámbar o rojo si algo anda mal
```

Usage ocupa 4 espacios; esta ocupa 1. **Clic** (o un atajo de teclado) abre el panel.

## 2. El panel (mismo estilo de vidrio y esquinas redondeadas que CAPE SecretManager)

| Tarjeta | Qué muestra |
|---|---|
| **Memoria** | % usado, GB usados / total, *presión* de memoria (lo que de verdad importa en Mac) y swap |
| **Procesador** | % total y una barra por núcleo, como Usage |
| **Disco** | libre / total, y **qué ocupa más** (descargas, cachés, `node_modules` viejos…) con botón «revisar» |
| **Red** | Wi-Fi o cable, bajada y subida en vivo, IP local y pública |
| **Batería** *(nuevo)* | %, tiempo restante, salud y ciclos, **qué apps la gastan más** ahora |
| **Procesos** *(nuevo)* | Los 5 que más gastan, con **Cerrar** (pide confirmación) |
| **Olvidados** *(nuevo)* | Servidores de prueba y Chrome de pruebas huérfanos (como el `php` de 11 días). Botón **Limpiar todo** |
| **Claude** *(nuevo)* | Cuántas sesiones hay abiertas, cuánta memoria usan, y cuáles llevan días sin usarse |
| **Docker** *(nuevo)* | Si está prendido y cuánto gasta (es de lo que más pesa cuando no se usa) |

## 3. Trabaja solo: alertas y limpieza

- **Notificación** si algo pasa de 80 % de CPU por más de 10 minutos («`php` lleva 10 min al 96 %: ¿cerrar?»),
  si el disco pasa de 90 %, o si la memoria entra en presión alta.
- **Limpiador**: cada 15 min cierra **solo** lo que cumple todas las reglas de «olvidado»: huérfano, de
  pruebas, sin uso y de más de 2 h. Lo demás nunca lo toca: solo avisa.
- **Historial** de 7 días (CPU, memoria, batería) para ver «¿por qué ayer se descargó tan rápido?».

## 4. Conectado a Claude

- La app deja el estado del Mac en un archivo que **cualquier sesión de Claude puede leer**
  (`~/.cape/monitor/estado.json`): CPU, memoria, disco, batería, procesos pesados, olvidados.
- Le damos a Claude un comando, `cape-monitor`, para preguntar «¿cómo está el Mac?» o «¿qué está gastando
  batería?». Así cualquier sesión puede revisarlo, como hice hoy, sin buscar a mano.
- **Claude puede proponer** cerrar algo, pero **la app te pide confirmar** con una notificación
  («Claude quiere cerrar `php` (96 % CPU, 11 días). ¿Cerrar?»). Es la misma idea que CAPE SecretManager:
  Claude pide, tú apruebas.
- En la noche, un resumen opcional en tu `Tareas Activas`: «hoy se limpiaron 3 procesos olvidados; el
  disco está al 31 %».

## 5. Cómo se construye

- **Tauri 2**, igual que CAPE SecretManager: reusamos el panel, los estilos, el vidrio, el ícono en la barra y
  el arreglo de las dos pantallas.
- **Rust** con la librería `sysinfo` (CPU, memoria, disco, red, procesos) y los datos de batería de macOS.
- **Gasta muy poco**: mide cada 2 s con el panel abierto y cada 15 s cerrado. Meta: menos de 1 % de CPU y
  menos de 60 MB de memoria (Usage gasta más).
- Va en su propio repo privado `cape-monitor` y lo puede construir David también, en Windows, si quiere.

## 6. En qué orden

| Fase | Qué queda listo | Tiempo aprox. |
|---|---|---|
| 1 | Ícono + panel con Memoria, Procesador, Disco, Red, Batería | una sesión |
| 2 | Procesos, Olvidados, alertas y limpiador | una sesión |
| 3 | Conexión con Claude (`estado.json`, `cape-monitor`, aprobar desde notificación) | media sesión |
| 4 | Historial de 7 días, Docker, «qué ocupa el disco», resumen nocturno | una sesión |

Al terminar la fase 2 ya puedes **borrar Usage**. Lo hago yo cuando me digas: es solo arrastrarla a la
papelera y quitarla de los ítems de inicio.

---

## Para decidir mañana

1. **¿App aparte o dentro de CAPE SecretManager?** Recomiendo **aparte**: un candado para las claves y otro
   ícono para el Mac. Son cosas distintas y así cada una es simple. Si prefieres un solo ícono, se puede hacer
   un panel con dos pestañas.
2. **¿Qué quieres ver en la barra?** Solo el ícono, CPU, o CPU y memoria.
3. **¿Limpieza automática o solo avisar?** Recomiendo automática solo para «olvidados de prueba» y avisar para
   todo lo demás.
4. **Nombre:** «CAPE Monitor», o el que quieras.
