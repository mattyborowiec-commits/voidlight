'use strict';

/* ================= środowisko ================= */
const T = window.__TAURI__;
const IS_TAURI = Boolean(T && T.core);

const store = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* brak dostępu do pamięci */ } },
};

const LOADERS = { vanilla: 'Vanilla', fabric: 'Fabric', quilt: 'Quilt', neoforge: 'NeoForge' };
const PROFILES = {
  balanced: {
    label: 'Zbalansowany', short: 'dla większości graczy', badge: 'Zalecany',
    desc: 'Domyślny wybór. Gra działa równo, z modami i bez.',
    good: 'Mało przycięć przy normalnej grze',
    trade: 'Nie jest najlepszy ani w oszczędzaniu pamięci, ani w płynności',
  },
  smooth: {
    label: 'Płynny', short: 'najmniej przycięć, mocny komputer', badge: '',
    desc: 'Dla mocnych komputerów. Gra rzadziej łapie krótkie przycięcia.',
    good: 'Najmniej mikro-przycięć, także przy dużym zasięgu widzenia',
    trade: 'Zajmuje więcej pamięci i procesora',
  },
  light: {
    label: 'Oszczędny', short: 'najmniej pamięci, słabszy komputer', badge: '',
    desc: 'Dla słabszych komputerów albo gdy chcesz zostawić pamięć innym programom.',
    good: 'Najmniejsze zużycie pamięci RAM',
    trade: 'Możliwe drobne przycięcia przy dużym świecie i wielu modach',
  },
};
const TOP_COLOR = { vanilla: '#5fa34a', fabric: '#d9c7a3', quilt: '#a78bfa', neoforge: '#f59e0b' };
const PAGES = ['home', 'instances', 'instance', 'mods', 'settings'];

/* ================= atrapa backendu (podgląd w zwykłej przeglądarce) ================= */
function mockArgs(profile, ram) {
  const xms = profile === 'light' ? Math.min(ram, 2048) : ram;
  const base = ['-XX:+IgnoreUnrecognizedVMOptions', `-Xms${xms}M`, `-Xmx${ram}M`, '-XX:-UsePerfData', '-Dfile.encoding=UTF-8'];
  const gc = {
    balanced: ['-XX:+UseG1GC', '-XX:MaxGCPauseMillis=50', '-XX:+UnlockExperimentalVMOptions', '-XX:G1NewSizePercent=30', '-XX:G1MaxNewSizePercent=40',
      '-XX:G1HeapRegionSize=16M', '-XX:G1ReservePercent=20', '-XX:InitiatingHeapOccupancyPercent=15'],
    smooth: ['-XX:+UseZGC'],
    light: ['-XX:+UseG1GC', '-XX:MaxGCPauseMillis=100'],
  }[profile] || [];
  return [...base, ...gc];
}

let mockCancel = false;
function mockAuth() {
  const name = store.get('mock_account') || null;
  return { signed_in: !!name, name, uuid: name ? '00000000000000000000000000000000' : null, client_id: store.get('mock_client_id') || null };
}

function mockInstances() {
  try {
    const saved = JSON.parse(store.get('mock_instances'));
    if (Array.isArray(saved)) return saved;
  } catch { /* użyj domyślnych */ }
  return [
    { name: 'Survival Fabric', mc_version: '1.21.4', loader: 'fabric', loader_version: null, ram_mb: 6144, jvm_profile: 'balanced' },
    { name: 'Czysty Vanilla', mc_version: '1.21.4', loader: 'vanilla', loader_version: null, ram_mb: 3072, jvm_profile: 'light' },
  ];
}

