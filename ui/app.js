"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const els = {
  status: $("status"),
  statusText: $("status-text"),
  vista: $("vista"),
  aviso: $("aviso"),
  toast: $("toast"),
  atualizar: $("atualizar"),
  categorias: $("categorias"),
  rodape: $("rodape-texto"),
};

const esc = (value) => {
  const span = document.createElement("span");
  span.textContent = value;
  return span.innerHTML;
};

const store = {
  get(key, fallback) {
    try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
  },
  set(key, value) {
    try { localStorage.setItem(key, value); } catch { /* sem armazenamento: ignora */ }
  },
};

const ICONS = {
  bulb: '<path d="M9 18h6M10 21h4M12 3a6 6 0 0 0-3.5 10.9c.6.5 1 1.2 1 2.1h5c0-.9.4-1.6 1-2.1A6 6 0 0 0 12 3Z"/>',
  tv: '<rect x="3" y="5" width="18" height="12" rx="2"/><path d="M8 21h8M12 17v4"/>',
  clima: '<path d="M12 3v18M4.2 7.5l15.6 9M19.8 7.5l-15.6 9M9.5 4.5 12 7l2.5-2.5M9.5 19.5 12 17l2.5 2.5"/>',
  jogos: '<path d="M7 9h10a4 4 0 0 1 4 4v2a2 2 0 0 1-3.5 1.3L16 15H8l-1.5 1.3A2 2 0 0 1 3 15v-2a4 4 0 0 1 4-4ZM8 11v2M7 12h2M16 12h.01M18 12h.01"/>',
  cenas: '<path d="m12 3 1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9ZM19 16l.7 1.8L21.5 18.5l-1.8.7L19 21l-.7-1.8-1.8-.7 1.8-.7Z"/>',
};
const svg = (name, cls = "cat-ico") =>
  `<svg class="${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name]}</svg>`;

/* ---------- Estado global (cabeçalho, avisos) ---------- */

let toastTimer = null;
function toast(mensagem) {
  els.toast.textContent = mensagem;
  els.toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (els.toast.hidden = true), 4500);
}

function setStatus(estado, texto) {
  els.status.dataset.state = estado;
  els.statusText.textContent = texto;
}

function hora(data) {
  return data.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
}

/* ---------- Módulo: Luzes ----------
   Cada categoria é um módulo { id, label, icon, ready, mount() }. Para um novo
   provider (TV, clima, console), registre um módulo novo em MODULES. */

