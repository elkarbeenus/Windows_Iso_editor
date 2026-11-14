// Windows ISO Editor – UI logic (vanilla JS, Tauri v1 global API)
'use strict';

const T = window.__TAURI__;
const $ = s => document.querySelector(s);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

const state = {
  lang: 'de',
  step: 1,
  catalog: null,
  iso: null,
  selected: new Set(),
  preset: 'rec',
  output: '',
  busy: false,
  done: null,
  version: '',
  openTech: new Set(),
};

try { const l = localStorage.getItem('wie-lang'); if (l === 'de' || l === 'en') state.lang = l; } catch (_) {}

const t = (k, ...a) => { const v = I18N[state.lang][k]; return typeof v === 'function' ? v(...a) : (v ?? k); };
const L = (o, f) => o[`${f}_${state.lang}`] ?? o[`${f}_de`];

async function invoke(cmd, args) {
  if (T) return T.tauri.invoke(cmd, args);
  return MOCK[cmd](args);
}

// ------------------------------------------------------------------ mock (browser preview)
const MOCK = {
  app_version: () => '0.0.1-preview',
  get_catalog: async () => JSON.stringify(await (await fetch('catalog.sample.json')).json()),
  inspect_iso: async ({ path }) => ({ path, file_name: path.split(/[\\/]/).pop(), volume_id: 'CCCOMA_X64FRE_DE-DE_DV9', size: 5.8e9, archs: ['x64'], image_file: 'install.wim', has_autounattend: false, editions: [{ name: 'Windows 11 Home', arch: 'x64', build: '26100' }, { name: 'Windows 11 Pro', arch: 'x64', build: '26100' }, { name: 'Windows 11 Education', arch: 'x64', build: '26100' }], windows11: true, suggested_output: path.replace(/\.iso$/i, '_debloated.iso') }),
  build_iso: async ({ dst }) => { for (const p of ['copy', 'patch', 'verify']) { for (let i = 0; i <= 10; i++) { await new Promise(r => setTimeout(r, 60)); progressHandler({ phase: p, done: i, total: 10 }); } } return { output: dst, xml_bytes: 20000, verified_entries: 9 }; },
  save_xml: async () => {},
  preview_script: async () => '# preview',
  open_folder: async () => {},
};

// ------------------------------------------------------------------ helpers
function fmtSize(b) {
  if (b > 1e9) return (b / 1073741824).toFixed(2) + ' GB';
  return (b / 1048576).toFixed(0) + ' MB';
}
function toast(msg, kind = '') {
  const el = $('#toast');
  el.className = `toast ${kind}`;
  el.innerHTML = msg;
  clearTimeout(toast._t);
  toast._t = setTimeout(() => el.classList.add('hidden'), 3800);
}
function presetIndex(p) { return { gentle: 0, rec: 1, max: 2 }[p]; }
function applyPreset(p) {
  state.preset = p;
  const idx = presetIndex(p);
  state.selected = new Set(state.catalog.items.filter(i => i.presets[idx]).map(i => i.id));
}
function detectPreset() {
  for (const p of ['gentle', 'rec', 'max']) {
    const idx = presetIndex(p);
    const ids = state.catalog.items.filter(i => i.presets[idx]).map(i => i.id);
    if (ids.length === state.selected.size && ids.every(id => state.selected.has(id))) return p;
  }
  return 'custom';
}

