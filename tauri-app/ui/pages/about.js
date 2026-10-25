// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// About & updates. State lives in js/updates.js (shared with the banner); this page only renders it.
import * as upd from '../js/updates.js';
import { logo } from '../js/icons.js';

const TAGS = { new: ['New', '#16233A', '#8DBBFF'], improved: ['Better', '#16233A', '#8DBBFF'], fixed: ['Fixed', '#1F242B', '#A7B0BB'] };

/** "New: text" / "- Fixed: text" / plain lines -> [{tag, text}] */
function parseNotes(text) {
  return String(text || '').split(/\r?\n/).map((l) => l.replace(/^\s*[-*•]\s*/, '').trim()).filter(Boolean).map((l) => {
    const m = /^(new|improved|fixed)\s*[:\-–]\s*(.+)$/i.exec(l);
    return m ? { tag: TAGS[m[1].toLowerCase()], text: m[2] } : { tag: null, text: l };
  });
}

export default async function (ctx) {
  const { h, ui, api, icon, setChildren } = ctx;
  upd.bind(ctx);
  const about = await api.invoke('get_about');
  const settings = await api.invoke('get_settings').catch(() => ({ auto_check: true }));

  const status = h('span', { style: { display: 'inline-flex', alignSelf: 'flex-start', alignItems: 'center', gap: '8px', minHeight: '26px', padding: '3px 10px', borderRadius: '13px', fontSize: '12px', fontWeight: 500 } });
  const btnCheck = ui.button({ label: 'Check for updates', kind: 'primary', icon: 'refresh', onClick: () => upd.checkNow({ manual: true }) });
  const btnInstall = ui.button({ label: 'Install update', icon: 'down', onClick: () => upd.startUpdate() });
  btnInstall.style.display = 'none';
  const auto = ui.checkbox({ checked: !!settings.auto_check, label: 'Check automatically at startup',
    onChange: (v) => api.invoke('set_settings', { auto_check: v }).catch((e) => ctx.toast(e.message, 'bad')) });
  const subTitle = h('span', { style: { fontSize: '12px', color: 'var(--muted)' } });
  const notesEl = h('ul', { style: { margin: 0, padding: 0, listStyle: 'none', display: 'flex', flexDirection: 'column', gap: '14px', lineHeight: 1.5, maxWidth: '900px' } });

  const head = ui.pageHead({ title: 'About & updates', sub: `Task Reaper · version ${about.version}` });

  const hero = h('section', { class: 'card', 'aria-label': 'Task Reaper', style: { padding: '26px 30px', display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: '30px' } },
    h('span', { style: { width: '100px', height: '100px', borderRadius: '24px', background: '#111418', border: '1px solid var(--card-line)', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', flex: 'none' } }, logo(68)),
    h('div', { style: { flex: '1 1 320px', minWidth: 0, display: 'flex', flexDirection: 'column', gap: '10px' } },
      h('div', { class: 'col', style: { '--g': '4px' } }, h('h2', { style: { fontSize: '21px', fontWeight: 600, letterSpacing: '-.01em' } }, 'Task Reaper'),
        h('span', { class: 'muted' }, `Version ${about.version} · ${about.copyright} · ${about.license}`)),
      status,
      h('div', { class: 'row wrap', style: { '--g': '18px', marginTop: '4px' } }, h('div', { class: 'row' }, btnCheck, btnInstall), auto)));

  const notesCard = h('section', { class: 'card', style: { flex: 1, minHeight: '260px', padding: '24px 30px', display: 'flex', flexDirection: 'column', gap: '16px' } },
    h('div', { class: 'row', style: { alignItems: 'baseline', '--g': '10px' } }, h('h2', {}, "What's new"), subTitle), notesEl);

  let lastNotesKey = '';
  const paintNotes = (text, ver) => {
    const key = `${ver}|${text}`;
    if (key === lastNotesKey) return;
    lastNotesKey = key;
    subTitle.textContent = ver ? `in ${ver}` : '';
    const items = parseNotes(text);
    setChildren(notesEl, items.length ? items.map((n) => h('li', { style: { display: 'flex', gap: '12px' } },
      n.tag ? h('span', { style: { flex: 'none', width: '46px', marginTop: '2px', fontSize: '11px', textAlign: 'center', height: '18px', lineHeight: '18px', borderRadius: '9px', background: n.tag[1], color: n.tag[2] } }, n.tag[0])
        : h('span', { style: { flex: 'none', width: '46px', textAlign: 'center', color: 'var(--muted)' } }, '•'),
      h('span', { style: { userSelect: 'text', minWidth: 0, overflowWrap: 'anywhere' } }, n.text)))
      : h('li', { class: 'muted' }, 'Release notes appear here when an update is found.'));
  };

  const TONES = { ok: ['#12241B', '#5BD69A'], info: ['var(--info-bg)', '#CFE1FF'], bad: ['var(--bad-bg)', 'var(--bad-text)'], mute: ['transparent', 'var(--muted)'] };
  const paint = (s) => {
    const busy = ['checking', 'downloading', 'installing'].includes(s.phase);
    let tone = 'mute', text = 'Press “Check for updates” to see if a newer version is available.', lead = null;
    const info = s.info;
    if (s.phase === 'checking') { text = 'Checking for updates…'; lead = ui.spinner(); }
    else if (s.phase === 'available' || s.phase === 'failed') { tone = s.phase === 'failed' ? 'bad' : 'info'; text = s.phase === 'failed' ? `Update failed: ${s.error}` : `Version ${info.version} is available (you have ${info.current}).`; }
    else if (s.phase === 'downloading') { tone = 'info'; text = `Downloading version ${info.version}…`; lead = ui.spinner(); }
    else if (s.phase === 'installing') { tone = 'info'; text = 'Installing… the app will restart in a moment'; lead = ui.spinner(); }
    else if (s.phase === 'uptodate') { tone = 'ok'; text = `You're on the latest version (${info.current}).`; lead = icon('check', 13, { 'stroke-width': 2 }); }
    else if (s.phase === 'error') { tone = 'bad'; text = s.error; }
    status.style.background = TONES[tone][0]; status.style.color = TONES[tone][1];
    status.style.paddingLeft = tone === 'mute' ? '0' : '10px';
    setChildren(status, lead, text);
    btnCheck.disabled = busy;
    btnInstall.style.display = info?.available && !['downloading', 'installing', 'checking'].includes(s.phase) ? '' : 'none';
    if (info?.available) paintNotes(info.notes || 'No release notes were provided.', info.version);
    else paintNotes(info?.notes || about.release_notes_current, about.version);
  };

  const off = upd.subscribe(paint);
  ctx.onCleanup(off);
  paint(upd.state);
  if (!upd.state.checked) upd.checkNow({ manual: false });

  return [head, hero, notesCard];
}