async function mockInvoke(cmd, args) {
  await new Promise((r) => setTimeout(r, 80));
  switch (cmd) {
    case 'system_info':
      return { total_ram_mb: 32768, recommended_modded_mb: 6144, recommended_vanilla_mb: 3072 };
    case 'list_releases':
      return ['1.21.4', '1.21.3', '1.21.1', '1.20.6', '1.20.4', '1.19.4', '1.18.2', '1.16.5'].map((id) => ({ id, kind: 'release' }));
    case 'get_instances':
      return mockInstances();
    case 'create_instance': {
      const all = mockInstances();
      if (all.some((i) => i.name === args.instance.name)) throw 'Instancja o tej nazwie już istnieje.';
      all.push(args.instance);
      store.set('mock_instances', JSON.stringify(all));
      return args.instance;
    }
    case 'reset_verification':
      return null;
    case 'auth_status':
      return mockAuth();
    case 'auth_set_client_id': {
      if (!/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(args.clientId.trim())) throw 'Client ID powinien wyglądać jak 8-4-4-4-12 znaków szesnastkowych.';
      store.set('mock_client_id', args.clientId.trim());
      return mockAuth();
    }
    case 'auth_login': {
      if (!store.get('mock_client_id')) throw 'Najpierw wpisz Client ID aplikacji Azure.';
      mockCancel = false;
      onAuthCode({ user_code: 'ABCD1234', verification_uri: 'https://www.microsoft.com/link', expires_in: 900 });
      for (let i = 0; i < 12 && !mockCancel; i++) await new Promise((r) => setTimeout(r, 250));
      if (mockCancel) throw 'Logowanie anulowane.';
      store.set('mock_account', 'MockPlayer');
      return mockAuth();
    }
    case 'auth_cancel':
      mockCancel = true;
      return null;
    case 'auth_logout':
      store.set('mock_account', '');
      return mockAuth();
    case 'open_url':
      return null;
    case 'update_instance': {
      const all = mockInstances();
      const i = all.find((x) => x.name === args.name);
      if (!i) throw 'Nie znaleziono instancji.';
      if (args.newName !== args.name && all.some((x) => x.name === args.newName)) throw 'Instancja o tej nazwie już istnieje.';
      i.name = args.newName; i.ram_mb = args.ramMb; i.jvm_profile = args.jvmProfile;
      store.set('mock_instances', JSON.stringify(all));
      return i;
    }
    case 'duplicate_instance': {
      const all = mockInstances();
      const i = all.find((x) => x.name === args.name);
      if (!i) throw 'Nie znaleziono instancji.';
      if (all.some((x) => x.name === args.newName)) throw 'Instancja o tej nazwie już istnieje.';
      const copy = { ...i, name: args.newName };
      all.push(copy);
      store.set('mock_instances', JSON.stringify(all));
      return copy;
    }
    case 'open_instance_folder':
      return null;
    case 'search_modpacks': {
      const packs = [
        ['Create: Pełny pakiet', 'Create z dodatkami, maszynami i kolejami.', 'twórca-1', 1250000],
        ['Create Factory', 'Automatyzacja i fabryki oparte na Create.', 'twórca-2', 480000],
        ['Fabulously Optimized', 'Wydajność i grafika bez zmiany rozgrywki.', 'twórca-3', 3400000],
        ['Przygody RPG', 'Eksploracja, lochy i walka.', 'twórca-4', 210000],
      ];
      const q = String(args.query || '').toLowerCase();
      const hits = packs.filter((p) => !q || p[0].toLowerCase().includes(q)).map((p, i) => ({
        project_id: `pk${i}`, slug: `pack-${i}`, title: p[0], description: p[1], author: p[2], downloads: p[3], icon_url: null }));
      return { hits: hits.slice(args.offset), total_hits: hits.length };
    }
    case 'install_modpack': {
      const all = mockInstances();
      let name = 'Create Pakiet', n = 2;
      while (all.some((x) => x.name === name)) name = `Create Pakiet ${n++}`;
      all.push({ name, mc_version: '1.21.1', loader: 'fabric', loader_version: '0.16.5', ram_mb: 4096, jvm_profile: 'balanced' });
      store.set('mock_instances', JSON.stringify(all));
      for (let i = 0; i <= 4; i++) { onProgress({ stage: 'Modpack', done: i * 25, total: 100 }); await new Promise((r) => setTimeout(r, 30)); }
      return { instance: all[all.length - 1], files: 100, optional: 0, skipped: 0 };
    }
    case 'import_mrpack': {
      if (!/\.mrpack$/i.test(args.path)) throw 'Wybierz plik z rozszerzeniem .mrpack.';
      const all = mockInstances();
      const base = args.path.replace(/\.mrpack$/i, '').split(/[\\/]/).pop();
      let name = base, n = 2;
      while (all.some((x) => x.name === name)) name = `${base} ${n++}`;
      const inst = { name, mc_version: '1.21.1', loader: 'fabric', loader_version: '0.16.5', ram_mb: 4096, jvm_profile: 'balanced' };
      all.push(inst);
      store.set('mock_instances', JSON.stringify(all));
      for (let i = 0; i <= 4; i++) { onProgress({ stage: 'Modpack', done: i * 5, total: 20 }); await new Promise((r) => setTimeout(r, 30)); }
      return { instance: inst, files: 20, optional: 2, skipped: 3 };
    }
    case 'delete_instance': {
      const all = mockInstances().filter((i) => i.name !== args.name);
      store.set('mock_instances', JSON.stringify(all));
      return null;
    }
    case 'jvm_args':
      return mockArgs(args.profile, args.ramMb);
    case 'launch_instance': {
      // Symulacja postępu na potrzeby podglądu w przeglądarce.
      for (const [stage, total] of [['Wersja', 4], ['Biblioteki', 100], ['Zasoby', 100], ['Java', 100]]) {
        for (let i = 0; i <= 4; i++) {
          onProgress({ stage, done: Math.round((total * i) / 4), total });
          await new Promise((r) => setTimeout(r, 250));
        }
      }
      throw 'Podgląd w przeglądarce: prawdziwy start gry działa tylko w aplikacji Tauri.';
    }
    default:
      if (typeof window.mockMods === 'function') return window.mockMods(cmd, args);
      throw `Nieznana komenda: ${cmd}`;
  }
}

