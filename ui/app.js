/* GameMagNet desktop UI */

const ICONS = {
  search: '<circle cx="11" cy="11" r="6.4"/><path d="M15.8 15.8 20 20"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  folder: '<path d="M3.8 7.5h5.7l1.8 2H20v9H4z"/>',
  refresh: '<path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  moon: '<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>',
  more: '<circle cx="6" cy="12" r="1.45"/><circle cx="12" cy="12" r="1.45"/><circle cx="18" cy="12" r="1.45"/>',
  close: '<path d="M6 6l12 12M18 6 6 18"/>',
  chevron: '<path d="m8 10 4 4 4-4"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.07 0l1.42-1.42a5 5 0 1 0-7.07-7.07L10 5.93"/><path d="M14 11a5 5 0 0 0-7.07 0L5.5 12.4a5 5 0 1 0 7.07 7.07L14 18.07"/>',
  copy: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
  download: '<path d="M12 4v10m-4-4 4 4 4-4M5 19h14"/>',
  pulse: '<path d="M3 12h4l2-5 4 10 2-5h6"/>',
  grid: '<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><rect x="14" y="14" width="6" height="6" rx="1"/>',
  magnet: '<path d="M6 15 2 11l6.75-6.77a7.79 7.79 0 0 1 11 11L13 22l-4-4 6.39-6.36a2.14 2.14 0 0 0-3-3L6 15"/><path d="m5 8 4 4"/><path d="m12 15 4 4"/>',
  external: '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
  check: '<path d="m5 12.5 4.2 4 9.8-10"/>',
  panel: '<rect x="4" y="5.5" width="16" height="13" rx="1.5"/><path d="M10 5.5v13"/>',
};

