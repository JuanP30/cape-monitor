// CAPE Monitor — panel. Pide el estado a Rust cada 2 s (Rust ya lo tiene medido) y lo pinta.
// Todo texto que viene de procesos se pinta con textContent.
const { invoke } = window.__TAURI__.core;
const vista = document.getElementById('vista');
const salud = document.getElementById('salud');
const pestanas = document.getElementById('pestanas');
let seccion = 'resumen';
const NS = 'http://www.w3.org/2000/svg';

function h(tag, attrs = {}, ...hijos) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) k.startsWith('on') ? e.addEventListener(k.slice(2), v) : e.setAttribute(k, v);
  for (const c of hijos.flat()) if (c != null && c !== false) e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  return e;
}
const gb = (b) => (b / 1e9).toLocaleString('es', { maximumFractionDigits: b >= 100e9 ? 0 : 1 }) + ' GB';
const gib = (b) => (b / 2 ** 30).toLocaleString('es', { maximumFractionDigits: 1 }) + ' GB'; // memoria: como la cuenta macOS
const mb = (b) => (b >= 2 ** 30 ? gib(b) : Math.round(b / 2 ** 20) + ' MB');
const velocidad = (b) => (b >= 1e6 ? (b / 1e6).toFixed(1) + ' MB/s' : Math.round(b / 1e3) + ' kB/s');
const pct = (x) => Math.round(x) + ' %';
const edad = (s) => (s >= 86400 ? `${Math.floor(s / 86400)} d` : s >= 3600 ? `${Math.floor(s / 3600)} h` : `${Math.floor(s / 60)} min`);

// Dona de varios segmentos [{v, color}] sobre 100.
function dona(segmentos) {
  const s = document.createElementNS(NS, 'svg');
  s.setAttribute('viewBox', '0 0 42 42'); s.setAttribute('class', 'dona');
  const anillo = (color, largo, desde) => {
    const c = document.createElementNS(NS, 'circle');
    Object.entries({ cx: 21, cy: 21, r: 15.9, fill: 'none', stroke: color, 'stroke-width': 6, 'stroke-dasharray': `${largo} ${100 - largo}`,
      'stroke-dashoffset': 25 - desde, 'stroke-linecap': largo > 0 && largo < 100 ? 'butt' : 'butt' }).forEach(([k, v]) => c.setAttribute(k, v));
    s.append(c);
  };
  anillo('var(--pista)', 100, 0);
  let d = 0;
  for (const { v, color } of segmentos) { anillo(color, Math.max(0, Math.min(100 - d, v)), d); d += v; }
  return s;
}

let ultimo = null, ipPublica = '', ipCuando = 0;
let hist = [], histHoras = 24, histCuando = 0;
let disco = null, analizando = false; // «Qué ocupa el disco»

async function cargarHistorial(forzar) {
  if (!forzar && Date.now() - histCuando < 60_000) return;
  histCuando = Date.now();
  try { hist = await invoke('historial', { horas: histHoras }); } catch { hist = []; }
  pintar();
}

// Gráfica de área con línea (0–100 o 0–máx). Devuelve un SVG que ocupa todo el ancho.
function grafica(valores, color, max = 100) {
  const s = document.createElementNS(NS, 'svg');
  s.setAttribute('viewBox', '0 0 100 30'); s.setAttribute('preserveAspectRatio', 'none'); s.setAttribute('class', 'graf');
  const vs = valores.filter((v) => v != null);
  if (vs.length < 2) return s;
  const n = valores.length - 1;
  const pts = valores.map((v, i) => v == null ? null : `${(i / n) * 100},${30 - Math.min(1, v / max) * 28}`).filter(Boolean);
  const area = document.createElementNS(NS, 'polygon');
  area.setAttribute('points', `0,30 ${pts.join(' ')} 100,30`); area.setAttribute('fill', color); area.setAttribute('opacity', '.18');
  const linea = document.createElementNS(NS, 'polyline');
  Object.entries({ points: pts.join(' '), fill: 'none', stroke: color, 'stroke-width': 1.2, 'vector-effect': 'non-scaling-stroke', 'stroke-linejoin': 'round' })
    .forEach(([k, v]) => linea.setAttribute(k, v));
  s.append(area, linea);
  return s;
}