// ------------------------------------------------------------------ static texts
function renderStatic() {
  document.documentElement.lang = state.lang;
  document.querySelectorAll('[data-i18n]').forEach(el => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll('[data-ico]').forEach(el => { el.innerHTML = icon(el.dataset.ico); });
  $('#langBtn').textContent = state.lang === 'de' ? 'EN' : 'DE';
  $('#infoBtn').innerHTML = icon('info');
  $('#dropIcon').innerHTML = icon('upload');
  $('#buildBtn').innerHTML = `${icon('rocket')}<span>${t('build')}</span>`;
}

// ------------------------------------------------------------------ navigation
function go(step) {
  if (state.busy) return;
  if (step > 1 && !state.iso) { toast(t('noIso'), 'warn'); return; }
  state.step = step;
  render();
  window.scrollTo({ top: 0, behavior: 'smooth' });
}

function render() {
  for (let i = 1; i <= 3; i++) $('#page' + i).classList.toggle('hidden', state.step !== i);
  document.querySelectorAll('.step').forEach(b => {
    const s = +b.dataset.step;
    b.classList.toggle('active', s === state.step);
    b.classList.toggle('done', s < state.step);
    b.disabled = state.busy || (s > 1 && !state.iso);
  });
  $('#backBtn').style.visibility = state.step > 1 && !state.busy ? 'visible' : 'hidden';
  $('#nextBtn').style.visibility = state.step < 3 ? 'visible' : 'hidden';
  $('#nextBtn').disabled = !state.iso;
  $('#footNote').textContent = state.iso ? `${state.iso.file_name} · ${t('selected', state.selected.size)}` : '';
  if (state.step === 1) renderIso();
  if (state.step === 2) renderChoose();
  if (state.step === 3) renderBuild();
}

// ------------------------------------------------------------------ step 1
async function pickIso() {
  if (!T) return loadIso('/home/deck/Downloads/Win11_24H2_German_x64.iso');
  const p = await T.dialog.open({ multiple: false, filters: [{ name: 'Windows ISO', extensions: ['iso', 'ISO'] }] });
  if (p) loadIso(Array.isArray(p) ? p[0] : p);
}

async function loadIso(path) {
  const box = $('#isoInfo');
  box.classList.remove('hidden');
  box.innerHTML = `<div class="loading"><span class="spinner"></span>${t('reading')}</div>`;
  try {
    const info = await invoke('inspect_iso', { path });
    state.iso = info;
    state.output = info.suggested_output;
    state.done = null;
  } catch (e) {
    state.iso = null;
    box.innerHTML = `<div class="alert error">${icon('alert')}<div><b>${t('failed')}</b><br>${esc(e)}</div></div>`;
    render();
    return;
  }
  render();
}

function renderIso() {
  const i = state.iso;
  const box = $('#isoInfo');
  $('#drop').classList.toggle('compact', !!i);
  if (!i) return;
  const eds = [...new Set(i.editions.map(e => e.name))];
  box.classList.remove('hidden');
  box.innerHTML = `
    <div class="iso-card">
      <div class="iso-head">
        <div class="iso-badge">${icon('disc')}</div>
        <div class="iso-title">
          <div class="ok-line">${icon('check')} ${t('isoOk')}</div>
          <div class="file">${esc(i.file_name)}</div>
        </div>
        <button class="btn ghost" id="otherIso">${t('otherIso')}</button>
      </div>
      <div class="facts">
        <div><span>${t('volume')}</span><b>${esc(i.volume_id || '–')}</b></div>
        <div><span>${t('arch')}</span><b>${i.archs.map(a => `<i class="tag">${esc(a)}</i>`).join(' ') || '–'}</b></div>
        <div><span>${t('size')}</span><b>${fmtSize(i.size)}</b></div>
        <div><span>${t('image')}</span><b>${esc(i.image_file)}</b></div>
      </div>
      ${eds.length ? `<div class="eds"><span>${t('editions')}</span><div>${eds.map(e => `<i class="tag soft">${esc(e)}</i>`).join('')}</div></div>` : ''}
      ${i.has_autounattend ? `<div class="alert warn">${icon('alert')}<div>${t('hasUnattend')}</div></div>` : ''}
      ${i.windows11 === false ? `<div class="alert warn">${icon('alert')}<div>${t('notWin11')}</div></div>` : ''}
    </div>`;
  $('#otherIso').onclick = pickIso;
}

// ------------------------------------------------------------------ step 2
function renderChoose() {
  const c = state.catalog;
  const cur = detectPreset();
  const presets = [
    { id: 'gentle', icon: 'feather', name: t('presetGentle'), desc: t('presetGentleD') },
    { id: 'rec', icon: 'shield-check', name: t('presetRec'), desc: t('presetRecD'), badge: t('badgeRec') },
    { id: 'max', icon: 'zap', name: t('presetMax'), desc: t('presetMaxD') },
  ];
  $('#presets').innerHTML = presets.map(p => {
    const n = c.items.filter(i => i.presets[presetIndex(p.id)]).length;
    return `<button class="preset ${cur === p.id ? 'on' : ''}" data-preset="${p.id}">
      ${p.badge ? `<span class="badge">${p.badge}</span>` : ''}
      <div class="preset-ico">${icon(p.icon)}</div>
      <div class="preset-name">${p.name}</div>
      <div class="preset-desc">${p.desc}</div>
      <div class="preset-n">${t('selected', n)}</div>
    </button>`;
  }).join('') + (cur === 'custom' ? `<div class="custom-pill">${icon('sliders')} ${t('presetCustom')}</div>` : '');

  const prot = state.lang === 'de' ? c.protected_de : c.protected_en;
  $('#protected').innerHTML = `<div class="prot-head">${icon('shield-check')}<b>${t('protectedTitle')}</b></div>
    <div class="prot-list">${prot.map(p => `<span>${icon('check')}${esc(p)}</span>`).join('')}</div>`;

  $('#groups').innerHTML = c.groups.map(g => {
    const items = c.items.filter(i => i.group === g.id);
    const on = items.filter(i => state.selected.has(i.id)).length;
    return `<section class="group">
      <div class="group-head">
        <div class="group-ico">${icon(g.icon)}</div>
        <div class="group-txt"><h3>${esc(L(g, 'title'))}</h3><p>${esc(L(g, 'desc'))}</p></div>
        <div class="group-tools">
          <span class="group-count">${t('groupsCount', on, items.length)}</span>
          <button class="mini" data-gall="${g.id}">${t('all')}</button>
          <button class="mini" data-gnone="${g.id}">${t('none')}</button>
        </div>
      </div>
      <div class="tiles">${items.map(tile).join('')}</div>
    </section>`;
  }).join('');
  $('#counter').innerHTML = `<b>${state.selected.size}</b><span>${t('selected', '').trim()}</span>`;
  $('#footNote').textContent = `${state.iso.file_name} · ${t('selected', state.selected.size)}`;
}

function tile(i) {
  const on = state.selected.has(i.id);
  const open = state.openTech.has(i.id);
  const warn = L(i, 'warn');
  return `<div class="tile ${on ? 'on' : ''}" data-item="${i.id}" role="checkbox" aria-checked="${on}" tabindex="0">
    <div class="tile-top">
      <div class="tile-ico">${icon(i.icon)}</div>
      <div class="switch ${on ? 'on' : ''}"><span></span></div>
    </div>
    <div class="tile-title">${esc(L(i, 'title'))}</div>
    <div class="tile-desc">${esc(L(i, 'desc'))}</div>
    ${warn ? `<div class="tile-warn">${icon('alert')}<span>${esc(warn)}</span></div>` : ''}
    <button class="tech-btn" data-tech="${i.id}">${icon('code')}${t('whatItDoes')}</button>
    ${open ? `<div class="tech">${esc(i.tech)}</div>` : ''}
  </div>`;
}

function onChooseClick(e) {
  const pb = e.target.closest('[data-preset]');
  if (pb) { applyPreset(pb.dataset.preset); renderChoose(); return; }
  const tb = e.target.closest('[data-tech]');
  if (tb) { e.stopPropagation(); const id = tb.dataset.tech; state.openTech.has(id) ? state.openTech.delete(id) : state.openTech.add(id); renderChoose(); return; }
  const ga = e.target.closest('[data-gall]');
  if (ga) { state.catalog.items.filter(i => i.group === ga.dataset.gall).forEach(i => state.selected.add(i.id)); renderChoose(); return; }
  const gn = e.target.closest('[data-gnone]');
  if (gn) { state.catalog.items.filter(i => i.group === gn.dataset.gnone).forEach(i => state.selected.delete(i.id)); renderChoose(); return; }
  const tl = e.target.closest('[data-item]');
  if (tl) { const id = tl.dataset.item; state.selected.has(id) ? state.selected.delete(id) : state.selected.add(id); renderChoose(); }
}

// ------------------------------------------------------------------ step 3
function renderBuild() {
  const c = state.catalog;
  $('#summary').innerHTML = c.groups.map(g => {
    const items = c.items.filter(i => i.group === g.id && state.selected.has(i.id));
    if (!items.length) return '';
    return `<div class="sum-group"><div class="sum-head">${icon(g.icon)}${esc(L(g, 'title'))}</div>
      <div class="sum-items">${items.map(i => `<span>${esc(L(i, 'title'))}</span>`).join('')}</div></div>`;
  }).join('') || `<p class="muted">${t('nothingSelected')}</p>`;
  $('#summary').insertAdjacentHTML('beforeend', `<button class="btn link" id="scriptBtn">${icon('code')}${t('viewScript')}</button>`);
  $('#scriptBtn').onclick = showScript;
  $('#outPath').textContent = state.output || '–';
  $('#buildBtn').disabled = state.busy || state.selected.size === 0;
  $('#buildArea').classList.toggle('hidden', state.busy || !!state.done);
  $('#progressArea').classList.toggle('hidden', !state.busy);
  $('#outBtn').disabled = state.busy;
  renderResult();
}

async function chooseOutput() {
  if (!T) return;
  const p = await T.dialog.save({ title: t('chooseOutput'), defaultPath: state.output, filters: [{ name: 'ISO', extensions: ['iso'] }] });
  if (p) { state.output = /\.iso$/i.test(p) ? p : p + '.iso'; renderBuild(); }
}

let progressHandler = () => {};
const PH = ['copy', 'patch', 'verify'];

async function startBuild() {
  if (state.selected.size === 0) { toast(t('nothingSelected'), 'warn'); return; }
  state.busy = true; state.done = null;
  render();
  const phaseLabel = { copy: t('phaseCopy'), patch: t('phasePatch'), verify: t('phaseVerify') };
  progressHandler = ({ phase, done, total }) => {
    const pi = PH.indexOf(phase);
    $('#phases').innerHTML = PH.map((p, k) => `<span class="${k < pi ? 'done' : k === pi ? 'active' : ''}">${k < pi ? icon('check') : `<i>${k + 1}</i>`}${phaseLabel[p]}</span>`).join('');
    const frac = total ? done / total : 0;
    const overall = phase === 'copy' ? frac * 0.94 : phase === 'patch' ? 0.94 + frac * 0.03 : 0.97 + frac * 0.03;
    $('#barFill').style.width = (overall * 100).toFixed(1) + '%';
    $('#barLabel').textContent = phase === 'copy' ? `${phaseLabel.copy} · ${fmtSize(done)} / ${fmtSize(total)} · ${(overall * 100).toFixed(0)} %` : `${phaseLabel[phase]} …`;
  };
  progressHandler({ phase: 'copy', done: 0, total: state.iso.size });
  let unlisten = null;
  if (T) unlisten = await T.event.listen('build-progress', e => progressHandler(e.payload));
  try {
    const res = await invoke('build_iso', { src: state.iso.path, dst: state.output, selected: [...state.selected] });
    state.done = { ok: true, res };
  } catch (e) {
    state.done = { ok: false, err: String(e) };
  } finally {
    if (unlisten) unlisten();
    state.busy = false;
    render();
  }
}

function renderResult() {
  const box = $('#resultArea');
  if (!state.done) { box.classList.add('hidden'); return; }
  box.classList.remove('hidden');
  if (state.done.ok) {
    box.innerHTML = `<div class="result ok">
      <div class="result-ico">${icon('check')}</div>
      <h3>${t('doneTitle')}</h3>
      <p>${t('doneText')}</p>
      <div class="path small">${esc(state.done.res.output)}</div>
      <div class="next-steps"><b>${t('doneNext')}</b><ol>${t('doneSteps').map(s => `<li>${esc(s)}</li>`).join('')}</ol></div>
      <div class="row"><button class="btn ghost" id="openFolder">${icon('folder')}${t('openFolder')}</button>
      <button class="btn primary" id="again">${t('again')}</button></div>
    </div>`;
    $('#openFolder').onclick = () => invoke('open_folder', { path: state.done.res.output }).catch(e => toast(esc(e), 'error'));
    $('#again').onclick = () => { state.done = null; state.iso = null; state.output = ''; $('#isoInfo').classList.add('hidden'); go(1); };
  } else {
    box.innerHTML = `<div class="alert error">${icon('alert')}<div><b>${t('failed')}</b><br>${esc(state.done.err)}</div></div>`;
    $('#buildArea').classList.remove('hidden');
  }
}

async function saveXml() {
  let p = 'autounattend.xml';
  if (T) {
    p = await T.dialog.save({ defaultPath: 'autounattend.xml', filters: [{ name: 'XML', extensions: ['xml'] }] });
    if (!p) return;
  }
  try { await invoke('save_xml', { path: p, selected: [...state.selected] }); toast(`${icon('check')} ${t('xmlSaved')}`, 'ok'); }
  catch (e) { toast(esc(e), 'error'); }
}

async function showScript() {
  let s = '';
  try { s = await invoke('preview_script', { selected: [...state.selected] }); } catch (e) { s = String(e); }
  modal(`<div class="modal-head"><h3>${icon('code')}${t('scriptTitle')}</h3><button class="icon-btn" data-close>${icon('x')}</button></div>
    <pre class="script">${esc(s)}</pre>`);
}

// ------------------------------------------------------------------ modal / info
function modal(html) {
  $('#modalCard').innerHTML = html;
  $('#modal').classList.remove('hidden');
}
function closeModal() { $('#modal').classList.add('hidden'); }

function showInfo() {
  modal(`<div class="modal-head"><h3>${t('infoTitle')}</h3><button class="icon-btn" data-close>${icon('x')}</button></div>
    <div class="about">
      <img src="icon.png" alt="" class="about-logo" />
      <div class="about-name">Windows ISO Editor</div>
      <div class="about-ver">Version ${esc(state.version)}</div>
      <p>${t('infoText')}</p>
      <div class="credits">${icon('heart')}${t('credits')}</div>
      <div class="about-box"><b>${t('howWorks')}</b><p>${esc(t('howWorksText'))}</p></div>
      <button class="btn link" id="ghLink">github.com/elkarbeenus/Windows_Iso_editor</button>
    </div>`);
  $('#ghLink').onclick = () => { const u = 'https://github.com/elkarbeenus/Windows_Iso_editor'; T ? T.shell.open(u) : window.open(u); };
}

// ------------------------------------------------------------------ init
async function init() {
  renderStatic();
  try {
    state.version = await invoke('app_version');
    state.catalog = JSON.parse(await invoke('get_catalog'));
  } catch (e) {
    document.body.innerHTML = `<pre style="padding:2rem;color:#f88">${esc(e)}</pre>`;
    return;
  }
  applyPreset('rec');

  $('#pickBtn').onclick = e => { e.stopPropagation(); pickIso(); };
  $('#drop').onclick = pickIso;
  $('#drop').onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') pickIso(); };
  $('#nextBtn').onclick = () => go(state.step + 1);
  $('#backBtn').onclick = () => go(state.step - 1);
  document.querySelectorAll('.step').forEach(b => b.onclick = () => go(+b.dataset.step));
  $('#page2').addEventListener('click', onChooseClick);
  $('#page2').addEventListener('keydown', e => { if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('[data-item]')) { e.preventDefault(); onChooseClick(e); } });
  $('#outBtn').onclick = chooseOutput;
  $('#buildBtn').onclick = startBuild;
  $('#xmlBtn').onclick = saveXml;
  $('#infoBtn').onclick = showInfo;
  $('#langBtn').onclick = () => { state.lang = state.lang === 'de' ? 'en' : 'de'; try { localStorage.setItem('wie-lang', state.lang); } catch (_) {} renderStatic(); render(); };
  $('#modal').addEventListener('click', e => { if (e.target.id === 'modal' || e.target.closest('[data-close]')) closeModal(); });
  document.addEventListener('keydown', e => { if (e.key === 'Escape') closeModal(); });

  if (T) {
    T.event.listen('tauri://file-drop', e => {
      $('#drop').classList.remove('hover');
      const f = (e.payload || []).find(p => /\.iso$/i.test(p));
      if (f && !state.busy) { go(1); loadIso(f); }
    });
    T.event.listen('tauri://file-drop-hover', () => $('#drop').classList.add('hover'));
    T.event.listen('tauri://file-drop-cancelled', () => $('#drop').classList.remove('hover'));
  }
  render();
}

init();