const call = (cmd, args = {}) => (IS_TAURI ? T.core.invoke(cmd, args) : mockInvoke(cmd, args));

/* ================= pomocnicze ================= */
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const gb = (mb) => `${+(mb / 1024).toFixed(1)} GB`;

const blockIcon = (loader) => `<svg viewBox="0 0 24 24" aria-hidden="true">
  <polygon points="12,2 22,7.5 12,13 2,7.5" fill="${TOP_COLOR[loader] || '#a78bfa'}"/>
  <polygon points="2,7.5 12,13 12,23 2,17.5" fill="#6b4a2f"/>
  <polygon points="22,7.5 12,13 12,23 22,17.5" fill="#53371f"/></svg>`;

function toast(msg, kind = '') {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = msg;
  $('#toasts').append(el);
  setTimeout(() => el.remove(), Math.min(14000, 3500 + msg.length * 45));
}

const state = {
  instances: [],
  selected: store.get('selected'),
  sys: { total_ram_mb: 16384, recommended_modded_mb: 6144, recommended_vanilla_mb: 3072 },
  releases: [],
  profile: store.get('profile') || 'balanced',
  player: store.get('player') || 'Player',
  playing: 'idle', // 'idle' | 'busy' | 'running'
  account: { signed_in: false, name: null, client_id: null },
  loginUri: null,
};
let ramTouched = false;

const current = () => state.instances.find((i) => i.name === state.selected) || state.instances[0] || null;

/* ================= tło hero (proceduralne, pikselowe) ================= */
function paintHero() {
  const c = $('#heroCanvas');
  const W = 200, H = 76;
  c.width = W; c.height = H;
  const g = c.getContext('2d');
  let seed = 20261031;
  const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
  const px = (x, y, w, h, col, a = 1) => { g.globalAlpha = a; g.fillStyle = col; g.fillRect(Math.round(x), Math.round(y), w, h); g.globalAlpha = 1; };

  for (let y = 0; y < H; y++) {
    const t = y / H;
    px(0, y, W, 1, `rgb(${Math.round(14 + t * 18)},${Math.round(9 + t * 8)},${Math.round(28 + t * 32)})`);
  }
  for (let i = 0; i < 44; i++) px(rnd() * W, rnd() * H * 0.45, 1, 1, '#cbb8ff', 0.15 + rnd() * 0.35);

  // księżyc
  px(148, 5, 16, 16, '#a78bfa', 0.07);
  px(152, 9, 8, 8, '#b9a9dd');
  px(154, 11, 2, 2, '#8d7cb8'); px(157, 13, 2, 2, '#8d7cb8'); px(153, 14, 1, 1, '#d7cdf0');

  const trees = (base, col, minH, maxH, gap, canopyW) => {
    for (let x = -4; x < W + 4; x += gap + Math.floor(rnd() * gap)) {
      const th = minH + Math.floor(rnd() * (maxH - minH));
      const tw = 2 + (rnd() > 0.6 ? 1 : 0);
      px(x, base - th, tw, th, col);
      const cw = canopyW + Math.floor(rnd() * 6);
      const ch = 6 + Math.floor(rnd() * 5);
      const cx = x + tw / 2 - cw / 2;
      const cy = base - th - ch + 2;
      for (let yy = 0; yy < ch; yy++) {
        for (let xx = 0; xx < cw; xx++) {
          const corner = (xx === 0 || xx === cw - 1) && (yy === 0 || yy === ch - 1);
          if (!corner && rnd() > 0.12) px(cx + xx, cy + yy, 1, 1, col);
        }
      }
    }
  };

  trees(54, '#1d1233', 14, 26, 6, 9);

  // fioletowe filary światła
  const pillar = (x, y, h) => {
    px(x - 2, y - 2, 7, h + 4, '#a855f7', 0.10);
    px(x, y, 3, h, '#c084fc');
    for (let i = 0; i < h; i += 3) px(x + (i % 2), y + i, 1, 1, '#7e22ce');
  };
  pillar(112, 34, 26); pillar(131, 46, 14); pillar(97, 44, 12);

  trees(63, '#140c24', 16, 30, 8, 11);
  trees(73, '#0b0714', 10, 20, 12, 13);
  px(0, 71, W, 5, '#090611');
}

/* ================= renderowanie ================= */
function renderHome() {
  const inst = current();
  $('#selName').textContent = inst ? inst.name : 'Brak instancji';
  $('#selMeta').textContent = inst ? `${inst.mc_version} · ${LOADERS[inst.loader] || inst.loader}` : 'Utwórz pierwszą instancję';
  $('#selIcon').innerHTML = blockIcon(inst ? inst.loader : 'quilt');
  if (state.playing === 'idle') $('#playBtn').textContent = inst ? 'GRAJ' : 'NOWA';
  const acc = state.account;
  $('#whoName').textContent = acc.signed_in ? acc.name : state.player;
  $('#whoMode').textContent = acc.signed_in ? 'Konto Microsoft' : 'Tryb offline';
  $('#whoDot').classList.toggle('on', !!acc.signed_in);

  $('#chips').innerHTML = inst
    ? [`${gb(inst.ram_mb)} RAM`, `Profil: ${PROFILES[inst.jvm_profile]?.label || inst.jvm_profile}`, LOADERS[inst.loader] || inst.loader]
      .map((t) => `<span class="chip">${esc(t)}</span>`).join('')
    : '';

  $('#statInst').textContent = state.instances.length;
  $('#statRam').textContent = gb(state.sys.total_ram_mb);
  $('#statRamNote').textContent = `zalecane ${gb(state.sys.recommended_modded_mb)}`;
  $('#statProfile').textContent = PROFILES[state.profile].label;
}

