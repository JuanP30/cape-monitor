// CAPE Monitor — panel. Pide el estado a Rust cada 2 s (Rust ya lo tiene medido) y lo pinta.
// Todo texto que viene de procesos se pinta con textContent.
const { invoke } = window.__TAURI__.core;
const vista = document.getElementById('vista');
const salud = document.getElementById('salud');
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
const confirmar = new Map(); // pid → hasta cuándo vale el «¿Seguro?»

async function ip() {
  if (Date.now() - ipCuando < 600_000) return;
  ipCuando = Date.now();
  try { ipPublica = (await (await fetch('https://api.ipify.org')).text()).trim(); } catch { ipPublica = ''; }
}

function botonCerrar(p) {
  const armado = (confirmar.get(p.pid) || 0) > Date.now();
  return h('button', { class: armado ? 'btn seguro' : 'btn', title: armado ? 'Clic otra vez para cerrar' : 'Cerrar este proceso',
    onclick: async (ev) => {
      ev.stopPropagation();
      if (!armado) { confirmar.set(p.pid, Date.now() + 4000); pintar(); setTimeout(pintar, 4100); return; }
      confirmar.delete(p.pid);
      try { await invoke('cerrar', { pid: p.pid, nombre: p.nombre }); } catch (e) { alertaEn(String(e)); }
      refrescar();
    } }, armado ? '¿Seguro?' : 'Cerrar');
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
        e.swap_usada > 0.5e9 ? h('span', { class: 'chip' }, `swap ${gib(e.swap_usada)}`) : null)),
    h('div', { class: 't' }, h('h3', {}, 'Procesador'), h('div', { class: 'grande' }, pct(e.cpu)),
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
            b.ciclos != null ? h('span', { class: 'chip' }, `${b.ciclos} ciclos`) : null)),
        (() => { const d = h('div', { class: `barra ${b.pct < 20 && !b.enchufado ? 'baja' : ''}` }, h('b', {})); d.firstChild.style.width = b.pct + '%'; return d; })())
      : null);

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

  const apps = [
    h('div', { class: 'sec' }, 'Apps pesadas'),
    h('div', { class: 'lista' },
      h('div', { class: 'p' }, h('div', { class: 'n' }, h('b', {}, 'Claude'),
        h('span', {}, e.claude.activo ? `${e.claude.sesiones} sesiones con proceso abierto · archiva las que ya no uses` : 'cerrado')),
        h('div', { class: 'num' }, e.claude.activo ? mb(e.claude.mem) : '—')),
      h('div', { class: 'p' }, h('div', { class: 'n' }, h('b', {}, 'Docker'), h('span', {}, e.docker.activo ? `CPU ${pct(e.docker.cpu)} · ciérralo si no lo estás usando` : 'cerrado')),
        h('div', { class: 'num' }, e.docker.activo ? mb(e.docker.mem) : '—'))),
  ];

  const historial = limpiados.length ? [h('div', { class: 'sec' }, 'Limpiado hace poco'),
    h('div', { class: 'lista' }, limpiados.map((l) => h('div', { class: 'p' }, h('div', { class: 'n' }, h('span', {}, l)))))] : [];

  const aviso = mensaje && Date.now() < mensajeHasta ? h('div', { class: 'lista pedido', style: 'margin-top:10px' }, h('div', { class: 'vacio', style: 'color:var(--mal)' }, mensaje)) : null;

  const y = vista.scrollTop;
  vista.replaceChildren(...[tarjetas, aviso, ...pedidos, ...olvidados, ...top, ...apps, ...historial].filter(Boolean),
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
  ip().then(pintar); refrescar();
});
window.__TAURI__.event.listen('estado', refrescar);
refrescar();