const luzes = {
  id: "luzes",
  label: "Luzes",
  icon: "bulb",
  ready: true,

  snapshot: null,
  stale: false,
  loadedAt: null,
  loading: false,
  busy: new Set(),
  groupBy: store.get("orbe.groupBy", "comodo"),

  summaryText() {
    const s = this.snapshot;
    if (!s) return ["Procurando suas luzes…", ""];
    if (s.lights_total === 0) return ["Nenhuma luz encontrada", ""];
    const { lights_on: on, lights_total: total } = s;
    if (on === 0) return ["Tudo apagado", `${total} luzes`];
    return [on === 1 ? "1 luz acesa" : `${on} luzes acesas`, `de ${total} luzes`];
  },

  groups() {
    const items = this.snapshot.panels.flatMap((p) => p.items.map((item) => ({ ...item, panel: p })));
    if (this.groupBy === "painel") {
      return this.snapshot.panels.map((p) => ({
        title: p.name,
        note: p.room || "",
        items: p.items.filter((i) => i.kind === "light").map((i) => ({ ...i, sub: i.room || "Sem cômodo" })),
      })).filter((g) => g.items.length > 0);
    }
    const byRoom = new Map();
    for (const item of items) {
      if (item.kind !== "light") continue;
      const room = item.room || "Sem cômodo";
      if (!byRoom.has(room)) byRoom.set(room, []);
      byRoom.get(room).push({ ...item, sub: item.panel.name });
    }
    return [...byRoom.entries()]
      .sort(([a], [b]) => (a === "Sem cômodo") - (b === "Sem cômodo") || a.localeCompare(b, "pt-BR"))
      .map(([title, list]) => ({ title, note: "", items: list }));
  },

  actions() {
    return this.snapshot.panels.flatMap((p) => p.items).filter((i) => i.kind === "action");
  },

  tile(item) {
    const ocupado = this.busy.has(item.id);
    const estado = item.diverged ? "Divergente" : item.on ? "Ligada" : "Desligada";
    const desabilitado = ocupado || this.stale;
    return (
      `<button class="tile${ocupado ? " ocupado" : ""}" type="button" role="switch" ` +
      `aria-checked="${item.on}" aria-label="${esc(item.name)}, ${estado}" data-id="${item.id}" ` +
      `${desabilitado ? "disabled" : ""}>` +
      `<span class="tile-topo">${svg("bulb", "bulb")}<span class="estado">${estado}</span></span>` +
      `<span><span class="nome">${esc(item.name)}</span>` +
      (item.diverged
        ? '<span class="alerta">Paralelos em estados diferentes</span>'
        : `<span class="sub">${esc(item.sub || "")}</span>`) +
      `</span></button>`
    );
  },

  render() {
    const root = els.vista;
    root.classList.toggle("obsoleta", this.stale);
    if (!this.snapshot) {
      root.innerHTML = `<p class="vazio">${esc(this.summaryText()[0])}</p>`;
      return;
    }
    const [titulo, sub] = this.summaryText();
    const { lights_on: on, lights_total: total } = this.snapshot;
    const pct = total ? Math.round((on / total) * 100) : 0;
    const grupos = this.groups();

    let html =
      `<div class="resumo"><div class="anel" style="--p:${pct}" aria-hidden="true"><b>${on}</b></div>` +
      `<div class="resumo-texto"><div class="resumo-titulo">${esc(titulo)}</div><div class="resumo-sub">${esc(sub)}</div></div></div>` +
      `<div class="barra"><span>Agrupar por</span><div class="segmento" role="group" aria-label="Agrupar por">` +
      `<button type="button" data-group="comodo" aria-pressed="${this.groupBy === "comodo"}">Cômodos</button>` +
      `<button type="button" data-group="painel" aria-pressed="${this.groupBy === "painel"}">Painéis</button>` +
      `</div></div>`;

    for (const g of grupos) {
      const acesas = g.items.filter((i) => i.on).length;
      html +=
        `<section class="grupo"><h2>${esc(g.title)}` +
        (g.note ? `<span>· ${esc(g.note)}</span>` : "") +
        `<small>${acesas}/${g.items.length}</small></h2>` +
        `<div class="grade">${g.items.map((i) => this.tile(i)).join("")}</div></section>`;
    }

    const acoes = this.actions();
    if (acoes.length) {
      html +=
        `<section class="grupo"><h2>Rotinas</h2><div class="rotinas">` +
        acoes.map((a) => `<span class="chip">${svg("cenas")}${esc(a.name)}</span>`).join("") +
        `</div><p class="dica">Rotinas serão habilitadas depois de mapearmos o que cada uma aciona.</p></section>`;
    }
    root.innerHTML = html;
  },

  async load() {
    if (this.loading) return;
    this.loading = true;
    els.atualizar.classList.add("girando");
    if (!this.snapshot) setStatus("carregando", "Conectando ao hub…");
    try {
      this.snapshot = await invoke("get_lights");
      this.stale = false;
      this.loadedAt = new Date();
      els.aviso.hidden = true;
      setStatus("ok", `Hub conectado · ${hora(this.loadedAt)}`);
    } catch (erro) {
      this.stale = this.snapshot !== null;
      els.aviso.textContent = String(erro);
      els.aviso.hidden = false;
      setStatus("erro", this.stale ? "Sem resposta · último estado" : "Hub indisponível");
    } finally {
      this.loading = false;
      els.atualizar.classList.remove("girando");
      if (current === this) this.render();
    }
  },

  async toggle(id) {
    const item = this.snapshot?.panels.flatMap((p) => p.items).find((i) => i.id === id);
    if (!item || this.busy.has(id)) return;
    this.busy.add(id);
    this.render();
    try {
      // O estado vem do hub depois do comando, nunca de suposição local.
      this.snapshot = await invoke("set_light", { endpoint: id, on: !item.on });
      this.loadedAt = new Date();
      els.aviso.hidden = true;
      setStatus("ok", `Hub conectado · ${hora(this.loadedAt)}`);
    } catch (erro) {
      toast(`Não foi possível alterar "${item.name}". ${erro}`);
    } finally {
      this.busy.delete(id);
      if (current === this) this.render();
    }
  },

  mount() {
    els.rodape.textContent = "Orbe local · Matter · sem nuvem";
    this.render();
    this.load();
  },

  onClick(evento) {
    const tile = evento.target.closest("button[data-id]");
    if (tile) return this.toggle(Number(tile.dataset.id));
    const group = evento.target.closest("button[data-group]");
    if (group) {
      this.groupBy = group.dataset.group;
      store.set("orbe.groupBy", this.groupBy);
      this.render();
    }
  },
};

const MODULES = [
  luzes,
  { id: "tv", label: "TV", icon: "tv", ready: false },
  { id: "clima", label: "Clima", icon: "clima", ready: false },
  { id: "jogos", label: "Jogos", icon: "jogos", ready: false },
  { id: "cenas", label: "Cenas", icon: "cenas", ready: false },
];

let current = luzes;

function renderCategorias() {
  els.categorias.innerHTML = MODULES.map((m) =>
    m.ready
      ? `<button class="cat" type="button" data-cat="${m.id}" aria-current="${current === m ? "page" : "false"}">${svg(m.icon)}${esc(m.label)}</button>`
      : `<button class="cat" type="button" disabled title="Em breve" aria-label="${esc(m.label)}, em breve">${svg(m.icon)}${esc(m.label)}</button>`
  ).join("");
}

els.categorias.addEventListener("click", (evento) => {
  const botao = evento.target.closest("button[data-cat]");
  const modulo = botao && MODULES.find((m) => m.id === botao.dataset.cat && m.ready);
  if (modulo && modulo !== current) {
    current = modulo;
    renderCategorias();
    current.mount();
  }
});

els.vista.addEventListener("click", (evento) => current.onClick?.(evento));
els.atualizar.addEventListener("click", () => current.load?.());
document.addEventListener("keydown", (evento) => {
  if (evento.key === "Escape") invoke("hide_panel");
});

listen("panel-shown", () => current.load?.());
listen("refresh", () => current.load?.());

// Acompanha mudanças feitas fora do Orbe (interruptor físico, Alexa, Tuya).
setInterval(() => {
  if (!document.hidden && current.busy?.size === 0) current.load?.();
}, 15000);

renderCategorias();
current.mount();