function bloqueHistorial() {
  const serie = (f) => hist.map(f);
  const prom = (xs) => { const v = xs.filter((x) => x != null); return v.length ? v.reduce((a, b) => a + b, 0) / v.length : null; };
  const maxRed = Math.max(1, ...hist.map((p) => p.bajada));
  const filas = [
    ['Procesador', serie((p) => p.cpu), 'var(--m500)', 100, (v) => `prom. ${pct(v)}`],
    ['Memoria', serie((p) => p.mem), 'var(--teal)', 100, (v) => `prom. ${pct(v)}`],
    ['Batería', serie((p) => p.bat), 'var(--ok)', 100, (v) => `prom. ${Math.round(v)} %`],
    ['Temperatura', serie((p) => p.temp), 'var(--warn)', 100, (v) => `prom. ${Math.round(v)} °C`],
    ['Red (bajada)', serie((p) => p.bajada), 'var(--azul)', maxRed, (v) => `prom. ${velocidad(v)}`],
  ].filter(([, v]) => v.some((x) => x != null));
  const tab = (h_, txt) => h('button', { class: `tab ${histHoras === h_ ? 'on' : ''}`, onclick: () => { histHoras = h_; cargarHistorial(true); } }, txt);
  return [
    h('div', { class: 'sec' }, 'Historial', h('span', { class: 'tabs' }, tab(1, '1 h'), tab(24, '24 h'), tab(168, '7 días'))),
    h('div', { class: 'lista' }, filas.length
      ? filas.map(([nombre, vals, color, max, fmt]) => h('div', { class: 'gfila' },
          h('div', { class: 'gtit' }, h('b', {}, nombre), h('span', {}, prom(vals) != null ? fmt(prom(vals)) : '')), grafica(vals, color, max)))
      : h('div', { class: 'vacio' }, 'Se guarda un punto por minuto: las gráficas aparecen en unos minutos y llegan hasta 7 días.')),
  ];
}

async function analizar() {
  analizando = true; pintar();
  try { disco = await invoke('analizar_disco'); } catch { disco = []; }
  analizando = false; pintar();
}

function bloqueDisco() {
  const tope = disco?.length ? disco[0].bytes : 1;
  return [
    h('div', { class: 'sec' }, 'Qué ocupa el disco', h('button', { class: 'btn', onclick: analizar }, analizando ? 'Analizando…' : disco ? 'Volver a medir' : 'Analizar')),
    h('div', { class: 'lista' }, !disco
      ? h('div', { class: 'vacio' }, analizando ? 'Midiendo carpetas… puede tardar un minuto.' : 'Mide Descargas, Documentos, cachés, Docker, Papelera… y te dice qué se puede liberar.')
      : disco.map((c) => {
          const b = h('div', { class: 'barra' }, h('b', {})); b.firstChild.style.width = Math.max(2, (c.bytes / tope) * 100) + '%';
          return h('div', { class: 'p', style: 'flex-wrap:wrap' },
            h('div', { class: 'n' }, h('b', {}, c.nombre), c.consejo ? h('span', {}, c.consejo) : null),
            h('div', { class: 'num', style: 'color:var(--tx);font-weight:600' }, gb(c.bytes)),
            h('button', { class: 'btn', onclick: () => invoke('mostrar_carpeta', { ruta: c.ruta }) }, 'Abrir'),
            h('div', { style: 'flex-basis:100%' }, b));
        })),
  ];
}
// Liberar RAM: en qué se va la memoria y qué conviene cerrar. Los consejos los decide Rust (medir.rs → clasificar).
function bloqueLiberar(e) {
  const partes = [
    ['Apps', e.mem_apps, 'var(--m500)'], ['Comprimida', e.mem_comprimida, 'var(--warn)'],
    ['Sistema', e.mem_fija, 'var(--m300)'], ['Caché', e.mem_cache, 'var(--teal)'],
  ];
  const barra = h('div', { class: 'reparto' }, partes.map(([, v, c]) => { const b = h('b', {}); b.style.width = (v / e.mem_total) * 100 + '%'; b.style.background = c; return b; }));
  const libre = Math.max(0, e.mem_total - e.mem_usada - e.mem_cache);
  const resumen = e.presion >= 4
    ? `El Mac se está quedando sin memoria: ya comprimió ${gib(e.mem_comprimida)} y usa ${gib(e.swap_usada)} del disco. Cierra algo de abajo ya.`
    : e.presion >= 2 || e.swap_usada > 1e9
      ? `La memoria está justa: macOS comprimió ${gib(e.mem_comprimida)}${e.swap_usada > 0.5e9 ? ` y pasó ${gib(e.swap_usada)} al disco` : ''}. Cerrar lo de abajo se nota enseguida.`
      : 'Hay memoria de sobra: no hace falta cerrar nada.';
  const fila = (c) => h('div', { class: 'p' },
    h('div', { class: 'n' }, h('b', {}, c.nombre), c.consejo ? h('span', { class: 'consejo' }, c.consejo) : h('span', {}, `${c.procesos} proceso${c.procesos > 1 ? 's' : ''}`)),
    h('div', { class: 'num', style: 'color:var(--tx);font-weight:600' }, mb(c.mem)),
    c.accion ? botonCerrar({ pid: c.pid, nombre: c.proceso }, c.accion === 'reiniciar' ? 'Reiniciar' : 'Cerrar') : null);
  const grupo = (titulo, lista, vacio) => lista.length || vacio ? [h('div', { class: 'sec' }, titulo),
    h('div', { class: 'lista' }, lista.length ? lista.map(fila) : h('div', { class: 'vacio' }, vacio))] : [];
  return [
    h('div', { class: 't' }, h('h3', {}, 'En qué se va la memoria', h('span', { class: 'der' }, `${gib(libre)} libres`)), barra,
      h('div', { class: 'leyenda' }, partes.map(([n, v, c]) => h('span', {}, h('i', { style: `background:${c}` }), `${n} ${gib(v)}`))),
      h('div', { class: 'sub', style: 'margin-top:8px' }, resumen)),
    ...grupo('Ciérralo', e.liberar.filter((c) => c.nivel === 2), 'Nada que cerrar sí o sí.'),
    ...grupo('Depende de ti', e.liberar.filter((c) => c.nivel === 1)),
    ...grupo('Lo demás', e.liberar.filter((c) => c.nivel === 0)),
    h('div', { class: 'sec' }, 'Consejos'),
    h('div', { class: 'lista' }, [
      'La caché y la memoria comprimida son normales: macOS las usa para ir rápido y las suelta solo.',
      'En Brave: Ajustes → Sistema → «Ahorro de memoria» duerme las pestañas que no estás mirando.',
      'Si el Mac lleva días sin reiniciarse y va lento, reiniciarlo libera todo lo acumulado.',
    ].map((t) => h('div', { class: 'vacio' }, t))),
  ];
}