function renderSelector() {
  const cur = current();
  const items = state.instances.map((i) => `
    <li role="presentation"><button class="opt" role="option" aria-selected="${cur && i.name === cur.name}" data-name="${esc(i.name)}">
      <span class="sel-icon">${blockIcon(i.loader)}</span>
      <span class="sel-text"><strong>${esc(i.name)}</strong><small>${esc(i.mc_version)} · ${LOADERS[i.loader] || esc(i.loader)}</small></span>
    </button></li>`).join('');
  $('#selectorMenu').innerHTML = `${items}<li role="presentation"><button class="opt add" data-add="1">+ Nowa instancja</button></li>`;
}

function instCard(i) {
  return `
    <article class="inst" data-name="${esc(i.name)}" title="Kliknij dwa razy, aby zobaczyć szczegóły">
      <div class="inst-icon">${blockIcon(i.loader)}</div>
      <div class="inst-info">
        <h3>${esc(i.name)}</h3>
        <small>${LOADERS[i.loader] || esc(i.loader)} ${esc(i.mc_version)}</small>
        <div class="tags">
          <span class="tag">${gb(i.ram_mb)} RAM</span>
          <span class="tag">${PROFILES[i.jvm_profile]?.label || esc(i.jvm_profile)}</span>
        </div>
      </div>
      <button class="inst-play" data-play="${esc(i.name)}" aria-label="Graj w ${esc(i.name)}" title="Graj">
        <svg viewBox="0 0 24 24"><path d="M8 5.5v13l11-6.5z"/></svg>
      </button>
    </article>`;
}

function renderInstances() {
  const empty = '<div class="empty"><strong>Nie masz jeszcze żadnej instancji</strong>Utwórz pierwszą, wybierając wersję gry, loader i profil wydajności.</div>';
  const grid = $('#instGrid');
  const home = $('#homeInst');
  if (!state.instances.length) {
    grid.innerHTML = empty;
    home.innerHTML = empty;
    return;
  }
  grid.innerHTML = state.instances.map(instCard).join('')
    + '<button class="inst new" data-new="1"><span class="plus">+</span>Nowa instancja</button>';
  home.innerHTML = state.instances.slice(0, 4).map(instCard).join('');
}

async function renderSettings() {
  const s = state.sys;
  $('#playerName').value = state.player;
  $('#accSignedIn').hidden = !state.account.signed_in;
  $('#accSignedOut').hidden = state.account.signed_in;
  $('#accName').textContent = state.account.name || '';
  if (state.account.client_id && document.activeElement !== $('#clientId')) $('#clientId').value = state.account.client_id;
  if (!state.account.client_id) $('#msSetup').open = true;
  $('#sysTotal').textContent = gb(s.total_ram_mb);
  $('#sysModded').textContent = gb(s.recommended_modded_mb);
  $('#sysVanilla').textContent = gb(s.recommended_vanilla_mb);
  $('#profileChoices').innerHTML = Object.entries(PROFILES).map(([k, p]) => `
    <label class="choice">
      <input type="radio" name="profile" value="${k}" ${k === state.profile ? 'checked' : ''}>
      <div class="choice-body">
        <div class="choice-head"><strong>${p.label}</strong>${p.badge ? `<span class="badge">${p.badge}</span>` : ''}</div>
        <p>${p.desc}</p>
        <ul class="pc">
          <li class="pro">${p.good}</li>
          <li class="con">${p.trade}</li>
        </ul>
      </div>
    </label>`).join('');
  try {
    const args = await call('jvm_args', { profile: state.profile, ramMb: s.recommended_modded_mb });
    $('#argsPreview').textContent = args.join('\n');
  } catch (e) {
    $('#argsPreview').textContent = String(e);
  }
}

function renderAll() {
  renderHome();
  renderSelector();
  renderInstances();
  renderSettings();
}

/* ================= nawigacja ================= */
function go(page) {
  if (!PAGES.includes(page)) page = 'home';
  $$('.page').forEach((p) => { p.hidden = p.id !== `page-${page}`; });
  $$('.nav-item').forEach((b) => b.classList.toggle('active', b.dataset.page === (page === 'instance' ? 'instances' : page)));
  closeMenu();
  if (page === 'mods' && typeof window.onModsPage === 'function') window.onModsPage();
}

