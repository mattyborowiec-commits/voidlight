'use strict';

/* Strona "Mody": przeglądarka Modrinth, instalacja z zależnościami, pakiet wydajnościowy, aktualizacje.
   Korzysta z pomocników zdefiniowanych w main.js ($, call, esc, state, current, toast, store). */
(() => {
  /* ---------- atrapa backendu (podgląd w zwykłej przeglądarce) ---------- */
  const MOCK_HITS = [
    { project_id: 'mock-sodium', slug: 'sodium', title: 'Sodium', description: 'Nowoczesny silnik renderowania, który znacząco zwiększa liczbę klatek na sekundę i ogranicza przycięcia.', author: 'jellysquid3', downloads: 52300000, icon_url: 'https://cdn.modrinth.com/data/mock/sodium.png' },
    { project_id: 'mock-fabric-api', slug: 'fabric-api', title: 'Fabric API', description: 'Podstawowa biblioteka potrzebna większości modów dla Fabric.', author: 'modmuss50', downloads: 118000000, icon_url: null },
    { project_id: 'mock-lithium', slug: 'lithium', title: 'Lithium', description: 'Optymalizacje logiki gry: szybsze sztuczki świata, AI mobów i fizyka, bez zmiany zasad rozgrywki.', author: 'jellysquid3', downloads: 41800000, icon_url: null },
    { project_id: 'mock-ferrite-core', slug: 'ferrite-core', title: 'FerriteCore', description: 'Zmniejsza zużycie pamięci RAM przez grę.', author: 'malte0811', downloads: 36500000, icon_url: null },
    { project_id: 'mock-modmenu', slug: 'modmenu', title: 'Mod Menu', description: 'Lista zainstalowanych modów dostępna z poziomu menu gry.', author: 'Prospector', downloads: 74200000, icon_url: null },
    { project_id: 'mock-iris', slug: 'iris', title: 'Iris Shaders', description: 'Obsługa shaderów zgodna z Sodium.', author: 'coderbot', downloads: 29100000, icon_url: null },
  ];
  const MOCK_EXTRA = Array.from({ length: 70 }, (_, i) => ({
    project_id: `mock-extra-${i + 1}`, slug: `extra-${i + 1}`, title: `Przykładowy mod ${i + 1}`,
    description: 'Wypełniacz do testowania przewijania i stron wyników w podglądzie.', author: 'autor',
    downloads: 1000000 - i * 9000, icon_url: null,
  }));
  // Klucz zapisu atrapy: mody jak dawniej, pozostałe rodzaje z dopiskiem rodzaju (i świata).
  const mk = (a) => (!a.kind || a.kind === 'mod' ? a.name : `${a.name}:${a.kind}:${a.world || ''}`);
  const MOCK_OTHER = (kind) => Array.from({ length: 40 }, (_, i) => ({
    project_id: `mock-${kind}-${i + 1}`, slug: `${kind}-${i + 1}`,
    title: `${{ shader: 'Shader', resourcepack: 'Paczka zasobów', datapack: 'Paczka danych' }[kind]} ${i + 1}`,
    description: 'Przykładowy wpis do podglądu w przeglądarce.', author: 'autor', downloads: 500000 - i * 7000, icon_url: null,
  }));
  const mockRead = (name) => { try { return JSON.parse(store.get(`mock_mods:${name}`)) || []; } catch { return []; } };
  const mockWrite = (name, v) => store.set(`mock_mods:${name}`, JSON.stringify(v));
  const mockInstall = (name, hit) => {
    const list = mockRead(name);
    if (list.some((m) => m.project_id === hit.project_id)) return false;
    list.push({ project_id: hit.project_id, slug: hit.slug, title: hit.title, version_id: 'v1', version_number: '1.0.0', filename: `${hit.slug}-1.0.0.jar`, explicit: true, icon_url: hit.icon_url });
    mockWrite(name, list);
    return true;
  };

  window.mockMods = async (cmd, args) => {
    await new Promise((r) => setTimeout(r, 150));
    switch (cmd) {
      case 'search_mods': {
        const q = String(args.query || '').toLowerCase();
        const base = !args.kind || args.kind === 'mod' ? MOCK_HITS.concat(MOCK_EXTRA) : MOCK_OTHER(args.kind);
        const all = base.filter((h) => !q || h.title.toLowerCase().includes(q));
        return { hits: all.slice(args.offset, args.offset + 30), total_hits: all.length };
      }
      case 'import_files': {
        const rep = { installed: [], skipped: [], failed: [] };
        const ext = args.kind === 'mod' ? '.jar' : '.zip';
        const list = mockRead(mk(args));
        for (const p of args.paths) {
          const f = p.split(/[\\/]/).pop();
          if (!f.toLowerCase().endsWith(ext)) { rep.failed.push(`${f}: oczekiwano pliku ${ext} dla tego rodzaju zawartości`); continue; }
          if (list.some((m) => m.filename === f)) { rep.skipped.push(f); continue; }
          list.push({ project_id: `local:${f}`, slug: `local:${f}`, title: f.replace(/\.[^.]+$/, ''), version_id: 'local', version_number: 'własny plik', filename: f, explicit: true, icon_url: '' });
          rep.installed.push(f);
        }
        mockWrite(mk(args), list);
        return rep;
      }
      case 'open_content_folder':
        return null;
      case 'list_worlds':
        return ['Nowy świat', 'Survival 1'];
      case 'list_mods':
        return mockRead(mk(args));
      case 'install_mod': {
        const pool = !args.kind || args.kind === 'mod' ? MOCK_HITS.concat(MOCK_EXTRA) : MOCK_OTHER(args.kind);
        const hit = pool.find((h) => h.slug === args.project || h.project_id === args.project);
        if (!hit) return { installed: [], skipped: [], failed: [`${args.project}: nie znaleziono`] };
        return mockInstall(mk(args), hit)
          ? { installed: [hit.title], skipped: [], failed: [] }
          : { installed: [], skipped: [hit.title], failed: [] };
      }
      case 'install_perf_pack': {
        const rep = { installed: [], skipped: [], failed: ['ModernFix: brak wersji dla tej wersji gry (Fabric)'] };
        for (const slug of ['sodium', 'lithium', 'ferrite-core']) {
          const hit = MOCK_HITS.find((h) => h.slug === slug);
          (mockInstall(args.name, hit) ? rep.installed : rep.skipped).push(hit.title);
        }
        return rep;
      }
      case 'remove_mod':
        mockWrite(mk(args), mockRead(mk(args)).filter((m) => m.project_id !== args.projectId));
        return null;
      case 'set_content_enabled': {
        const list = mockRead(mk(args));
        const m = list.find((x) => x.project_id === args.projectId);
        if (m) m.enabled = args.enabled;
        mockWrite(mk(args), list);
        return null;
      }
      case 'check_updates':
        return mockRead(mk(args))
          .filter((m) => ['sodium', 'lithium'].includes(m.slug) && m.version_number !== '9.9.9')
          .map((m) => ({ project_id: m.project_id, version_number: '9.9.9' }));
      case 'update_mods': {
        const list = mockRead(mk(args));
        const done = [];
        for (const m of list) {
          if (!['sodium', 'lithium'].includes(m.slug) || m.version_number === '9.9.9') continue;
          if (args.projectId && m.project_id !== args.projectId) continue;
          m.version_number = '9.9.9';
          done.push(m.title);
        }
        mockWrite(mk(args), list);
        return { updated: done, failed: [] };
      }
      default:
        throw `Nieznana komenda: ${cmd}`;
    }
  };

  /* ---------- stan i pomocnicze ---------- */
  const PAGE_SIZE = 30;
  const KINDS = {
    mod: { label: 'Mody', search: 'Szukaj modów na Modrinth…', empty: 'mod' },
    shader: { label: 'Shadery', search: 'Szukaj shaderów…', empty: 'shaderów' },
    resourcepack: { label: 'Paczki zasobów', search: 'Szukaj paczek zasobów…', empty: 'paczek zasobów' },
    datapack: { label: 'Paczki danych', search: 'Szukaj paczek danych…', empty: 'paczek danych' },
  };
  const M = { updates: {}, updReq: 0, kind: 'mod', world: null, worlds: [], target: null, installed: [], hits: [], total: 0, page: 0, query: '', sort: 'downloads', req: 0, timer: null, loading: false };

  const fmt = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(1).replace('.', ',')} mln` : n >= 1e3 ? `${Math.round(n / 1e3)} tys.` : String(n));
  // Mody tylko dla Fabrica; shadery i paczki działają w każdej instancji.
  const eligible = () => state.instances.filter((i) => M.kind !== 'mod' || i.loader === 'fabric');
  const target = () => eligible().find((i) => i.name === M.target) || null;
  const ctx = (t, extra = {}) => ({ name: t.name, kind: M.kind, world: M.kind === 'datapack' ? M.world : null, ...extra });

  function pickTarget() {
    if (target()) return;
    const cur = current();
    const list = eligible();
    M.target = (cur && list.some((i) => i.name === cur.name) ? cur.name : list[0] && list[0].name) || null;
  }

  async function loadWorlds() {
    M.worlds = [];
    const t = target();
    if (M.kind === 'datapack' && t) {
      try { M.worlds = await call('list_worlds', { name: t.name }); } catch { M.worlds = []; }
    }
    if (!M.worlds.includes(M.world)) M.world = M.worlds[0] || null;
  }

  const isLocal = (m) => String(m.project_id).startsWith('local:');

  const iconHtml = (h) => {
    const letter = esc((h.title || '?').charAt(0).toUpperCase());
    return h.icon_url && /^https:\/\/cdn\.modrinth\.com\//.test(h.icon_url)
      ? `<img class="mod-icon" src="${esc(h.icon_url)}" alt="" loading="lazy" data-letter="${letter}">`
      : `<div class="mod-icon">${letter}</div>`;
  };

  // Gdy ikona się nie wczyta (brak sieci, usunięty plik), pokaż literę zamiast uszkodzonego obrazka.
  $('#page-mods').addEventListener('error', (e) => {
    const img = e.target;
    if (!(img instanceof HTMLImageElement) || !img.classList.contains('mod-icon')) return;
    const box = document.createElement('div');
    box.className = 'mod-icon';
    box.textContent = img.dataset.letter || '?';
    img.replaceWith(box);
  }, true);

  function reportToast(rep) {
    const parts = [];
    if (rep.installed && rep.installed.length) parts.push(`Zainstalowano: ${rep.installed.join(', ')}`);
    if (rep.skipped && rep.skipped.length) parts.push(`Już zainstalowane: ${rep.skipped.join(', ')}`);
    if (rep.failed && rep.failed.length) parts.push(`Nie udało się: ${rep.failed.join('; ')}`);
    toast(parts.join('. ') || 'Nic do zrobienia.', rep.failed && rep.failed.length ? 'err' : '');
  }

  /* ---------- renderowanie ---------- */
  function renderCtx() {
    const sel = $('#modsInstance');
    const list = eligible();
    sel.innerHTML = list.map((i) => `<option value="${esc(i.name)}">${esc(i.name)}</option>`).join('');
    sel.disabled = !list.length;
    const t = target();
    if (t) sel.value = t.name;
    const kindLabel = KINDS[M.kind].label;
    $('#modsCtx').textContent = t
      ? `Instancja „${t.name}” · Minecraft ${t.mc_version} · ${kindLabel}`
      : (M.kind === 'mod' ? 'Mody wymagają instancji z loaderem Fabric.' : 'Utwórz instancję, aby dodać zawartość.');
    $$('.kind').forEach((b) => b.classList.toggle('active', b.dataset.kind === M.kind));
    $('#perfBanner').hidden = M.kind !== 'mod';
    $('#shaderNote').hidden = M.kind !== 'shader';
    $('#worldRow').hidden = M.kind !== 'datapack';
    $('#worldSelect').innerHTML = M.worlds.map((w) => `<option value="${esc(w)}">${esc(w)}</option>`).join('');
    if (M.world) $('#worldSelect').value = M.world;
    $('#modQuery').placeholder = KINDS[M.kind].search;
    const noWorld = M.kind === 'datapack' && !M.world;
    ['perfBtn', 'updateBtn', 'openFolderBtn', 'modQuery', 'modSort'].forEach((id) => { $(`#${id}`).disabled = !t || noWorld; });
  }

  function renderBrowse() {
    const list = $('#modList');
    $('#modPager').hidden = true;
    if (!target()) {
      list.innerHTML = M.kind === 'mod'
        ? '<div class="empty"><strong>Brak instancji Fabric</strong>Utwórz instancję z loaderem Fabric, a potem wróć tutaj po mody.</div>'
        : '<div class="empty"><strong>Brak instancji</strong>Utwórz instancję, a potem wróć tutaj.</div>';
      return;
    }
    if (M.kind === 'datapack' && !M.world && !M.loading) {
      list.innerHTML = '<div class="empty"><strong>Ta instancja nie ma jeszcze żadnego świata</strong>Uruchom grę i utwórz świat, a potem dodasz do niego paczki danych.</div>';
      return;
    }
    if (!M.hits.length) {
      list.innerHTML = `<div class="empty"><strong>${M.loading ? 'Szukam…' : 'Brak wyników'}</strong>${M.loading ? '' : 'Spróbuj innej nazwy albo sprawdź, czy ta pozycja wspiera tę wersję gry.'}</div>`;
      return;
    }
    const have = new Set(M.installed.map((m) => m.project_id));
    list.innerHTML = M.hits.map((h) => `
      <article class="mod">
        ${iconHtml(h)}
        <div class="mod-body">
          <h3>${esc(h.title)} <small>przez ${esc(h.author)}</small></h3>
          <p>${esc(h.description)}</p>
          <div class="mod-meta"><span class="tag">${fmt(h.downloads)} pobrań</span></div>
        </div>
        ${have.has(h.project_id)
          ? '<button class="mini done" disabled>Zainstalowany</button>'
          : `<button class="mini" data-install="${esc(h.slug)}">Zainstaluj</button>`}
      </article>`).join('');
    renderPager();
  }

  /* Numery stron: pierwsza, ostatnia i okolice bieżącej, z "…" w przerwach. */
  function pageList(cur, last) {
    const nums = [...new Set([0, last, cur - 1, cur, cur + 1])]
      .filter((n) => n >= 0 && n <= last)
      .sort((a, b) => a - b);
    const out = [];
    nums.forEach((n, i) => {
      if (i && n - nums[i - 1] > 1) out.push('…');
      out.push(n);
    });
    return out;
  }

  function renderPager() {
    const pager = $('#modPager');
    const last = Math.ceil(M.total / PAGE_SIZE) - 1;
    if (last < 1) { pager.hidden = true; return; }
    const numbers = pageList(M.page, last).map((n) => (n === '…'
      ? '<span class="pager-gap">…</span>'
      : `<button class="pg${n === M.page ? ' active' : ''}" data-page="${n}">${n + 1}</button>`)).join('');
    pager.innerHTML =
      `<button class="pg" data-page="${M.page - 1}" ${M.page === 0 ? 'disabled' : ''}>‹ Poprzednia</button>` +
      numbers +
      `<button class="pg" data-page="${M.page + 1}" ${M.page >= last ? 'disabled' : ''}>Następna ›</button>`;
    pager.hidden = false;
  }

  function installedHtml() {
    const upd = M.updates || {};
    if (!M.installed.length) {
      return M.kind === 'mod'
        ? '<div class="empty"><strong>Brak zainstalowanych modów</strong>Zainstaluj pakiet wydajnościowy albo wyszukaj mody w zakładce Przeglądaj.</div>'
        : `<div class="empty"><strong>Nic jeszcze nie zainstalowano</strong>Wyszukaj ${KINDS[M.kind].empty} w zakładce Przeglądaj.</div>`;
    }
    return M.installed.map((m) => {
      const on = m.enabled !== false;
      const id = esc(m.project_id);
      return `
      <article class="mod${on ? '' : ' off'}">
        ${iconHtml(m)}
        <div class="mod-body">
          <h3>${esc(m.title)} <small>${esc(m.version_number)}</small></h3>
          <div class="mod-meta">
            <span class="tag">${isLocal(m) ? 'własny plik' : m.explicit ? 'zainstalowany' : 'zależność'}</span>
            ${on ? '' : '<span class="tag">wyłączony</span>'}
            ${upd[m.project_id] ? `<span class="tag upd">nowa wersja ${esc(upd[m.project_id])}</span>` : ''}
            <span class="tag">${esc(m.filename)}</span>
          </div>
        </div>
        <div class="row-actions">
          ${upd[m.project_id] ? `<button class="mini" data-update="${id}">Aktualizuj</button>` : ''}
          <label class="switch" title="${on ? 'Wyłącz' : 'Włącz'}"><input type="checkbox" data-toggle="${id}" ${on ? 'checked' : ''} aria-label="Włączony"><span></span></label>
          <button class="mini danger" data-remove="${id}">Usuń</button>
        </div>
      </article>`;
    }).join('');
  }

  // Ta sama lista jest na stronie Odkrywaj (zakładka Zainstalowane) i na stronie instancji.
  function renderInstalled() {
    const upd = M.updates || {};
    const nUpd = M.installed.filter((m) => upd[m.project_id]).length;
    $('#installedCount').textContent = M.installed.length ? `(${M.installed.length})` : '';
    $('#updateBtn').textContent = nUpd ? `Aktualizuj wszystko (${nUpd})` : 'Aktualizuj wszystko';
    $('#installedList').innerHTML = installedHtml();
    $('#instContent').innerHTML = !target() && M.kind === 'mod'
      ? '<div class="empty"><strong>Ta instancja nie ma modów</strong>Mody są obsługiwane tylko w instancjach Fabric.</div>'
      : (M.kind === 'datapack' && !M.world
        ? '<div class="empty"><strong>Ta instancja nie ma jeszcze żadnego świata</strong>Uruchom grę i utwórz świat, a potem dodasz do niego paczki danych.</div>'
        : installedHtml());
  }

  /* ---------- operacje ---------- */
  async function loadInstalled() {
    const t = target();
    if (!t || (M.kind === 'datapack' && !M.world)) { M.installed = []; return; }
    try {
      M.installed = await call('list_mods', ctx(t));
    } catch (e) {
      M.installed = [];
      toast(`Nie udało się wczytać listy modów: ${e}`, 'err');
    }
  }

  async function search(reset) {
    const t = target();
    if (!t || (M.kind === 'datapack' && !M.world)) { M.loading = false; renderBrowse(); return; }
    if (reset) { M.page = 0; M.total = 0; }
    M.hits = [];
    const req = ++M.req;
    M.loading = true;
    renderBrowse();
    try {
      const r = await call('search_mods', { kind: M.kind, query: M.query, mcVersion: t.mc_version, offset: M.page * PAGE_SIZE, sort: M.sort });
      if (req !== M.req) return;
      M.hits = r.hits;
      M.total = r.total_hits;
    } catch (e) {
      if (req === M.req) toast(`Wyszukiwanie nie powiodło się: ${e}`, 'err');
    }
    if (req === M.req) { M.loading = false; renderBrowse(); }
  }

  // Sprawdza w tle, które pozycje mają nowsze wersje (bez blokowania interfejsu).
  async function checkUpdates() {
    const t = target();
    const req = ++M.updReq;
    M.updates = {};
    if (!t || !M.installed.some((m) => !isLocal(m)) || (M.kind === 'datapack' && !M.world)) { renderInstalled(); return; }
    try {
      const list = await call('check_updates', ctx(t));
      if (req !== M.updReq) return;
      M.updates = Object.fromEntries(list.map((u) => [u.project_id, u.version_number]));
    } catch { /* brak sieci: po prostu bez oznaczeń */ }
    renderInstalled();
  }

  async function refreshAll() {
    await loadInstalled();
    renderInstalled();
    renderBrowse();
    checkUpdates();
  }

  async function runAction(btn, busyLabel, fn) {
    const label = btn.textContent;
    btn.disabled = true;
    btn.textContent = busyLabel;
    try {
      await fn();
    } catch (e) {
      toast(String(e), 'err');
    }
    btn.disabled = false;
    btn.textContent = label;
    await refreshAll();
  }

  /* ---------- zdarzenia ---------- */
  $$('.kind').forEach((b) => b.addEventListener('click', () => {
    if (M.kind === b.dataset.kind) return;
    M.kind = b.dataset.kind;
    M.query = ''; $('#modQuery').value = '';
    M.hits = []; M.total = 0; M.page = 0; M.installed = []; M.updates = {};
    M.loading = true;
    renderCtx();
    renderBrowse();
    renderInstalled();
    window.onModsPage();
  }));
  $('#worldSelect').addEventListener('change', (e) => {
    M.world = e.target.value;
    window.onModsPage();
  });
  $('#irisBtn').addEventListener('click', (e) => {
    const fab = state.instances.filter((i) => i.loader === 'fabric');
    const t = target() && target().loader === 'fabric' ? target() : fab[0];
    if (!t) { toast('Iris wymaga instancji Fabric.', 'err'); return; }
    runAction(e.currentTarget, 'Instaluję…', async () => {
      const rep = await call('install_mod', { name: t.name, project: 'iris', kind: 'mod', world: null });
      reportToast(rep);
    });
  });

  $('#modsInstance').addEventListener('change', (e) => {
    M.target = e.target.value;
    window.onModsPage();
  });

  $('#modQuery').addEventListener('input', (e) => {
    clearTimeout(M.timer);
    M.timer = setTimeout(() => { M.query = e.target.value; search(true); }, 350);
  });
  $('#modSort').addEventListener('change', (e) => { M.sort = e.target.value; search(true); });
  $('#modPager').addEventListener('click', (e) => {
    const b = e.target.closest('[data-page]');
    if (!b || b.disabled) return;
    M.page = Number(b.dataset.page);
    search(false).then(() => { $('#tab-browse').scrollTop = 0; });
  });

  $('#modList').addEventListener('click', (e) => {
    const btn = e.target.closest('[data-install]');
    const t = target();
    if (!btn || !t) return;
    runAction(btn, 'Instaluję…', async () => reportToast(await call('install_mod', ctx(t, { project: btn.dataset.install }))));
  });

  $('#perfBtn').addEventListener('click', (e) => {
    const t = target();
    if (!t) return;
    runAction(e.currentTarget, 'Instaluję…', async () => reportToast(await call('install_perf_pack', { name: t.name })));
  });

  $('#updateBtn').addEventListener('click', (e) => {
    const t = target();
    if (!t) return;
    runAction(e.currentTarget, 'Aktualizuję…', async () => {
      const rep = await call('update_mods', ctx(t));
      const parts = [];
      if (rep.updated.length) parts.push(`Zaktualizowano: ${rep.updated.join(', ')}`);
      if (rep.failed.length) parts.push(`Nie udało się: ${rep.failed.join('; ')}`);
      toast(parts.join('. ') || 'Wszystko jest aktualne.', rep.failed.length ? 'err' : '');
    });
  });

  /* ---------- import własnych plików ---------- */
  const EXT = { mod: '.jar', shader: '.zip', resourcepack: '.zip', datapack: '.zip' };

  async function importPaths(paths) {
    const packs = paths.filter((p) => /\.mrpack$/i.test(p));
    if (packs.length) { await window.importMrpack(packs[0]); return; }
    if ($('#page-mods').hidden) { toast('Przejdź na stronę Mody, aby dodać pliki do instancji.', 'err'); return; }
    const t = target();
    if (!t) { toast('Najpierw wybierz instancję.', 'err'); return; }
    if (M.kind === 'datapack' && !M.world) { toast('Wybierz świat, do którego dodać paczkę danych.', 'err'); return; }
    try {
      reportToast(await call('import_files', ctx(t, { paths })));
    } catch (e) {
      toast(String(e), 'err');
    }
    await refreshAll();
  }

  const overlay = $('#dropOverlay');
  const showDrop = (on) => {
    if (on) {
      $('#dropText').textContent = $('#page-mods').hidden
        ? 'Upuść plik .mrpack, aby zaimportować modpack'
        : `Upuść pliki ${EXT[M.kind]} (lub .mrpack), aby dodać do: ${KINDS[M.kind].label}`;
      overlay.hidden = false;
    } else {
      overlay.hidden = true;
    }
  };

  if (IS_TAURI && T.event) {
    T.event.listen('tauri://drag-enter', () => showDrop(true));
    T.event.listen('tauri://drag-leave', () => showDrop(false));
    T.event.listen('tauri://drag-drop', (e) => {
      showDrop(false);
      const paths = (e.payload && e.payload.paths) || [];
      if (paths.length) importPaths(paths);
    });
  } else {
    // Podgląd w zwykłej przeglądarce: zwykłe zdarzenia przeciągania (dostępne są tylko nazwy plików).
    document.addEventListener('dragover', (e) => { e.preventDefault(); showDrop(true); });
    document.addEventListener('dragleave', (e) => { if (!e.relatedTarget) showDrop(false); });
    document.addEventListener('drop', (e) => {
      e.preventDefault();
      showDrop(false);
      const names = [...(e.dataTransfer ? e.dataTransfer.files : [])].map((f) => f.name);
      if (names.length) importPaths(names);
    });
  }
  window.importPaths = importPaths;

  $('#openFolderBtn').addEventListener('click', async () => {
    const t = target();
    if (!t) return;
    try { await call('open_content_folder', ctx(t)); } catch (e) { toast(String(e), 'err'); }
  });

  const onInstalled = (type, fn) => [$('#installedList'), $('#instContent')].forEach((el) => el.addEventListener(type, fn));

  onInstalled('change', async (e) => {
    const box = e.target.closest('[data-toggle]');
    const t = target();
    if (!box || !t) return;
    box.disabled = true;
    try {
      await call('set_content_enabled', ctx(t, { projectId: box.dataset.toggle, enabled: box.checked }));
    } catch (err) {
      toast(String(err), 'err');
    }
    await loadInstalled();
    renderInstalled();
    renderBrowse();
  });

  onInstalled('click', (e) => {
    const btn = e.target.closest('[data-update]');
    const t = target();
    if (!btn || !t) return;
    runAction(btn, 'Aktualizuję…', async () => {
      const rep = await call('update_mods', ctx(t, { projectId: btn.dataset.update }));
      toast(rep.failed.length ? `Nie udało się: ${rep.failed.join('; ')}` : `Zaktualizowano: ${rep.updated.join(', ') || 'nic do zrobienia'}.`, rep.failed.length ? 'err' : '');
    });
  });

  onInstalled('click', (e) => {
    const btn = e.target.closest('[data-remove]');
    const t = target();
    if (!btn || !t) return;
    if (!btn.classList.contains('armed')) {
      // pierwsze kliknięcie tylko prosi o potwierdzenie
      btn.classList.add('armed');
      btn.textContent = 'Na pewno?';
      setTimeout(() => { btn.classList.remove('armed'); btn.textContent = 'Usuń'; }, 3000);
      return;
    }
    runAction(btn, 'Usuwam…', async () => { await call('remove_mod', ctx(t, { projectId: btn.dataset.remove })); });
  });

  $$('.tab').forEach((b) => b.addEventListener('click', () => {
    $$('.tab').forEach((x) => x.classList.toggle('active', x === b));
    $('#tab-browse').hidden = b.dataset.tab !== 'browse';
    $('#tab-installed').hidden = b.dataset.tab !== 'installed';
  }));

  /* ---------- strona instancji: zawartość ---------- */
  function syncInstanceKinds() {
    $$('.ikind').forEach((b) => b.classList.toggle('active', b.dataset.ikind === M.kind));
    $('#instWorldRow').hidden = M.kind !== 'datapack';
    $('#instWorld').innerHTML = M.worlds.map((w) => `<option value="${esc(w)}">${esc(w)}</option>`).join('');
    if (M.world) $('#instWorld').value = M.world;
  }

  window.loadInstanceContent = async (name, keepKind) => {
    if (!keepKind) M.kind = 'mod';
    M.target = name;
    M.installed = []; M.updates = {};
    await loadWorlds();
    syncInstanceKinds();
    await loadInstalled();
    renderInstalled();
    checkUpdates();
  };

  $$('.ikind').forEach((b) => b.addEventListener('click', () => {
    if (M.kind === b.dataset.ikind) return;
    M.kind = b.dataset.ikind;
    // Strona Odkrywaj ma własną listę wyników, więc czyścimy ją, żeby nie pokazać wyników innego rodzaju.
    M.hits = []; M.total = 0; M.page = 0; M.query = ''; $('#modQuery').value = '';
    window.loadInstanceContent(M.target, true);
  }));
  $('#instWorld').addEventListener('change', (e) => {
    M.world = e.target.value;
    window.loadInstanceContent(M.target, true);
  });
  $('#ipAdd').addEventListener('click', () => {
    M.hits = []; M.total = 0; M.page = 0;
    go('mods');
  });
  $('#ipFolder').addEventListener('click', async () => {
    try { await call('open_instance_folder', { name: M.target }); } catch (e) { toast(String(e), 'err'); }
  });

  /* ---------- wejście na stronę ---------- */
  window.onModsPage = async () => {
    if (M.hitsKind !== M.kind) { M.hits = []; M.total = 0; M.page = 0; M.hitsKind = M.kind; }
    pickTarget();
    await loadWorlds();
    renderCtx();
    await loadInstalled();
    renderInstalled();
    checkUpdates();
    if (M.hits.length) renderBrowse(); else await search(true);
  };
  if (!$('#page-mods').hidden) window.onModsPage();
})();