const confirmar = new Map(); // pid → hasta cuándo vale el «¿Seguro?»

async function ip() {
  if (Date.now() - ipCuando < 600_000) return;
  ipCuando = Date.now();
  try { ipPublica = (await (await fetch('https://api.ipify.org')).text()).trim(); } catch { ipPublica = ''; }
}

function botonCerrar(p, texto = 'Cerrar') {
  const armado = (confirmar.get(p.pid) || 0) > Date.now();
  return h('button', { class: armado ? 'btn seguro' : 'btn', title: armado ? 'Clic otra vez para confirmar' : `${texto} este proceso`,
    onclick: async (ev) => {
      ev.stopPropagation();
      if (!armado) { confirmar.set(p.pid, Date.now() + 4000); pintar(); setTimeout(pintar, 4100); return; }
      confirmar.delete(p.pid);
      try { await invoke('cerrar', { pid: p.pid, nombre: p.nombre }); } catch (e) { alertaEn(String(e)); }
      refrescar();
    } }, armado ? '¿Seguro?' : texto);
}

let mensaje = '', mensajeHasta = 0;
const alertaEn = (t) => { mensaje = t; mensajeHasta = Date.now() + 5000; pintar(); };

function pintar() {
  if (!ultimo) return;
  const { estado: e, solicitudes, limpiados } = ultimo;
  const memPct = e.mem_total ? (e.mem_usada / e.mem_total) * 100 : 0;
  const discoUsado = e.disco_total ? ((e.disco_total - e.disco_libre) / e.disco_total) * 100 : 0;
  const caliente = e.top.filter((p) => p.cpu > 80);

  // Semáforo de la cabecera.
  const problemas = [];
  if (e.presion >= 4) problemas.push('memoria al límite'); else if (e.presion >= 2) problemas.push('memoria justa');
  if (discoUsado > 90) problemas.push('disco casi lleno');
  if (caliente.length) problemas.push(`${caliente[0].nombre} al ${pct(caliente[0].cpu)}`);
  if (e.olvidados.length) problemas.push(`${e.olvidados.length} olvidado${e.olvidados.length > 1 ? 's' : ''}`);
  salud.replaceChildren(h('span', { class: `punto ${problemas.length ? (e.presion >= 4 || caliente.length ? 'mal' : 'warn') : ''}` }),
    problemas.length ? problemas[0] : 'Todo en orden');

  const presionTxt = ['', 'normal', 'justa', '', 'al límite'][e.presion] || 'normal';
  const b = e.bateria;

  const tarjetas = h('div', { class: 'rejilla' },
    h('div', { class: 't' }, h('h3', {}, 'Memoria'),
      h('div', { class: 'fila2' }, h('div', {}, h('div', { class: 'grande' }, pct(memPct)), h('div', { class: 'sub' }, `${gib(e.mem_usada)} / ${gib(e.mem_total)}`)),
        dona([{ v: memPct, color: e.presion >= 4 ? 'var(--mal)' : e.presion >= 2 ? 'var(--warn)' : 'var(--m500)' }])),
      h('div', { class: 'chips' }, h('span', { class: `chip ${e.presion >= 4 ? 'mal' : e.presion >= 2 ? 'warn' : 'ok'}` }, `presión ${presionTxt}`),
        e.mem_comprimida > 0.5e9 ? h('span', { class: 'chip' }, `comprimida ${gib(e.mem_comprimida)}`) : null,
        e.swap_usada > 0.5e9 ? h('span', { class: 'chip' }, `swap ${gib(e.swap_usada)}`) : null)),
    h('div', { class: 't' }, h('h3', {}, 'Procesador', e.temp_chip != null ? h('span', { class: `der ${e.temp_chip > 90 ? 'caliente' : ''}` }, `${Math.round(e.temp_chip)} °C`) : null),
      h('div', { class: 'grande' }, pct(e.cpu)),
      h('div', { class: 'nucleos' }, e.nucleos.map((c) => { const i = h('i', {}, h('b', {})); i.firstChild.style.height = Math.max(4, c) + '%'; return i; }))),
    h('div', { class: 't' }, h('h3', {}, 'Red', h('span', { class: 'der' }, e.red.interfaz || 'sin conexión')),
      h('div', { class: 'sub' }, '▼ ', h('b', { style: 'color:var(--tx)' }, velocidad(e.red.bajada)), '   ▲ ', h('b', { style: 'color:var(--tx)' }, velocidad(e.red.subida))),
      h('div', { class: 'chips' }, e.red.ip_local ? h('span', { class: 'chip' }, e.red.ip_local) : null, ipPublica ? h('span', { class: 'chip' }, ipPublica) : null)),
    h('div', { class: 't' }, h('h3', {}, 'Disco'),
      h('div', { class: 'fila2' }, h('div', {}, h('div', { class: 'grande' }, gb(e.disco_libre)), h('div', { class: 'sub' }, `libres de ${gb(e.disco_total)}`)),
        dona([{ v: discoUsado, color: discoUsado > 90 ? 'var(--mal)' : 'var(--azul)' }]))),
    b ? h('div', { class: 't ancha' }, h('h3', {}, 'Batería', h('span', { class: 'der' },
          b.cargando ? 'cargando' : b.enchufado ? 'enchufado' : b.restante ? `quedan ${b.restante} h` : 'en batería')),
        h('div', { class: 'fila2' }, h('div', { class: 'grande' }, b.pct + ' %'),
          h('div', { class: 'chips', style: 'margin:0 0 0 auto' },
            b.salud != null ? h('span', { class: `chip ${b.salud < 80 ? 'warn' : 'ok'}` }, `salud ${b.salud} %`) : null,
            b.ciclos != null ? h('span', { class: 'chip' }, `${b.ciclos} ciclos`) : null,
            e.temp_bateria != null ? h('span', { class: 'chip' }, `${Math.round(e.temp_bateria)} °C`) : null)),
        (() => { const d = h('div', { class: `barra ${b.pct < 20 && !b.enchufado ? 'baja' : ''}` }, h('b', {})); d.firstChild.style.width = b.pct + '%'; return d; })())
      : null,
    e.dispositivos?.length ? h('div', { class: 't ancha' }, h('h3', {}, 'Dispositivos'),
      h('div', { class: 'chips' }, e.dispositivos.map((d) => h('span', { class: `chip ${d.pct < 20 ? 'mal' : ''}` }, `${d.nombre} · ${d.pct} %`)))) : null);

  const pedidos = solicitudes.length ? [
    h('div', { class: 'sec' }, 'Claude pide permiso'),
    h('div', { class: 'lista pedido' }, solicitudes.map((s) => h('div', { class: 'p' },
      h('div', { class: 'n' }, h('b', {}, `Cerrar ${s.nombre} (${s.pid})`), h('span', {}, s.motivo || 'sin motivo')),
      h('button', { class: 'btn p1', onclick: () => decidir(s.id, true) }, 'Aprobar'),
      h('button', { class: 'btn', onclick: () => decidir(s.id, false) }, 'No')))),
  ] : [];

  const olvidados = [
    h('div', { class: 'sec' }, 'Olvidados', e.olvidados.length ? h('button', { class: 'btn p1', onclick: limpiar }, 'Limpiar todo') : null),
    h('div', { class: 'lista' }, e.olvidados.length
      ? e.olvidados.map((p) => h('div', { class: 'p' }, h('div', { class: 'n' }, h('b', {}, p.nombre), h('span', {}, `${p.detalle} · hace ${edad(p.edad_s)}`)),
          h('div', { class: `num ${p.cpu > 80 ? 'caliente' : ''}` }, pct(p.cpu)), botonCerrar(p)))
      : h('div', { class: 'vacio' }, 'Nada olvidado. Se revisa solo cada 15 minutos.')),
  ];

  const top = [
    h('div', { class: 'sec' }, 'Lo que más gasta ahora'),
    h('div', { class: 'lista' }, e.top.slice(0, 5).map((p) => h('div', { class: 'p' },
      h('div', { class: 'n' }, h('b', {}, p.nombre), h('span', {}, `${mb(p.mem)} · abierto hace ${edad(p.edad_s)}`)),
      h('div', { class: `num ${p.cpu > 80 ? 'caliente' : ''}` }, pct(p.cpu)), botonCerrar(p)))),
  ];

  const historial = limpiados.length ? [h('div', { class: 'sec' }, 'Limpiado hace poco'),
    h('div', { class: 'lista' }, limpiados.map((l) => h('div', { class: 'p' }, h('div', { class: 'n' }, h('span', {}, l)))))] : [];

  const aviso = mensaje && Date.now() < mensajeHasta ? h('div', { class: 'lista pedido', style: 'margin-top:10px' }, h('div', { class: 'vacio', style: 'color:var(--mal)' }, mensaje)) : null;

  const recomendados = e.liberar.filter((c) => c.nivel === 2).length;
  const secciones = [
    ['resumen', 'Resumen', 0, () => [tarjetas, ...top]],
    ['ram', 'Liberar RAM', recomendados, () => bloqueLiberar(e)],
    ['olvidados', 'Olvidados', e.olvidados.length, () => [...olvidados, ...historial]],
    ['historial', 'Historial', 0, bloqueHistorial],
    ['disco', 'Disco', 0, bloqueDisco],
  ];
  pestanas.replaceChildren(...secciones.map(([id, txt, n]) => h('button', { class: `pest ${seccion === id ? 'on' : ''}`,
    onclick: () => { if (seccion !== id) { seccion = id; vista.scrollTop = 0; pintar(); } } }, txt, n ? h('i', {}, n) : null)));
  const y = vista.scrollTop;
  vista.replaceChildren(...[aviso, ...pedidos, ...secciones.find(([id]) => id === seccion)[3]()].filter(Boolean),
    h('div', { class: 'pie' }, '⌘⇧M abre este panel · Claude lo lee con «cape-monitor»'));
  vista.scrollTop = y;
}

async function refrescar() {
  try { ultimo = await invoke('estado'); pintar(); } catch (e) { console.error(e); }
}
async function limpiar() { const n = await invoke('limpiar'); alertaEn(n ? `Listo: ${n} cerrado${n > 1 ? 's' : ''}.` : 'No había nada que cerrar.'); refrescar(); }
async function decidir(id, aprobar) { try { await invoke('decidir', { id, aprobar }); } catch (e) { alertaEn(String(e)); } refrescar(); }

window.__TAURI__.event.listen('panel-mostrar', () => {
  document.body.classList.remove('entrando'); void document.body.offsetWidth; document.body.classList.add('entrando');
  ip().then(pintar); refrescar(); cargarHistorial();
});
window.__TAURI__.event.listen('estado', refrescar);
refrescar();
cargarHistorial(true);