/* ================= wybór instancji ================= */
function closeMenu() {
  $('#selector').classList.remove('open');
  $('#selectorMenu').hidden = true;
  $('#selectorBtn').setAttribute('aria-expanded', 'false');
}
function toggleMenu() {
  const open = !$('#selector').classList.contains('open');
  $('#selector').classList.toggle('open', open);
  $('#selectorMenu').hidden = !open;
  $('#selectorBtn').setAttribute('aria-expanded', String(open));
}
function select(name) {
  state.selected = name;
  store.set('selected', name);
  closeMenu();
  renderHome();
  renderSelector();
}

/* ================= granie ================= */
function setPlay(mode, label = '', pct = 0) {
  state.playing = mode;
  const b = $('#playBtn');
  b.disabled = mode !== 'idle';
  b.classList.toggle('busy', mode === 'busy');
  b.classList.toggle('running', mode === 'running');
  b.style.setProperty('--p', `${pct}%`);
  if (mode === 'idle') renderHome();
  else b.textContent = label;
}

function onProgress({ stage, done, total }) {
  if (state.importing) {
    const pct = total ? Math.round((done / total) * 100) : 0;
    $('#mrpackBtn').textContent = `Import: ${stage} ${pct}%`;
    return;
  }
  const pct = total ? Math.round((done / total) * 100) : 0;
  setPlay('busy', `${String(stage).toUpperCase()} ${pct}%`, pct);
}

async function runImport(task) {
  if (state.importing) return;
  state.importing = true;
  const btn = $('#mrpackBtn');
  btn.disabled = true;
  btn.textContent = 'Import: start…';
  toast('Importuję modpack – to może potrwać kilka minut.');
  try {
    const r = await task();
    toast(`Zaimportowano „${r.instance.name}” (${r.files} plików${r.skipped ? `, pominięto ${r.skipped} serwerowych` : ''}). Pierwszy start pobierze grę.`);
    await reloadInstances();
    go('instances');
  } catch (e) {
    toast(String(e), 'err');
  } finally {
    state.importing = false;
    btn.disabled = false;
    btn.textContent = 'Importuj modpack';
  }
}

const importMrpack = (path) => runImport(() => call('import_mrpack', { path }));
window.importMrpack = importMrpack;

