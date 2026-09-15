// GIF Library file manager (v2): folders left, files right; drop/browse queues, Upload sends; mkdir/rename/delete/preview/download (parity with the ESP32 firmware)
import { t } from './i18n.js';

export function initGifLibrary() {
  const $ = (id) => document.getElementById(id);
  const folderList = $('gif-fm-folder-list'), newFolder = $('gif-fm-newfolder'), mkdirBtn = $('gif-fm-mkdir');
  const title = $('gif-fm-title'), countEl = $('gif-fm-count'), folderActions = $('gif-fm-folder-actions'), renameFolderBtn = $('gif-fm-rename-folder'), deleteFolderBtn = $('gif-fm-delete-folder');
  const dropZone = $('gif-fm-drop'), dropTarget = $('gif-fm-drop-target'), pickDir = $('gif-fm-pick-dir'), filesIn = $('gif-fm-files-input'), dirIn = $('gif-fm-dir-input');
  const list = $('gif-fm-list'), moreBtn = $('gif-fm-more'), rescanBtn = $('gif-fm-rescan'), progress = $('gif-lib-progress'), uploadBtn = $('gif-lib-upload-btn');
  const orientTabs = $('gif-fm-orientation');
  if (!folderList || !dropZone || !uploadBtn) return;
  const toast = (m, kind) => (window.showToast ? window.showToast(m, kind) : console.log(kind, m));
  const tr = (k, f, v) => { let s = (typeof t === 'function') ? t(k, f) : f; if (v) Object.keys(v).forEach(x => { s = s.split('{' + x + '}').join(v[x]); }); return s; };
  const headers = () => { const h = {}; const tok = localStorage.getItem('api_token'); if (tok) h['X-API-Token'] = tok; return h; };
  const api = async (url, opts) => { const r = await fetch(url, Object.assign({ headers: headers() }, opts || {})); let b = {}; try { b = await r.json(); } catch (_) {} if (!r.ok) throw new Error(b.message || ('HTTP ' + r.status)); return b; };
  const esc = (s) => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const enc = encodeURIComponent;
  const fmtBytes = (b) => (b === undefined || b === null) ? '' : b >= 1048576 ? (b / 1048576).toFixed(1) + ' MB' : b >= 1024 ? Math.round(b / 1024) + ' KB' : b + ' B';
  const BATCH = 6, PAGE = 100, ALLOWED = /\.(gif|png|raw)$/i;
  let folders = {}, selected = null, files = [], shown = 0, pending = null, busy = false;
  // Which library we are managing: 'yoko' -> /gifs, 'tate' -> /gifs_tate (the engine keeps them separate).
  let orientation = 'yoko';
  const orient = () => 'orientation=' + orientation;
  let selSeq = 0;                 // ignore folder listings that arrive after a newer selection
  let previewCtl = null;          // one preview fetch at a time (matters on the ESP32; harmless here)
  const PREVIEW_AUTO_MAX = 512 * 1024;
  // media fetch through the authenticated path (works with the API token on, no URL tokens)
  async function fetchBlobUrl(url) { const r = await fetch(url, { headers: headers() }); if (!r.ok) throw new Error('HTTP ' + r.status); return URL.createObjectURL(await r.blob()); }
  async function downloadFile(url, name) { try { const u = await fetchBlobUrl(url); const a = document.createElement('a'); a.href = u; a.download = name; document.body.appendChild(a); a.click(); a.remove(); setTimeout(() => URL.revokeObjectURL(u), 10000); } catch (e) { toast('Download failed: ' + (e.message || e), 'error'); } }

  // ---------------- in-page confirm (the browser's confirm() is easy to miss) ----------------
  function fmConfirm(titleText, bodyText, okText) {
    return new Promise(resolve => {
      const ov = document.createElement('div'); ov.className = 'modal-overlay'; ov.style.cssText = 'position: fixed; inset: 0; background: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 10000;';
      ov.innerHTML = `<div class="card" style="max-width: 460px; width: 92%; padding: 1.2rem; background: var(--bg-secondary, #111827); border: 1px solid var(--accent-primary, #6cf); border-radius: 10px;">
          <h3 style="margin: 0 0 0.6rem 0;">${esc(titleText)}</h3>
          <p style="margin: 0 0 1rem 0; line-height: 1.4;">${esc(bodyText)}</p>
          <div style="display: flex; gap: 0.6rem; justify-content: flex-end;">
            <button class="btn" data-act="cancel">${tr('gifs_cancel', 'Cancel')}</button>
            <button class="btn btn-primary" data-act="ok">${esc(okText)}</button>
          </div></div>`;
      const done = (v) => { ov.remove(); resolve(v); };
      ov.querySelector('[data-act="cancel"]').onclick = () => done(false);
      ov.querySelector('[data-act="ok"]').onclick = () => done(true);
      ov.onclick = (e) => { if (e.target === ov) done(false); };
      document.body.appendChild(ov);
    });
  }
  // ---------------- full rescan (background + polled on the ESP32; synchronous on the RPi) ----------------
  async function pollRescan() {
    for (;;) {
      await new Promise(r => setTimeout(r, 2000));
      let st = {}; try { st = await api('/api/gifs/reindex/status'); } catch (_) { continue; }
      if (st.running) {
        const pct = st.expected ? Math.min(100, Math.round((st.files || 0) * 100 / st.expected)) : (st.total ? Math.round((st.done || 0) * 100 / st.total) : 0);
        progress.innerHTML = `<div style="display:flex; align-items:center; gap:0.6rem;"><div style="flex:1; height:8px; background: rgba(255,255,255,0.12); border-radius:4px; overflow:hidden;"><div style="width:${pct}%; height:100%; background: var(--accent-primary, #6cf); transition: width 0.4s;"></div></div><span style="white-space:nowrap;">${esc(tr('gifs_rescan_progress', 'Rescanning {done}/{total}: {current}', { done: st.done || 0, total: st.total || '?', current: st.current || '' }))} · ${(st.files || 0).toLocaleString()}${st.expected ? '/' + st.expected.toLocaleString() : ''} ${esc(tr('gifs_files_word', 'files'))} · ${Math.round((st.elapsed_ms || 0) / 1000)}s${st.eta ? ' · ' + esc(st.eta) : ''}${st.cancelling ? ' · ' + esc(tr('gifs_rescan_cancelling', 'cancelling…')) : ''}</span><a href="#" class="fm-rescan-cancel" style="white-space:nowrap;">${esc(tr('gifs_cancel', 'Cancel'))}</a></div>`;
        const c = progress.querySelector('.fm-rescan-cancel'); if (c) c.onclick = async (e) => { e.preventDefault(); try { await fetch('/api/gifs/reindex', { method: 'DELETE', headers: headers() }); } catch (_) {} };
        continue;
      }
      progress.textContent = tr('gifs_rescan_done', 'Rescan finished ({result})', { result: st.last_result || '' }) + ` · ${(st.files || 0).toLocaleString()} ${tr('gifs_files_word', 'files')} · ${Math.round((st.elapsed_ms || 0) / 1000)}s`;
      return;
    }
  }
  if (rescanBtn) rescanBtn.addEventListener('click', async () => {
    if (busy) return;
    if (!await fmConfirm(tr('gifs_rescan', '🔄 Rescan library'), tr('gifs_rescan_confirm', 'Rebuild the index of every folder from the card? On a large library this takes minutes and the panel pauses meanwhile.'), tr('gifs_rescan_go', 'Continue'))) return;
    busy = true; rescanBtn.disabled = true;
    try {
      const r = await fetch('/api/gifs/reindex', { method: 'POST', headers: headers() });
      if (r.status === 202) { progress.textContent = tr('gifs_rescan_started', 'Rescan started…'); await pollRescan(); }
      else if (r.status === 409) { toast(tr('gifs_rescan_busy', 'A rescan is already running'), 'info'); await pollRescan(); }
      else if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.message || ('HTTP ' + r.status)); }
      else { progress.textContent = tr('gifs_rescan_done', 'Rescan finished ({result})', { result: 'ok' }); }   // synchronous firmware (RPi)
      await loadLibrary(true);
    } catch (e) { toast('Rescan failed: ' + (e.message || e), 'error'); }
    finally { busy = false; rescanBtn.disabled = false; }
  });
  // ---------------- folders (left pane) ----------------
  async function loadLibrary(keep) {
    try {
      const data = await api('/api/gifs/library?' + orient()); folders = data.folders || {};
      const names = Object.keys(folders).sort((a, b) => a.localeCompare(b));
      folderList.innerHTML = names.length ? '' : '<span class="text-muted">No playlist folders yet.</span>';
      names.forEach(n => {
        const b = document.createElement('button'); b.className = 'btn'; b.dataset.folder = n;
        b.style.cssText = 'text-align: left; padding: 0.3rem 0.6rem; display: flex; justify-content: space-between; gap: 0.5rem;' + (n === selected ? ' border: 1px solid var(--accent, #6cf);' : ' opacity: 0.85;');
        b.innerHTML = `<span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">${esc(n)}</span><span class="text-muted">${folders[n].count}</span>`;
        b.onclick = () => selectFolder(n); folderList.appendChild(b);
      });
      if (selected && !names.includes(selected)) { selected = null; renderFiles([]); }
      else if (selected && keep !== false) selectFolder(selected, true);
    } catch (e) { folderList.textContent = 'Library unavailable: ' + (e.message || e); }
  }

  async function selectFolder(name, silent) {
    selected = name; dropTarget.textContent = name ? `“${name}”` : tr('gifs_drop_target_default', 'the selected folder');
    folderList.querySelectorAll('button').forEach(b => { b.style.border = b.dataset.folder === name ? '1px solid var(--accent, #6cf)' : ''; b.style.opacity = b.dataset.folder === name ? '1' : '0.85'; });
    if (!name) { title.textContent = tr('gifs_select_folder', 'Select a folder'); countEl.textContent = ''; folderActions.style.display = 'none'; renderFiles([]); return; }
    const seq = ++selSeq;
    if (previewCtl) { previewCtl.abort(); previewCtl = null; }
    title.textContent = name; countEl.textContent = ''; folderActions.style.display = '';
    list.innerHTML = `<div class="text-muted" style="padding: 0.4rem;">${tr('gifs_loading', 'Loading…')}</div>`; moreBtn.style.display = 'none';
    try {
      const data = await api('/api/gifs/files?' + orient() + '&folder=' + enc(name));
      if (seq !== selSeq) return;   // the user moved on to another folder meanwhile
      files = data.files || []; countEl.textContent = tr('gifs_files_count', '{n} file(s)', { n: files.length });
      renderFiles(files, true);
    } catch (e) { if (seq === selSeq) { list.innerHTML = ''; if (!silent) toast('Could not list folder: ' + (e.message || e), 'error'); } }
  }

  // ---------------- files (right pane) ----------------
  function renderFiles(fl, reset) {
    if (reset) { list.innerHTML = ''; shown = 0; }
    if (!fl.length) { if (selected) list.innerHTML = `<div class="text-muted" style="padding: 0.4rem;">${tr('gifs_empty_folder', 'Empty folder.')}</div>`; else list.innerHTML = ''; moreBtn.style.display = 'none'; return; }
    const slice = fl.slice(shown, shown + PAGE); shown += slice.length;
    slice.forEach(f => {
      const row = document.createElement('div');
      row.style.cssText = 'display: flex; align-items: center; gap: 0.6rem; padding: 0.25rem 0.4rem; border-bottom: 1px solid rgba(255,255,255,0.06);';
      const url = `/api/gifs/file?${orient()}&folder=${enc(selected)}&name=${enc(f.name)}`;
      row.innerHTML = `<a href="#" class="fm-name" style="flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: inherit; text-decoration: none;" title="Click to preview">${esc(f.name)}</a>
        <span class="text-muted" style="white-space: nowrap;">${fmtBytes(f.bytes)}</span>
        <a href="#" class="fm-dl text-muted" title="Download" style="text-decoration: none;">⤓</a>
        <a href="#" class="fm-rename text-muted" title="Rename" style="text-decoration: none;">✎</a>
        <a href="#" class="fm-del" title="Delete" style="color: #f87171; text-decoration: none;">✕</a>`;
      const nameEl = row.querySelector('.fm-name');
      nameEl.onclick = (e) => {
        e.preventDefault(); let pv = row.nextElementSibling;
        if (pv && pv.classList.contains('fm-preview')) { pv.remove(); return; }
        pv = document.createElement('div'); pv.className = 'fm-preview'; pv.style.cssText = 'padding: 0.4rem; background: rgba(0,0,0,0.3);';
        row.after(pv);
        const load = () => {
          if (previewCtl) previewCtl.abort();
          const ctl = new AbortController(); previewCtl = ctl;
          pv.innerHTML = `<span class="text-muted">${tr('gifs_preview_loading', 'loading preview…')}${f.bytes ? ' (' + fmtBytes(f.bytes) + ')' : ''}</span>`;
          fetch(url, { headers: headers(), signal: ctl.signal }).then(async r => { if (!r.ok) throw new Error('HTTP ' + r.status); const u = URL.createObjectURL(await r.blob()); pv.innerHTML = `<img src="${u}" alt="${esc(f.name)}" style="image-rendering: pixelated; max-width: 100%; height: 64px;">`; })
            .catch(err => { if (err && err.name === 'AbortError') { pv.remove(); return; } pv.innerHTML = `<span class="text-muted">preview failed: ${esc(err.message || err)}</span>`; })
            .finally(() => { if (previewCtl === ctl) previewCtl = null; });
        };
        if (f.bytes && f.bytes > PREVIEW_AUTO_MAX) {
          pv.innerHTML = `<span class="text-muted">${tr('gifs_preview_large', 'Large file ({size}) — previewing it will take a while.', { size: fmtBytes(f.bytes) })} <a href="#" class="fm-preview-anyway">${tr('gifs_preview_anyway', 'Load preview anyway')}</a></span>`;
          pv.querySelector('.fm-preview-anyway').onclick = (ev) => { ev.preventDefault(); load(); };
        } else load();
      };
      row.querySelector('.fm-dl').onclick = (e) => { e.preventDefault(); downloadFile(url + '&download=1', f.name); };
      row.querySelector('.fm-rename').onclick = async (e) => {
        e.preventDefault(); const to = prompt(`Rename ${f.name} to:`, f.name); if (!to || to === f.name) return;
        try { await api(`/api/gifs/rename?${orient()}&folder=${enc(selected)}&name=${enc(f.name)}&to=${enc(to)}`, { method: 'POST' }); toast('Renamed', 'success'); selectFolder(selected, true); }
        catch (err) { toast('Rename failed: ' + (err.message || err), 'error'); }
      };
      row.querySelector('.fm-del').onclick = async (e) => {
        e.preventDefault(); if (!confirm(tr('gifs_confirm_delete_file', 'Delete {name} from {folder}?', { name: f.name, folder: selected }))) return;
        try { await api(`/api/gifs/file?${orient()}&folder=${enc(selected)}&name=${enc(f.name)}`, { method: 'DELETE' }); toast(`Deleted ${f.name}`, 'success'); await selectFolder(selected, true); loadLibrary(false); }
        catch (err) { toast('Delete failed: ' + (err.message || err), 'error'); }
      };
      list.appendChild(row);
    });
    moreBtn.style.display = shown < fl.length ? '' : 'none'; moreBtn.textContent = `${tr('gifs_show_more', 'Show more')} (${fl.length - shown})`;
  }
  moreBtn.onclick = () => renderFiles(files, false);

  // ---------------- folder actions ----------------
  mkdirBtn.onclick = async () => {
    const name = (newFolder.value || '').trim(); if (!name) { toast('Type a folder name first.', 'error'); return; }
    try { const r = await api('/api/gifs/mkdir?' + orient() + '&folder=' + enc(name), { method: 'POST' }); newFolder.value = ''; toast(`Created ${r.folder}`, 'success'); selected = r.folder; await loadLibrary(); }
    catch (e) { toast('Create failed: ' + (e.message || e), 'error'); }
  };
  renameFolderBtn.onclick = async () => {
    if (!selected) return; const to = prompt(`Rename folder ${selected} to:`, selected); if (!to || to === selected) return;
    try { await api(`/api/gifs/rename?${orient()}&folder=${enc(selected)}&to=${enc(to)}`, { method: 'POST' }); toast('Folder renamed', 'success'); selected = to.trim(); await loadLibrary(); }
    catch (e) { toast('Rename failed: ' + (e.message || e), 'error'); }
  };
  deleteFolderBtn.onclick = async () => {
    if (!selected) return; if (!confirm(tr('gifs_confirm_delete_folder', 'Delete the whole playlist folder "{folder}" and every file in it? This cannot be undone.', { folder: selected }))) return;
    try { const d = await api('/api/gifs/folder?' + orient() + '&folder=' + enc(selected), { method: 'DELETE' }); toast(`Deleted ${selected} (${d.files} files)`, 'success'); selected = null; await loadLibrary(); selectFolder(null); }
    catch (e) { toast('Delete failed: ' + (e.message || e), 'error'); }
  };

  // ---------------- uploads: queue by drop/browse, send with the Upload button ----------------
  async function uploadBatch(folder, batch) {
    const fd = new FormData(); batch.forEach(f => fd.append('file', f, f.name));
    const r = await fetch('/api/gifs/upload?' + orient() + '&folder=' + enc(folder), { method: 'POST', headers: headers(), body: fd });
    let body = {}; try { body = await r.json(); } catch (_) {}
    if (!r.ok && !(body.saved && body.saved.length)) throw new Error(body.message || ('HTTP ' + r.status));
    return body;
  }
  async function uploadGroups(groups) {
    if (busy) { toast('An upload is already running.', 'info'); return; }
    let total = 0; groups.forEach(v => total += v.length); let done = 0, saved = 0, skipped = [];
    busy = true; uploadBtn.disabled = true; dropZone.style.opacity = '0.6';
    try {
      for (const [folder, fl] of groups) for (let i = 0; i < fl.length; i += BATCH) {
        const batch = fl.slice(i, i + BATCH); progress.textContent = tr('gifs_uploading', 'Uploading to {folder}: {done}/{total}…', { folder, done, total });
        const r = await uploadBatch(folder, batch); saved += (r.saved || []).length; skipped = skipped.concat(r.skipped || []); done += batch.length;
      }
      progress.textContent = tr('gifs_done', 'Done: {saved} saved', { saved }) + (skipped.length ? tr('gifs_skipped', ', {n} skipped', { n: skipped.length }) + ` (${skipped.map(s => s.name).slice(0, 5).join(', ')}${skipped.length > 5 ? '…' : ''})` : '') + '.';
      toast(`Uploaded ${saved} file(s)` + (skipped.length ? `, ${skipped.length} skipped` : ''), skipped.length && !saved ? 'error' : 'success');
    } catch (e) { progress.textContent = 'Upload failed: ' + (e.message || e); toast('Upload failed: ' + (e.message || e), 'error'); }
    finally {
      busy = false; dropZone.style.opacity = ''; filesIn.value = ''; dirIn.value = '';
      if (groups.size === 1) selected = Array.from(groups.keys())[0];
      await loadLibrary();
    }
  }
  function queueSelection(items) {
    const groups = new Map(); let looseNoTarget = 0;
    items.forEach(({ file, relPath }) => {
      if (!ALLOWED.test(file.name)) return;
      const parts = (relPath || file.name).split('/').filter(Boolean);
      const folder = parts.length > 1 ? parts[0] : selected;
      if (!folder) { looseNoTarget++; return; }
      if (!groups.has(folder)) groups.set(folder, []); groups.get(folder).push(file);
    });
    if (!groups.size) { pending = null; uploadBtn.disabled = true; progress.textContent = looseNoTarget ? tr('gifs_select_first', 'Select a folder on the left first, then choose the files again.') : tr('gifs_none_valid', 'No .gif/.png/.raw files in that selection.'); return; }
    pending = groups; uploadBtn.disabled = false; let n = 0; groups.forEach(v => n += v.length);
    progress.textContent = tr('gifs_queued', '{n} file(s) queued for {k} playlist(s): {list}. Click Upload to send.', { n, k: groups.size, list: Array.from(groups.keys()).join(', ') }) + (looseNoTarget ? ` (${looseNoTarget} loose file(s) ignored: no folder selected.)` : '');
  }
  async function collectDropped(dt) {
    const out = []; const items = dt.items ? Array.from(dt.items) : [];
    const entries = items.map(it => (it.webkitGetAsEntry ? it.webkitGetAsEntry() : null)).filter(Boolean);
    if (!entries.length) { Array.from(dt.files || []).forEach(f => out.push({ file: f, relPath: f.name })); return out; }
    const readAll = (rd) => new Promise((res, rej) => { const all = []; const step = () => rd.readEntries(b => { if (!b.length) res(all); else { all.push(...b); step(); } }, rej); step(); });
    const walk = async (entry, prefix) => {
      if (entry.isFile) { const file = await new Promise((res, rej) => entry.file(res, rej)); out.push({ file, relPath: prefix + entry.name }); }
      else if (entry.isDirectory) { for (const c of await readAll(entry.createReader())) await walk(c, prefix + entry.name + '/'); }
    };
    for (const e of entries) await walk(e, ''); return out;
  }
  ['dragenter', 'dragover'].forEach(ev => dropZone.addEventListener(ev, e => { e.preventDefault(); dropZone.style.borderColor = 'var(--accent, #6cf)'; }));
  ['dragleave', 'drop'].forEach(ev => dropZone.addEventListener(ev, e => { e.preventDefault(); dropZone.style.borderColor = 'rgba(255,255,255,0.2)'; }));
  dropZone.addEventListener('drop', async e => queueSelection(await collectDropped(e.dataTransfer)));
  dropZone.addEventListener('click', e => { if (e.target === pickDir) return; filesIn.click(); });
  pickDir.addEventListener('click', e => { e.preventDefault(); e.stopPropagation(); dirIn.click(); });
  filesIn.onchange = () => queueSelection(Array.from(filesIn.files || []).map(f => ({ file: f, relPath: f.name })));
  dirIn.onchange = () => queueSelection(Array.from(dirIn.files || []).map(f => ({ file: f, relPath: f.webkitRelativePath || f.name })));
  uploadBtn.onclick = () => { if (!pending) { toast('Choose files or a folder first.', 'error'); return; } const g = pending; pending = null; uploadGroups(g); };

  // ---------------- orientation toggle (horizontal /gifs vs vertical /gifs_tate) ----------------
  if (orientTabs) orientTabs.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-orientation]');
    if (!btn || busy) return;
    const next = btn.dataset.orientation === 'tate' ? 'tate' : 'yoko';
    if (next === orientation) return;
    orientation = next;
    orientTabs.querySelectorAll('[data-orientation]').forEach(b => {
      const on = b.dataset.orientation === orientation;
      b.classList.toggle('active', on);
      b.setAttribute('aria-selected', on ? 'true' : 'false');
    });
    // the two libraries are independent: drop any selection/queue belonging to the other one
    selected = null; files = []; pending = null; uploadBtn.disabled = true;
    progress.textContent = '';
    if (previewCtl) { previewCtl.abort(); previewCtl = null; }
    selectFolder(null);
    await loadLibrary();
  });

  loadLibrary();
}