function icon(name, cls = '') {
  const body = ICONS[name] || '';
  if (name === 'more') {
    return `<svg class="${cls}" viewBox="0 0 24 24" width="18" height="18" fill="currentColor" stroke="none">${body}</svg>`;
  }
  return `<svg class="${cls}" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;
}

const $ = (id) => document.getElementById(id);
const invoke = (...args) => window.__TAURI__.core.invoke(...args);
const listen = (...args) => window.__TAURI__.event.listen(...args);

const html = (value) =>
  String(value == null ? '' : value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

let S = null;
let rows = [];
let detail = { has: false, cards: [], sources: [], raw: '', title: '', meta: '', hint: '' };
let swarmTexts = {};
let expandedCards = new Set();
let rawOpen = false;
let rawLoading = false;
let sidebarOpen = true;
let sidebarCollapsed = false;
let detailsOpen = true;
let activeDetailKey = '';
let toastTimer = null;

const emptyDetail = { has: false, cards: [], sources: [], raw: '', title: '', meta: '', hint: '' };

/* --------------------------------------------------------------- icons */
function paintIcons(root = document) {
  root.querySelectorAll('[data-icon]').forEach((node) => {
    node.innerHTML = icon(node.dataset.icon);
  });
}

/* ---------------------------------------------------------------- boot */
async function boot() {
  paintIcons();
  bindStatic();
  applyTheme(localStorage.getItem('gm-theme') || 'dark');

  if (!window.__TAURI__) {
    $('pageSub').textContent = '未检测到 Tauri 运行时';
    return;
  }

  await listen('state', (event) => applyState(event.payload));
  await listen('results', (event) => {
    rows = event.payload || [];
    renderRows();
  });
  await listen('detail', (event) => {
    detail = event.payload || emptyDetail;
    rawOpen = false;
    rawLoading = false;
    renderDetail();
  });
  await listen('detail-raw', (event) => {
    const payload = event.payload || {};
    if (!S || payload.index !== S.selectedResultIndex) return;
    rawLoading = false;
    if (!payload.raw) {
      rawOpen = false;
      showToast('原始 JSON 读取失败');
      renderDetail(true);
      return;
    }
    detail.raw = payload.raw;
    rawOpen = true;
    renderDetail(true);
  });
  await listen('swarm', (event) => {
    swarmTexts = event.payload || {};
    renderDetail(true);
  });
  await listen('toast', (event) => showToast(event.payload));
  await listen('message', (event) => showMessage(event.payload));
  await listen('update-dialog', () => showUpdateDialog());

  S = await invoke('get_snapshot');
  rows = await invoke('get_results');
  detail = await invoke('get_detail');
  swarmTexts = await invoke('get_swarm_texts');
  renderAll();
}

/* ------------------------------------------------------------- theme */
function applyTheme(theme) {
  const light = theme === 'light';
  document.documentElement.classList.toggle('light', light);
  document.documentElement.classList.toggle('dark', !light);
  $('themeToggle').innerHTML = icon(light ? 'sun' : 'moon');
  $('themeToggle').title = light ? '切换为深色主题' : '切换为浅色主题';
  localStorage.setItem('gm-theme', theme);
  if (window.__TAURI__ && window.__TAURI__.core) {
    invoke('set_window_theme', { light }).catch(() => {});
  }
}

/* -------------------------------------------------------------- state */
function applyState(next) {
  const previousQuery = S ? S.query : '';
  S = next;
  renderHeader();
  renderSidebar();
  renderFilters();
  renderStatus();
  updateSelection();
  updateSearchEcho(previousQuery);
  renderDetailHeader();
}

function updateSearchEcho(previousQuery) {
  const input = $('searchInput');
  if (!S) return;
  if (document.activeElement !== input && input.value !== S.query) {
    input.value = S.query;
  }
  if (previousQuery !== S.query && document.activeElement === input && input.value !== S.query) {
    input.value = S.query;
  }
  $('clearSearch').classList.toggle('hidden', !input.value);
}

function renderAll() {
  renderHeader();
  renderSidebar();
  renderFilters();
  renderStatus();
  renderRows();
  renderDetail();
}

function renderHeader() {
  if (!S) return;
  $('pageTitle').textContent = S.pageTitle;
  $('pageSub').textContent = S.pageSub;
  const meta = [S.scopeText, S.countText, S.elapsedText].filter((part) => part).join('  ·  ');
  $('resultMeta').textContent = meta;
}

function renderStatus() {
  if (!S) return;
  $('statusText').textContent = S.status;
  $('statusVersion').textContent = `${S.version} · 本地目录`;
  $('expandDetailsBtn').classList.toggle('hidden', detailsOpen);
  $('detailPane').classList.toggle('closed', !detailsOpen);
}

function renderFilters() {
  if (!S) return;
  $('fuzzyBtn').classList.toggle('checked', S.fuzzy);
  $('thresholdValue').textContent = Number(S.threshold).toFixed(1);
  $('sortBtn').textContent = S.sortDesc ? '最新在前 ↓' : '最早在前 ↑';
  $('sortChev').classList.toggle('asc', !S.sortDesc);
  const input = $('searchInput');
  if (document.activeElement !== input && input.value !== S.query) input.value = S.query;
  $('clearSearch').classList.toggle('hidden', !input.value);
}

function formatCount(value) {
  return String(value == null ? 0 : value).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

function renderSidebar() {
  if (!S) return;
  const list = $('sourceList');
  list.innerHTML = S.sources
    .map((source) => {
      const badge = source.name === ''
        ? icon('grid')
        : html(String(source.display).charAt(0).toUpperCase() || '·');
      return `
        <div class="source-row${source.active ? ' active' : ''}" data-name="${html(source.name)}">
          <span class="source-badge">${badge}</span>
          <span class="source-name" title="${html(source.display)}">${html(source.display)}</span>
          ${source.local ? '<span class="source-local">本地</span>' : ''}
          <span class="source-count">${formatCount(source.count)}</span>
        </div>`;
    })
    .join('');
  $('sourceCount').textContent = Math.max(0, S.sources.length - 1);
  $('sidebar').classList.toggle('collapsed', sidebarCollapsed);
}

function renderRows() {
  const wrap = $('rows');
  if (!rows.length) {
    wrap.innerHTML = '';
    updateEmpty();
    return;
  }
  const frag = document.createDocumentFragment();
  rows.forEach((row, index) => {
    const div = document.createElement('div');
    div.className = 'result-row' + (index % 2 === 1 ? ' alt' : '');
    div.dataset.index = String(index);
    div.innerHTML = `
      <div class="result-title" title="${html(row.title)}">${html(row.title)}</div>
      <div class="cell-source"><span class="source-chip" title="${html(row.sources)}">${html(row.sources)}</span></div>
      <div class="cell-size">${html(row.size)}</div>
      <div class="cell-date">${html(row.date)}</div>
      <div class="cell-links">${icon('link')}<span>${row.linkCount}</span></div>`;
    frag.appendChild(div);
  });
  wrap.replaceChildren(frag);
  wrap.scrollTop = 0;
  updateSelection();
  updateEmpty();
}

function updateSelection() {
  const selected = S ? S.selectedResultIndex : -1;
  document.querySelectorAll('.result-row').forEach((node) => {
    node.classList.toggle('selected', Number(node.dataset.index) === selected);
  });
  const current = document.querySelector('.result-row.selected');
  if (current && rows.length) {
    const wrap = $('rows');
    const top = current.offsetTop;
    const bottom = top + current.offsetHeight;
    if (top < wrap.scrollTop) wrap.scrollTop = top;
    if (bottom > wrap.scrollTop + wrap.clientHeight) {
      wrap.scrollTop = bottom - wrap.clientHeight;
    }
  }
}

function updateEmpty() {
  const empty = $('emptyState');
  const show = rows.length === 0;
  empty.classList.toggle('hidden', !show);
  if (!show || !S) return;
  const loading = S.busy || S.latestLoading;
  $('emptyTitle').textContent = loading
    ? '正在加载游戏…'
    : S.query.length > 0
      ? '没有找到结果'
      : '暂无游戏';
  $('emptySub').textContent = loading
    ? '首次读取全部来源可能需要几秒，请稍候。'
    : S.query.length > 0
      ? '试试更短的关键词，或切换来源'
      : '添加或刷新源后，这里会显示最新的游戏。';
}

/* ------------------------------------------------------------- detail */
function renderDetailHeader() {
  if (!S) return;
  $('detailHint').textContent = detail.has ? detail.hint : '';
  $('detailBody').classList.toggle('visible', detail.has);
  $('detailEmpty').classList.toggle('hidden', detail.has);
}

function renderDetail(keepScroll = false) {
  if (!detail) detail = emptyDetail;
  const key = detail.has ? `${detail.title}|${detail.meta}` : '';
  if (key !== activeDetailKey) {
    expandedCards.clear();
    activeDetailKey = key;
    rawOpen = false;
  }
  renderDetailHeader();
  const body = $('detailBody');
  const previousScroll = keepScroll ? body.scrollTop : 0;
  if (!detail.has) {
    body.innerHTML = '';
    $('detailEmpty').classList.remove('hidden');
    return;
  }
  $('detailEmpty').classList.add('hidden');

  const chords = detail.sources.map((source) => `<span class="chip">${html(source)}</span>`).join('');
  const cards = detail.cards.map(renderCard).join('');
  const rawSection = rawOpen
    ? `<pre class="raw-block">${html(detail.raw || '')}</pre>`
    : rawLoading
      ? '<pre class="raw-block">正在读取原始 JSON…</pre>'
      : '';
  const rawLabel = rawLoading
    ? '读取原始 JSON…'
    : rawOpen
      ? '收起原始 JSON'
      : '查看原始 JSON';

  body.innerHTML = `
    <div class="detail-title">${html(detail.title)}</div>
    <div class="chips">${chords}</div>
    <div class="detail-meta">${html(detail.meta)}</div>
    <div class="detail-actions">
      <button class="btn soft" data-action="copy-all">${icon('copy')}<span>复制全部</span></button>
    </div>
    ${cards}
    <button class="raw-toggle${rawOpen ? ' open' : ''}" data-action="toggle-raw">
      <span>${rawLabel}</span>
      <span class="ico">${icon('chevron')}</span>
    </button>
    ${rawSection}`;
  body.scrollTop = previousScroll;
}

function renderCard(card) {
  const expanded = expandedCards.has(card.key);
  const groups = card.links
    .map((group) => {
      const all = group.visible || [];
      const shown = expanded ? all : all.slice(0, 6);
      const rest = all.length - shown.length;
      const links = shown.map(renderLink).join('');
      const more = rest > 0
        ? `<button class="btn ghost small" data-action="expand-card" data-key="${html(card.key)}">显示其余 ${rest} 条</button>`
        : '';
      return `
        <div class="link-group">
          <div class="link-group-head"><span>${html(group.label)} · ${group.total}</span><span class="count">${group.total}</span></div>
          ${links}
          ${more}
        </div>`;
    })
    .join('');
  return `
    <div class="card" data-key="${html(card.key)}">
      <div class="card-head">
        <span class="card-source">${html(card.source)}</span>
        <span class="card-meta">${html(card.date)}  ·  ${html(card.size)}</span>
      </div>
      ${card.description ? `<div class="card-desc">${html(card.description)}</div>` : ''}
      ${groups}
    </div>`;
}

function renderLink(link) {
  const heat = swarmTexts[link.uri] || '';
  const actions = link.isMagnet
    ? `<div class="link-actions">
         <button class="btn primary" data-action="bt-menu" data-uri="${html(link.uri)}">${icon('download')}<span>BT 下载</span></button>
         ${heat ? `<span class="heat-pill"><span class="heat-dot"></span><span>${html(heat)}</span></span>` : ''}
         <button class="btn ghost" data-action="swarm" data-uri="${html(link.uri)}">${icon('pulse')}<span>查热度</span></button>
       </div>`
    : '';
  return `
    <div class="link-row">
      <div class="link-line">
        <span class="link-icon">${icon(link.isMagnet ? 'magnet' : 'link')}</span>
        <span class="link-uri" title="${html(link.uri)}">${html(link.uri)}</span>
        <button class="link-btn" title="复制链接" data-action="copy-link" data-uri="${html(link.uri)}">${icon('copy')}</button>
        ${link.isMagnet ? '' : `<button class="link-btn" title="在浏览器打开" data-action="open-link" data-uri="${html(link.uri)}">${icon('external')}</button>`}
      </div>
      ${actions}
    </div>`;
}

/* ------------------------------------------------------------- events */
function bindStatic() {
  $('sidebarToggle').innerHTML = icon('panel');
  $('sidebarToggle').title = '隐藏资料库 (Ctrl+B)';
  $('moreButton').innerHTML = icon('more');
  $('sidebarChev').innerHTML = icon('chevron');
  $('addSourceBtn').innerHTML = icon('plus');
  $('addSourceBtn2').querySelector('.ico').innerHTML = icon('plus');
  $('closeSidebarBtn').innerHTML = icon('close');
  $('importBtn').innerHTML = icon('folder');
  $('refreshBtn').innerHTML = icon('refresh');
  $('clearSearch').innerHTML = icon('close');
  $('closeDetailBtn').innerHTML = icon('close');
  $('searchIcon').innerHTML = icon('search');
  $('emptyState').querySelector('.empty-icon').innerHTML = icon('search');
  $('detailEmpty').querySelector('.empty-icon').innerHTML = icon('grid');
  $('fuzzyBtn').querySelector('.ico').innerHTML = icon('check');
  $('sortChev').innerHTML = icon('chevron');
  $('expandDetailsBtn').querySelector('.ico').innerHTML = icon('grid');

  $('themeToggle').addEventListener('click', () => {
    const light = document.documentElement.classList.contains('light');
    applyTheme(light ? 'dark' : 'light');
  });

  $('sidebarToggle').addEventListener('click', toggleSidebar);
  $('closeSidebarBtn').addEventListener('click', () => setSidebar(false));
  $('sidebarCollapse').addEventListener('click', () => {
    sidebarCollapsed = !sidebarCollapsed;
    renderSidebar();
  });

  $('searchInput').addEventListener('input', (event) => {
    $('clearSearch').classList.toggle('hidden', !event.target.value);
    invoke('set_query', { text: event.target.value });
  });
  $('clearSearch').addEventListener('click', () => {
    $('searchInput').value = '';
    $('clearSearch').classList.add('hidden');
    invoke('set_query', { text: '' });
    $('searchInput').focus();
  });

  $('fuzzyBtn').addEventListener('click', () => invoke('set_fuzzy', { enabled: !(S && S.fuzzy) }));
  $('thresholdMinus').addEventListener('click', () => {
    const value = Math.max(0.1, Math.round(((S ? S.threshold : 0.3) - 0.1) * 10) / 10);
    invoke('set_threshold', { value });
  });
  $('thresholdPlus').addEventListener('click', () => {
    const value = Math.min(1.0, Math.round(((S ? S.threshold : 0.3) + 0.1) * 10) / 10);
    invoke('set_threshold', { value });
  });
  $('sortBtn').addEventListener('click', () => invoke('toggle_sort'));
  $('dateHeader').addEventListener('click', () => invoke('toggle_sort'));

  $('expandDetailsBtn').addEventListener('click', () => setDetails(true));
  $('closeDetailBtn').addEventListener('click', () => setDetails(false));

  $('sourceList').addEventListener('click', (event) => {
    const row = event.target.closest('.source-row');
    if (!row) return;
    invoke('select_source', { name: row.dataset.name });
  });
  $('sourceList').addEventListener('contextmenu', (event) => {
    const row = event.target.closest('.source-row');
    if (!row) return;
    event.preventDefault();
    invoke('select_source', { name: row.dataset.name });
    showSourceMenu(event.clientX, event.clientY, row.dataset.name);
  });

  $('rows').addEventListener('click', (event) => {
    const row = event.target.closest('.result-row');
    if (!row) return;
    invoke('select_result', { index: Number(row.dataset.index) });
  });
  $('rows').addEventListener('dblclick', (event) => {
    const row = event.target.closest('.result-row');
    if (!row) return;
    if (!detailsOpen) setDetails(true);
  });

  $('addSourceBtn').addEventListener('click', showAddDialog);
  $('addSourceBtn2').addEventListener('click', showAddDialog);
  $('importBtn').addEventListener('click', () => invoke('pick_import_files'));
  $('refreshBtn').addEventListener('click', () => invoke('refresh_all'));

  $('moreButton').addEventListener('click', (event) => {
    const rect = event.currentTarget.getBoundingClientRect();
    showMoreMenu(rect.right - 236, rect.bottom + 6);
  });

  $('detailBody').addEventListener('click', onDetailClick);

  document.addEventListener('keydown', onKeyDown);
  window.addEventListener('resize', closeMenus);
}

function onDetailClick(event) {
  const button = event.target.closest('[data-action]');
  if (!button) return;
  const action = button.dataset.action;
  if (action === 'copy-all') invoke('copy_current_links');
  else if (action === 'toggle-raw') {
    if (rawLoading) return;
    const scroll = $('detailBody').scrollTop;
    if (rawOpen) {
      rawOpen = false;
    } else if (detail.raw) {
      rawOpen = true;
    } else {
      rawLoading = true;
      invoke('request_detail_raw').catch((error) => {
        rawLoading = false;
        showToast(`原始 JSON 读取失败: ${error}`);
      });
    }
    renderDetail(true);
    $('detailBody').scrollTop = scroll;
  } else if (action === 'expand-card') {
    const scroll = $('detailBody').scrollTop;
    expandedCards.add(button.dataset.key);
    renderDetail(true);
    $('detailBody').scrollTop = scroll;
  } else if (action === 'copy-link') invoke('copy_text', { text: button.dataset.uri });
  else if (action === 'open-link') invoke('open_external_url', { url: button.dataset.uri });
  else if (action === 'swarm') invoke('query_swarm', { uri: button.dataset.uri });
  else if (action === 'bt-menu') {
    const rect = button.getBoundingClientRect();
    showBtMenu(rect.left, rect.bottom + 4, button.dataset.uri);
  }
}

function toggleSidebar() {
  setSidebar(!sidebarOpen);
}

function setSidebar(open) {
  sidebarOpen = open;
  $('sidebar').classList.toggle('hidden', !open);
  $('sidebarToggle').title = open ? '隐藏资料库 (Ctrl+B)' : '显示资料库 (Ctrl+B)';
}

function setDetails(open) {
  detailsOpen = open;
  $('detailPane').classList.toggle('closed', !open);
  $('expandDetailsBtn').classList.toggle('hidden', open);
}

function onKeyDown(event) {
  const ctrl = event.ctrlKey || event.metaKey;
  if (event.key === 'Escape') {
    closeMenus();
    closeModal('dismiss');
    return;
  }
  if (ctrl && event.key.toLowerCase() === 'k') {
    event.preventDefault();
    $('searchInput').focus();
    $('searchInput').select();
  } else if (ctrl && event.key.toLowerCase() === 'f') {
    event.preventDefault();
    $('searchInput').focus();
    $('searchInput').select();
  } else if (ctrl && event.key.toLowerCase() === 'u') {
    event.preventDefault();
    showAddDialog();
  } else if (ctrl && event.key.toLowerCase() === 'i') {
    event.preventDefault();
    invoke('pick_import_files');
  } else if (ctrl && event.key.toLowerCase() === 'b') {
    event.preventDefault();
    toggleSidebar();
  } else if (ctrl && event.key.toLowerCase() === 'd') {
    event.preventDefault();
    setDetails(!detailsOpen);
  } else if (event.key === 'F5') {
    event.preventDefault();
    invoke('refresh_all');
  }
}

/* -------------------------------------------------------------- menus */
function closeMenus() {
  $('menuLayer').innerHTML = '';
}

function placeMenu(menu, x, y) {
  const layer = $('menuLayer');
  layer.appendChild(menu);
  const rect = menu.getBoundingClientRect();
  const left = Math.min(Math.max(6, x), window.innerWidth - rect.width - 6);
  const top = Math.min(Math.max(6, y), window.innerHeight - rect.height - 6);
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
}

function buildMenu(items) {
  const menu = document.createElement('div');
  menu.className = 'menu';
  items.forEach((item) => {
    if (item.separator) {
      const sep = document.createElement('div');
      sep.className = 'menu-sep';
      menu.appendChild(sep);
      return;
    }
    const btn = document.createElement('button');
    btn.className = 'menu-item' + (item.disabled ? ' disabled' : '') + (item.header ? ' header' : '');
    btn.textContent = item.label;
    if (!item.disabled && !item.header) {
      btn.addEventListener('click', () => {
        closeMenus();
        item.onClick();
      });
    }
    menu.appendChild(btn);
  });
  return menu;
}

function showMenu(x, y, items) {
  closeMenus();
  placeMenu(buildMenu(items), x, y);
}

document.addEventListener(
  'click',
  (event) => {
    if (!event.target.closest('.menu')) closeMenus();
  },
  true
);

function showMoreMenu(x, y) {
  const hasSource = Boolean(S && S.activeSource);
  showMenu(x, y, [
    { label: '源', header: true },
    { label: '更新选中源…', disabled: !hasSource, onClick: showUpdateSourceDialog },
    { label: '删除选中源', disabled: !hasSource, onClick: showDeleteDialog },
    { label: '打开源文件夹', onClick: () => invoke('open_sources_folder') },
    { separator: true },
    { label: '聚焦搜索框', onClick: () => { $('searchInput').focus(); $('searchInput').select(); } },
    { label: (sidebarOpen ? '隐藏' : '显示') + '资料库', onClick: toggleSidebar },
    { separator: true },
    { label: '常见 BT 客户端…', onClick: showBtClientsDialog },
    { label: '检查更新…', onClick: () => invoke('check_updates') },
    { label: '关于', onClick: showAboutDialog },
  ]);
}

function showSourceMenu(x, y, name) {
  showMenu(x, y, [
    { label: '更新此源…', disabled: !name, onClick: showUpdateSourceDialog },
    {
      label: '复制源地址',
      disabled: !(S && S.activeSourceUrl),
      onClick: () => invoke('copy_text', { text: S.activeSourceUrl }),
    },
    { label: '删除源', disabled: !name, onClick: showDeleteDialog },
    { separator: true },
    { label: '打开源文件夹', onClick: () => invoke('open_sources_folder') },
    { label: '从本地导入 JSON…', onClick: () => invoke('pick_import_files') },
  ]);
}

function showBtMenu(x, y, uri) {
  const targets = (S && S.btTargets) || [];
  const items = targets.length
    ? targets.map((target) => ({
        label: target.name,
        onClick: () => invoke('send_to_bt', { uri, key: target.key }),
      }))
    : [{ label: '未检测到 BT 客户端', disabled: true }];
  items.push({ separator: true });
  items.push({ label: '常见 BT 客户端…', onClick: showBtClientsDialog });
  showMenu(x, y, items);
}

/* ------------------------------------------------------------- modals */
let modalDismiss = null;

function closeModal(reason = 'action') {
  const layer = $('modalLayer');
  if (reason === 'dismiss' && modalDismiss) {
    try {
      modalDismiss();
    } catch (error) {
      /* ignore */
    }
  }
  modalDismiss = null;
  layer.classList.remove('open');
  layer.innerHTML = '';
}

function openModal({ title, sub, body, bodyClass = 'modal-body', fields = [], checkboxes = [], actions = [], width, iconName, iconClass, onDismiss }) {
  closeMenus();
  modalDismiss = onDismiss || null;
  const layer = $('modalLayer');
  layer.innerHTML = '';
  layer.classList.add('open');

  const backdrop = document.createElement('div');
  backdrop.className = 'modal-backdrop';
  backdrop.addEventListener('click', () => closeModal('dismiss'));

  const modal = document.createElement('div');
  modal.className = 'modal' + (width ? ` ${width}` : '');

  const heading = document.createElement('div');
  if (iconName) {
    const row = document.createElement('div');
    row.className = 'modal-row';
    const ico = document.createElement('span');
    ico.className = `modal-icon ${iconClass || ''}`;
    ico.innerHTML = icon(iconName);
    const text = document.createElement('div');
    text.className = 'modal-title';
    text.textContent = title;
    row.append(ico, text);
    heading.appendChild(row);
  } else {
    heading.className = 'modal-title';
    heading.textContent = title;
    if (sub) {
      const subEl = document.createElement('div');
      subEl.className = 'modal-sub';
      subEl.textContent = sub;
      subEl.style.marginTop = '4px';
      heading.appendChild(subEl);
    }
  }
  modal.appendChild(heading);

  const inputs = {};
  fields.forEach((field) => {
    if (field.label) {
      const label = document.createElement('div');
      label.className = 'modal-field-label';
      label.textContent = field.label;
      modal.appendChild(label);
    }
    const input = document.createElement(field.type === 'textarea' ? 'textarea' : 'input');
    input.className = 'modal-input' + (field.type === 'textarea' ? ' modal-textarea' : '');
    if (field.type === 'textarea') {
      input.rows = field.rows || 6;
      input.spellcheck = false;
    } else {
      input.type = 'text';
    }
    input.placeholder = field.placeholder || '';
    input.value = field.value || '';
    modal.appendChild(input);
    inputs[field.id] = input;
  });

  const checks = {};
  checkboxes.forEach((box) => {
    const label = document.createElement('label');
    label.className = 'modal-check';
    const input = document.createElement('input');
    input.type = 'checkbox';
    input.checked = Boolean(box.checked);
    const text = document.createElement('span');
    text.textContent = box.label;
    label.append(input, text);
    modal.appendChild(label);
    checks[box.id] = input;
  });

  if (body) {
    const bodyEl = document.createElement('div');
    bodyEl.className = bodyClass;
    bodyEl.innerHTML = body;
    modal.appendChild(bodyEl);
  }

  const actionsRow = document.createElement('div');
  actionsRow.className = 'modal-actions';
  actions.forEach((action) => {
    const btn = document.createElement('button');
    btn.className = `btn ${action.kind || 'soft'}`;
    btn.textContent = action.label;
    btn.disabled = Boolean(action.disabled);
    btn.addEventListener('click', () => {
      const context = { ...inputs, ...checks };
      const keepOpen = action.onClick ? action.onClick(context) === false : false;
      if (!keepOpen) closeModal('action');
    });
    actionsRow.appendChild(btn);
  });
  modal.appendChild(actionsRow);

  layer.append(backdrop, modal);
  const first = modal.querySelector('input');
  if (first) setTimeout(() => first.focus(), 10);
  return { inputs };
}

function showAddDialog() {
  openModal({
    title: '添加源',
    sub: '源地址（一行一个，失败行会自动跳过）',
    fields: [
      {
        id: 'urls',
        type: 'textarea',
        rows: 7,
        placeholder: '每行一个源地址，例如 https://example.com/catalog.json',
      },
    ],
    checkboxes: [{ id: 'overwrite', label: '覆盖同名源', checked: false }],
    actions: [
      { label: '取消', kind: 'soft' },
      {
        label: '开始添加',
        kind: 'primary',
        onClick: (ctx) => {
          const urls = ctx.urls.value
            .split(String.fromCharCode(10))
            .map((line) => line.trim())
            .filter(Boolean);
          if (!urls.length) {
            showToast('请输入至少一个源地址');
            return false;
          }
          invoke('add_sources', { urls, overwrite: ctx.overwrite.checked });
        },
      },
    ],
  });
}

function showUpdateSourceDialog() {
  if (!S || !S.activeSource) {
    showMessage({ title: '提示', text: '请先在左侧选择一个源。', error: false, link: '' });
    return;
  }
  openModal({
    title: `更新源：${S.activeSourceDisplay}`,
    sub: '源地址',
    fields: [{ id: 'url', value: S.activeSourceUrl, placeholder: 'https://example.com/catalog.json' }],
    actions: [
      { label: '取消', kind: 'soft' },
      {
        label: '开始更新',
        kind: 'primary',
        onClick: (inputs) => invoke('request_update_source', { url: inputs.url.value }),
      },
    ],
  });
}

function showDeleteDialog() {
  if (!S || !S.activeSource) {
    showMessage({ title: '提示', text: '请先选择一个源。', error: false, link: '' });
    return;
  }
  openModal({
    title: '确认删除',
    body: `确定要删除源「${html(S.activeSourceDisplay)}」吗？本地 JSON 与缓存都会被删除。`,
    actions: [
      { label: '取消', kind: 'soft' },
      {
        label: '删除',
        kind: 'danger',
        onClick: () => invoke('delete_active_source'),
      },
    ],
  });
}

function showMessage(payload) {
  const title = payload.title || '';
  const error = Boolean(payload.error);
  openModal({
    title,
    iconName: error ? 'close' : 'check',
    iconClass: error ? 'err' : 'ok',
    body: html(payload.text || ''),
    actions: [
      ...(payload.link
        ? [
            {
              label: '用浏览器打开',
              kind: 'soft',
              onClick: () => invoke('open_external_url', { url: payload.link }),
            },
          ]
        : []),
      { label: '确定', kind: 'primary' },
    ],
  });
}

async function showUpdateDialog() {
  try {
    S = await invoke('get_snapshot');
  } catch (error) {
    /* keep the last snapshot */
  }
  const info = (S && S.updateInfo) || {};
  const notes = info.notes || '该版本没有提供更新说明。';
  openModal({
    title: `发现新版本 ${info.latest || ''}`,
    sub: `当前版本 ${info.current || ''}  →  ${info.latest || ''}`,
    width: 'wide',
    body: `<div style="color:var(--text);font-size:12px;margin-bottom:8px">${html(info.name || '')}</div>${html(notes)}`,
    actions: [
      ...(info.url
        ? [{ label: '打开发布页', kind: 'ghost', onClick: () => invoke('open_external_url', { url: info.url }) }]
        : []),
      ...(info.downloadUrl
        ? [{ label: '下载更新', kind: 'primary', onClick: () => invoke('open_external_url', { url: info.downloadUrl }) }]
        : []),
      { label: '关闭', kind: 'soft' },
    ],
  });
}

function showAboutDialog() {
  const total = S && S.sources.length ? S.sources[0].count : 0;
  const sourceCount = S ? Math.max(0, S.sources.length - 1) : 0;
  openModal({
    title: '关于',
    body: `
      <div style="color:var(--text);font-size:16px;font-weight:500;margin-bottom:4px">GameMagNet 游戏目录管理器</div>
      <div style="margin-bottom:12px">版本 ${html(S ? S.version : '')} · Rust / Tauri</div>
      <div style="color:var(--border-strong);height:1px;margin:0 0 12px"></div>
      <div>本地游戏目录管理器，支持去重搜索、来源筛选、详情与链接复制。</div>
      <div style="margin-top:8px">已加载 ${formatCount(total)} 个游戏 · ${sourceCount} 个源</div>
      <div style="margin-top:8px;color:var(--faint);font-size:10px">${html(S ? S.sourcesDir : '')}</div>`,
    actions: [{ label: '关闭', kind: 'primary' }],
  });
}

function showBtClientsDialog() {
  const catalog = (S && S.btCatalog) || [];
  const rows = catalog
    .map(
      (entry, index) => `
      <div class="client-row">
        <div class="info">
          <div class="name">${html(entry.name)}${entry.recommended ? ' · 推荐' : ''}</div>
          <div class="desc">${html(entry.desc)}</div>
        </div>
        <button class="btn soft" data-client="${index}">下载页</button>
      </div>`
    )
    .join('');
  openModal({
    title: '常见 BT 客户端',
    sub: '以下链接均指向官方下载页。安装时请留意捆绑组件。',
    width: 'wide',
    body: `<div style="display:flex;flex-direction:column;gap:8px;background:transparent;border:0;padding:0;max-height:52vh">${rows}</div>`,
    actions: [{ label: '关闭', kind: 'primary' }],
  });
  const layer = $('modalLayer');
  layer.querySelectorAll('[data-client]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const entry = catalog[Number(btn.dataset.client)];
      if (entry) invoke('open_external_url', { url: entry.url });
    });
  });
}

/* --------------------------------------------------------------- toast */
function showToast(text) {
  const toast = $('toast');
  toast.textContent = text;
  toast.classList.add('visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove('visible'), 1800);
}

window.addEventListener('DOMContentLoaded', boot);