/* ================= modpacki z Modrinth ================= */
const PK = { q: '', sort: 'downloads', offset: 0, total: 0, loading: false };
const fmtDl = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(1).replace('.', ',')} mln` : n >= 1e3 ? `${Math.round(n / 1e3)} tys.` : String(n));

function packCard(h) {
  const icon = h.icon_url ? `<img class="mod-icon" src="${esc(h.icon_url)}" alt="" loading="lazy" />` : `<div class="mod-icon">${esc(h.title.slice(0, 1).toUpperCase())}</div>`;
  return `<article class="mod">${icon}<div class="mod-body">
    <h3>${esc(h.title)} <small>przez ${esc(h.author)}</small></h3>
    <p>${esc(h.description)}</p>
    <div class="mod-meta"><span class="tag">${fmtDl(h.downloads)} pobrań</span></div></div>
    <button class="mini" data-pack="${esc(h.slug)}">Zainstaluj</button></article>`;
}

async function searchPacks(append) {
  if (PK.loading) return;
  PK.loading = true;
  const list = $('#packList');
  if (!append) { PK.offset = 0; list.innerHTML = '<div class="empty"><strong>Szukam…</strong></div>'; }
  try {
    const r = await call('search_modpacks', { query: PK.q, offset: PK.offset, sort: PK.sort });
    PK.total = r.total_hits;
    PK.offset += r.hits.length;
    const html = r.hits.map(packCard).join('');
    if (append) list.insertAdjacentHTML('beforeend', html);
    else list.innerHTML = html || '<div class="empty"><strong>Brak wyników</strong>Spróbuj innej nazwy.</div>';
  } catch (e) {
    list.innerHTML = `<div class="empty"><strong>Nie udało się wyszukać</strong>${esc(String(e))}</div>`;
  }
  $('#packMore').hidden = PK.offset >= PK.total;
  PK.loading = false;
}

function wirePacks() {
  const dlg = $('#packsDialog');
  $('#packsBtn').addEventListener('click', () => { dlg.showModal(); if (!$('#packList').children.length) searchPacks(false); });
  $('#packsClose').addEventListener('click', () => dlg.close());
  let timer;
  $('#packQuery').addEventListener('input', (e) => {
    clearTimeout(timer);
    timer = setTimeout(() => { PK.q = e.target.value; $$('.chip').forEach((c) => c.classList.remove('on')); searchPacks(false); }, 350);
  });
  $('#packSort').addEventListener('change', (e) => { PK.sort = e.target.value; searchPacks(false); });
  $('#packMore').addEventListener('click', () => searchPacks(true));
  $$('.chip').forEach((c) => c.addEventListener('click', () => {
    $$('.chip').forEach((x) => x.classList.toggle('on', x === c));
    PK.q = c.dataset.q;
    $('#packQuery').value = PK.q;
    searchPacks(false);
  }));
  $('#packList').addEventListener('click', (e) => {
    const b = e.target.closest('[data-pack]');
    if (!b || state.importing) return;
    dlg.close();
    runImport(() => call('install_modpack', { projectId: b.dataset.pack }));
  });
}


async function play(name) {
  if (!name) return openNew();
  if (state.playing !== 'idle') return;
  setPlay('busy', 'PRZYGOTOWANIE', 0);
  try {
    await call('launch_instance', { name, player: state.player });
  } catch (e) {
    setPlay('idle');
    toast(String(e), 'err');
  }
}

/* ================= nowa instancja ================= */
async function ensureReleases() {
  if (state.releases.length) return;
  const sel = $('#fVersion');
  sel.innerHTML = '<option value="">Ładowanie…</option>';
  $('#fVersionErr').hidden = true;
  try {
    state.releases = (await call('list_releases')).slice(0, 60);
    store.set('releases_cache', JSON.stringify(state.releases));
  } catch (e) {
    toast(`Nie udało się pobrać listy wersji: ${e}`, 'err');
    const box = $('#fVersionErr');
    box.textContent = String(e);
    box.hidden = false;
    try { state.releases = JSON.parse(store.get('releases_cache') || '[]'); } catch { state.releases = []; }
  }
  sel.innerHTML = state.releases.length
    ? state.releases.map((v) => `<option value="${esc(v.id)}">${esc(v.id)}</option>`).join('')
    : '<option value="">Brak danych</option>';
}

function syncRamRange() {
  const total = state.sys.total_ram_mb;
  const max = Math.max(4096, Math.floor(total / 2 / 512) * 512);
  const r = $('#fRam');
  r.min = 2048;
  r.max = max;
  if (!ramTouched) {
    r.value = $('#fLoader').value === 'vanilla' ? state.sys.recommended_vanilla_mb : state.sys.recommended_modded_mb;
  }
  $('#fRamOut').textContent = gb(Number(r.value));
}

async function openNew() {
  const dlg = $('#newInstance');
  if (dlg.open) return;
  ramTouched = false;
  $('#fName').value = '';
  $('#fProfile').innerHTML = Object.entries(PROFILES)
    .map(([k, p]) => `<option value="${k}">${p.label} · ${p.short}</option>`).join('');
  $('#fProfile').value = state.profile;
  syncRamRange();
  dlg.showModal();
  $('#fName').focus();
  await ensureReleases();
}

async function submitNew(e) {
  e.preventDefault();
  const instance = {
    name: $('#fName').value.trim(),
    mc_version: $('#fVersion').value,
    loader: $('#fLoader').value,
    loader_version: null,
    ram_mb: Number($('#fRam').value),
    jvm_profile: $('#fProfile').value,
  };
  try {
    const created = await call('create_instance', { instance });
    state.instances.push(created);
    state.selected = created.name;
    store.set('selected', created.name);
    $('#newInstance').close();
    renderAll();
    toast(`Utworzono instancję „${created.name}”`);
  } catch (err) {
    toast(String(err), 'err');
  }
}

/* ================= start ================= */
function wire() {
  $$('.nav-item').forEach((b) => b.addEventListener('click', () => go(b.dataset.page)));
  $$('[data-goto]').forEach((b) => b.addEventListener('click', () => go(b.dataset.goto)));

  $('#selectorBtn').addEventListener('click', (e) => { e.stopPropagation(); toggleMenu(); });
  $('#selectorMenu').addEventListener('click', (e) => {
    const opt = e.target.closest('.opt');
    if (!opt) return;
    if (opt.dataset.add) { closeMenu(); openNew(); } else select(opt.dataset.name);
  });
  document.addEventListener('click', (e) => { if (!e.target.closest('#selector')) closeMenu(); });
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeMenu(); });

  $('#playBtn').addEventListener('click', () => { const c = current(); play(c && c.name); });
  const wireInst = (el, { allowNew }) => {
    el.addEventListener('click', (e) => {
      const b = e.target.closest('[data-play]');
      if (b) { play(b.dataset.play); return; }
      if (allowNew && e.target.closest('[data-new]')) { openNew(); return; }
      const card = e.target.closest('.inst[data-name]');
      if (card) select(card.dataset.name);
    });
    el.addEventListener('dblclick', (e) => {
      if (e.target.closest('[data-play]')) return;
      const card = e.target.closest('.inst[data-name]');
      if (card) openInstancePage(card.dataset.name);
    });
  };
  wireInst($('#instGrid'), { allowNew: true });
  wireInst($('#homeInst'), { allowNew: false });

  $('#newBtn').addEventListener('click', openNew);
  wirePacks();
  $('#mrpackBtn').addEventListener('click', () => toast('Przeciągnij plik .mrpack na okno launchera, aby go zaimportować.'));
  $('#cancelNew').addEventListener('click', () => $('#newInstance').close());
  $('#newForm').addEventListener('submit', submitNew);
  $('#fLoader').addEventListener('change', syncRamRange);
  $('#fRam').addEventListener('input', () => { ramTouched = true; $('#fRamOut').textContent = gb(Number($('#fRam').value)); });

  $('#profileChoices').addEventListener('change', (e) => {
    if (e.target.name !== 'profile') return;
    state.profile = e.target.value;
    store.set('profile', state.profile);
    renderHome();
    renderSettings();
  });

  $('#playerName').addEventListener('change', (e) => {
    const v = e.target.value.trim();
    if (!/^[A-Za-z0-9_]{3,16}$/.test(v)) {
      toast('Nick: 3-16 znaków, litery, cyfry lub podkreślnik.', 'err');
      e.target.value = state.player;
      return;
    }
    state.player = v;
    store.set('player', v);
    renderHome();
  });

  if (IS_TAURI && T.event) {
    T.event.listen('launch-progress', (e) => onProgress(e.payload));
    T.event.listen('auth-code', (e) => onAuthCode(e.payload));
    T.event.listen('auth-warning', (e) => toast(`Logowanie Microsoft nie powiodło się, gra startuje offline. ${e.payload}`, 'err'));
    T.event.listen('game-started', (e) => {
      setPlay('running', 'W GRZE', 100);
      const ms = e.payload && e.payload.prep_ms;
      if (typeof ms === 'number') toast(`Przygotowanie przed startem: ${ms < 1000 ? ms + ' ms' : (ms / 1000).toFixed(1) + ' s'}.`);
    });
    T.event.listen('game-exit', (e) => {
      setPlay('idle');
      const code = e.payload && e.payload.code;
      if (code !== 0) {
        toast(`Gra zakończyła działanie z kodem ${code === null || code === undefined ? '?' : code}. Szczegóły: plik launcher.log w folderze instancji.`, 'err');
      }
    });
  }

  if (IS_TAURI && T.window) {
    const w = T.window.getCurrentWindow();
    $('#winMin').addEventListener('click', () => w.minimize());
    $('#winMax').addEventListener('click', () => w.toggleMaximize());
    $('#winClose').addEventListener('click', () => w.close());
  }
}

/* Usuwa instancję (wywoływane z okna szczegółów) i odświeża widoki. */
window.afterInstanceDeleted = async (name) => {
  state.instances = await call('get_instances');
  if (state.selected === name) { state.selected = null; store.set('selected', ''); }
  renderAll();
  if (typeof window.onModsPage === 'function' && !$('#page-mods').hidden) window.onModsPage();
};

/* ================= strona instancji ================= */
const validName = (n) => n.length >= 1 && n.length <= 40 && n === n.trim() && /^[\p{L}\p{N} _-]+$/u.test(n);

function ipInst() { return state.instances.find((i) => i.name === state.viewInstance) || null; }

function renderInstancePage() {
  const i = ipInst();
  if (!i) return;
  $('#ipIcon').innerHTML = blockIcon(i.loader);
  $('#ipName').textContent = i.name;
  $('#ipMeta').textContent = `${LOADERS[i.loader] || i.loader} ${i.mc_version}`;
  $('#ipTags').innerHTML = `<span class="tag">${gb(i.ram_mb)} RAM</span><span class="tag">${PROFILES[i.jvm_profile]?.label || esc(i.jvm_profile)}</span>`;
  $('#ipRename').value = i.name;
  $('#ipDupName').value = `${i.name} kopia`.slice(0, 40);
  $('#ipProfile').innerHTML = Object.entries(PROFILES).map(([k, p]) => `<option value="${k}">${p.label} · ${p.short}</option>`).join('');
  $('#ipProfile').value = i.jvm_profile;
  const r = $('#ipRam');
  const max = Math.max(4096, Math.floor(state.sys.total_ram_mb / 2 / 512) * 512);
  r.min = Math.min(2048, i.ram_mb);
  r.max = Math.max(max, i.ram_mb);
  r.value = i.ram_mb;
  $('#ipRamOut').textContent = gb(i.ram_mb);
  resetDelete();
}

let delTimer = null;
function resetDelete() {
  clearTimeout(delTimer);
  const b = $('#ipDelete');
  b.dataset.armed = '';
  b.textContent = 'Usuń instancję';
}

function openInstancePage(name) {
  state.viewInstance = name;
  setItab('content');
  go('instance');
  renderInstancePage();
  if (typeof window.loadInstanceContent === 'function') window.loadInstanceContent(name);
}

function setItab(tab) {
  $$('.itab').forEach((b) => b.classList.toggle('active', b.dataset.itab === tab));
  $('#itab-content').hidden = tab !== 'content';
  $('#itab-settings').hidden = tab !== 'settings';
}

async function reloadInstances() {
  state.instances = await call('get_instances');
  renderAll();
}

function wireInstancePage() {
  $('#instBack').addEventListener('click', () => go('instances'));
  $$('.itab').forEach((b) => b.addEventListener('click', () => setItab(b.dataset.itab)));
  $('#ipPlay').addEventListener('click', () => play(state.viewInstance));
  $('#ipRam').addEventListener('input', (e) => { $('#ipRamOut').textContent = gb(Number(e.target.value)); });

  $('#ipForm').addEventListener('submit', async (e) => {
    e.preventDefault();
    const i = ipInst();
    if (!i) return;
    const newName = $('#ipRename').value.trim();
    if (!validName(newName)) { toast('Nazwa: litery, cyfry, spacja, myślnik i podkreślnik (1-40 znaków).', 'err'); return; }
    try {
      const saved = await call('update_instance', {
        name: i.name, newName, ramMb: Number($('#ipRam').value), jvmProfile: $('#ipProfile').value,
      });
      if (state.selected === i.name) { state.selected = saved.name; store.set('selected', saved.name); }
      state.viewInstance = saved.name;
      await reloadInstances();
      renderInstancePage();
      if (typeof window.loadInstanceContent === 'function') window.loadInstanceContent(saved.name, true);
      toast('Zapisano zmiany.');
    } catch (err) { toast(String(err), 'err'); }
  });

  $('#ipDup').addEventListener('click', async (e) => {
    const i = ipInst();
    const newName = $('#ipDupName').value.trim();
    if (!i) return;
    if (!validName(newName)) { toast('Nazwa kopii: litery, cyfry, spacja, myślnik i podkreślnik (1-40 znaków).', 'err'); return; }
    const btn = e.currentTarget;
    btn.disabled = true;
    btn.textContent = 'Kopiuję…';
    try {
      await call('duplicate_instance', { name: i.name, newName, withWorlds: $('#ipDupWorlds').checked });
      await reloadInstances();
      toast(`Utworzono kopię „${newName}”.`);
      openInstancePage(newName);
    } catch (err) { toast(String(err), 'err'); }
    btn.disabled = false;
    btn.textContent = 'Utwórz kopię';
  });

  $('#ipDelete').addEventListener('click', async (e) => {
    const b = e.currentTarget;
    const i = ipInst();
    if (!i) return;
    if (!b.dataset.armed) {
      b.dataset.armed = '1';
      b.textContent = 'Kliknij ponownie, aby usunąć';
      delTimer = setTimeout(resetDelete, 4000);
      return;
    }
    b.disabled = true;
    try {
      await call('delete_instance', { name: i.name });
      await window.afterInstanceDeleted(i.name);
      go('instances');
      toast(`Usunięto instancję „${i.name}”.`);
    } catch (err) { toast(String(err), 'err'); resetDelete(); }
    b.disabled = false;
  });
}

/* ================= logowanie Microsoft ================= */
function onAuthCode(c) {
  state.loginUri = c.verification_uri;
  $('#devCode').textContent = c.user_code;
  $('#devBox').hidden = false;
}

function refreshAccount(acc) {
  state.account = acc;
  renderHome();
  renderSettings();
}

function wireAuth() {
  $('#loginBtn').addEventListener('click', async (e) => {
    const btn = e.currentTarget;
    btn.disabled = true;
    try {
      refreshAccount(await call('auth_login'));
      toast(`Zalogowano jako ${state.account.name}.`);
    } catch (err) {
      toast(String(err), 'err');
    }
    $('#devBox').hidden = true;
    btn.disabled = false;
  });
  $('#devOpen').addEventListener('click', () => {
    if (state.loginUri) call('open_url', { url: state.loginUri }).catch((err) => toast(String(err), 'err'));
  });
  $('#devCancel').addEventListener('click', () => call('auth_cancel'));
  $('#logoutBtn').addEventListener('click', async () => {
    try { refreshAccount(await call('auth_logout')); toast('Wylogowano. Gra wróci do trybu offline.'); } catch (err) { toast(String(err), 'err'); }
  });
  $('#saveClientId').addEventListener('click', async () => {
    try {
      refreshAccount(await call('auth_set_client_id', { clientId: $('#clientId').value }));
      toast('Client ID zapisany.');
    } catch (err) { toast(String(err), 'err'); }
  });
  $('#msSetup').addEventListener('click', (e) => {
    const b = e.target.closest('[data-open]');
    if (b) { e.preventDefault(); call('open_url', { url: b.dataset.open }).catch((err) => toast(String(err), 'err')); }
  });
}

async function init() {
  paintHero();
  wire();
  wireAuth();
  wireInstancePage();
  go((location.hash || '#home').slice(1));
  try { state.sys = await call('system_info'); } catch (e) { toast(`Nie udało się odczytać danych systemu: ${e}`, 'err'); }
  try { state.account = await call('auth_status'); } catch { /* zostaje tryb offline */ }
  try { state.instances = await call('get_instances'); } catch (e) { toast(`Nie udało się wczytać instancji: ${e}`, 'err'); }
  renderAll();
  // Jeśli użytkownik już jest na stronie Mody, odśwież ją, gdy lista instancji się wczytała.
  if (!$('#page-mods').hidden && typeof window.onModsPage === 'function') window.onModsPage();
}

init();
