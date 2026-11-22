/* =====================================================================
 * features-saves.js —— 存档与数据
 *
 * 五个分区：世界（种子/估算时长）、备份、截图画廊、数据包、投影文件。
 * 后端命令都在 saves.rs；本文件只在 ready 之后碰 DOM，数据缓存在模块内，
 * render 钩子用缓存重画，语言切换不会重复请求后端。
 * ===================================================================== */
(function () {
  'use strict';

  const GM = window.GM;
  if (!GM || typeof GM.register !== 'function') return;

  const t = (k, v) => GM.t('saves.' + k, v);
  const esc = (s) => (GM.ui && GM.ui.escAttr ? GM.ui.escAttr(s) : String(s == null ? '' : s));

  const cache = {
    worlds: [],
    backups: [],
    shots: [],
    schematics: [],
    shotThumbs: new Map(),
    loaded: false,
  };
  let versionId = '';
  let invoking = null;

  function invoke(cmd, args) {
    return (invoking || (invoking = GM.invoke))(cmd, args || {});
  }

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function fmtSize(n) {
    n = Number(n) || 0;
    if (n < 1024) return n + ' B';
    if (n < 1024 * 1024) return (n / 1024).toFixed(1) + ' KB';
    if (n < 1024 * 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + ' MB';
    return (n / 1024 / 1024 / 1024).toFixed(2) + ' GB';
  }

  function fmtTime(ms) {
    if (!ms) return '—';
    try {
      return new Date(Number(ms)).toLocaleString();
    } catch (_) {
      return '—';
    }
  }

  function fmtSecs(s) {
    return fmtTime(Number(s) * 1000);
  }

  function installed() {
    const S = GM.state();
    return (S && S.installed) || [];
  }

  function currentVersion() {
    const list = installed();
    if (versionId && list.some((v) => v.id === versionId)) return versionId;
    return list.length ? list[0].id : '';
  }

  function empty(text) {
    return `<div class="empty"><span class="big-ico">◌</span><span>${esc(text)}</span></div>`;
  }

  // ---------------------------------------------------------------- 页面骨架

  function pageHtml() {
    return `
      <div class="page-head">
        <h1>${esc(t('title'))}</h1>
        <div class="sub">${esc(t('sub'))}</div>
        <div class="spacer"></div>
        <select id="saves-version" style="min-width:170px"></select>
        <button class="btn sm" id="saves-refresh">${esc(t('refresh'))}</button>
      </div>

      <div class="card section-gap">
        <div class="card-title"><span>${esc(t('worlds.title'))}</span><span class="k" id="saves-world-count">—</span></div>
        <div id="saves-worlds">${empty(t('loading'))}</div>
        <div class="mini-note" style="margin-top:8px">${esc(t('worlds.hint'))}</div>
      </div>

      <div class="card section-gap">
        <div class="card-title"><span>${esc(t('backups.title'))}</span><span class="k" id="saves-backup-count">—</span></div>
        <div id="saves-backups">${empty(t('loading'))}</div>
      </div>

      <div class="card section-gap">
        <div class="card-title"><span>${esc(t('shots.title'))}</span><span class="k" id="saves-shot-count">—</span></div>
        <div id="saves-shots">${empty(t('loading'))}</div>
      </div>

      <div class="card section-gap">
        <div class="card-title">
          <span>${esc(t('schematics.title'))}</span>
          <span class="k" id="saves-sch-count">—</span>
          <button class="btn sm" data-saves-schimport style="margin-left:8px">${esc(t('schematics.import'))}</button>
        </div>
        <div id="saves-schematics">${empty(t('loading'))}</div>
      </div>
    `;
  }

  function ensurePage() {
    if (!document.querySelector('.page[data-page="saves"]')) GM.addPage('saves', pageHtml());
  }

  function setCount(id, n) {
    const el = document.getElementById(id);
    if (el) el.textContent = String(n);
  }

  function paintVersion() {
    const sel = document.getElementById('saves-version');
    if (!sel) return;
    const list = installed();
    sel.innerHTML = list.length
      ? list
          .map(
            (v) =>
              `<option value="${esc(v.id)}"${v.id === versionId ? ' selected' : ''}>${esc(v.id)}</option>`,
          )
          .join('')
      : `<option value="">${esc(t('noVersion'))}</option>`;
  }

  // ---------------------------------------------------------------- 加载

  async function loadAll() {
    const id = currentVersion();
    versionId = id;
    paintVersion();
    if (!id) {
      cache.worlds = [];
      cache.backups = [];
      cache.shots = [];
      cache.schematics = [];
      cache.shotThumbs.clear();
      cache.loaded = true;
      paint();
      return;
    }
    GM.busy(true, t('loading'));
    try {
      const [worlds, backups, shots, schematics] = await Promise.all([
        invoke('saves_list', { versionId: id }),
        invoke('saves_list_backups', { versionId: id }),
        invoke('screenshots_list', { versionId: id }),
        invoke('schematic_list', { versionId: id }),
      ]);
      cache.worlds = worlds || [];
      cache.backups = backups || [];
      cache.shots = shots || [];
      cache.schematics = schematics || [];
    } catch (e) {
      cache.worlds = [];
      cache.backups = [];
      cache.shots = [];
      cache.schematics = [];
      GM.toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      GM.busy(false);
      cache.loaded = true;
      paint();
    }
  }

  /** 单个写操作：busy + toast，失败返回 null。 */
  async function run(cmd, args, okMsg) {
    GM.busy(true, t('working'));
    try {
      const r = await invoke(cmd, args);
      if (okMsg) GM.toast(okMsg, 'ok');
      return r;
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
      return null;
    } finally {
      GM.busy(false);
    }
  }

  // ---------------------------------------------------------------- 世界表格

  function worldsHtml() {
    setCount('saves-world-count', cache.worlds.length);
    if (!cache.worlds.length) return empty(t('worlds.empty'));
    const th = (key) =>
      `<th style="padding:6px 8px;text-align:left;font-weight:500;white-space:nowrap">${esc(key)}</th>`;
    return `<div style="overflow-x:auto"><table style="width:100%;border-collapse:collapse;font-size:12.5px">
      <thead><tr style="color:var(--text-dim);border-bottom:1px solid var(--card-border)">
        ${th(t('worlds.colName'))}${th(t('worlds.colSeed'))}${th(t('worlds.colPlay'))}
        ${th(t('worlds.colLast'))}${th(t('worlds.colActions'))}
      </tr></thead>
      <tbody>${cache.worlds.map(worldRow).join('')}</tbody>
    </table></div>`;
  }

  function worldRow(w) {
    const seed = w.seed
      ? `<span class="mono-dim">${esc(w.seed)}</span>`
      : `<span class="badge-pill warn">${esc(t('unknown'))}</span>`;
    const play = w.playHours > 0 ? esc(t('worlds.hours', { hours: w.playHours })) : '—';
    const extras = [];
    extras.push(fmtSize(w.sizeBytes));
    if (w.hasIcon) extras.push(t('worlds.icon'));
    if (w.hasScreenshots) extras.push(t('worlds.shot'));
    return `<tr style="border-bottom:1px solid var(--card-border)">
      <td style="padding:8px;min-width:140px">
        <div class="v-id" style="font-size:12.5px">${esc(w.name)}</div>
        <div class="mini-note">${esc(extras.join(' · '))}</div>
      </td>
      <td style="padding:8px">${seed}</td>
      <td style="padding:8px;white-space:nowrap">${play}</td>
      <td style="padding:8px;white-space:nowrap" class="mono-dim">${esc(fmtTime(w.lastPlayed))}</td>
      <td style="padding:8px;white-space:nowrap">
        <button class="btn sm" data-saves-backup="${esc(w.id)}">${esc(t('worlds.backup'))}</button>
        <button class="btn sm" data-saves-packs="${esc(w.id)}">${esc(t('worlds.packs'))}</button>
        <button class="btn sm" data-saves-locate="${esc(w.path)}">${esc(t('locate'))}</button>
      </td>
    </tr>`;
  }

  // ---------------------------------------------------------------- 备份

  function backupsHtml() {
    setCount('saves-backup-count', cache.backups.length);
    if (!cache.backups.length) return empty(t('backups.empty'));
    return `<div class="row-list">${cache.backups
      .map(
        (b) => `<div class="vrow">
          <div class="v-main">
            <div class="v-id">${esc(b.world)}${b.label ? ' · ' + esc(b.label) : ''}</div>
            <div class="v-meta"><span class="mono-dim">${esc(fmtSecs(b.created))}</span> · ${esc(
              t('backups.files', { n: b.files }),
            )}</div>
          </div>
          <div class="v-actions">
            <button class="btn sm primary" data-saves-restore="${esc(b.id)}">${esc(
              t('backups.restore'),
            )}</button>
            <button class="btn sm danger" data-saves-delbackup="${esc(b.id)}">${esc(
              t('delete'),
            )}</button>
          </div>
        </div>`,
      )
      .join('')}</div>`;
  }

  // ---------------------------------------------------------------- 截图

  function shotsHtml() {
    setCount('saves-shot-count', cache.shots.length);
    if (!cache.shots.length) return empty(t('shots.empty'));
    cache.shotThumbs.clear();
    const cards = cache.shots.map((s, i) => {
      cache.shotThumbs.set(i, s.thumb || '');
      const pic = s.thumb
        ? `<img src="${s.thumb}" alt="" style="width:100%;height:88px;object-fit:cover;border-radius:8px">`
        : `<div style="width:100%;height:88px;display:grid;place-items:center;border-radius:8px;color:var(--text-dim);font-size:12px">${esc(
            t('shots.noThumb'),
          )}</div>`;
      return `<div class="card tight" style="padding:6px">
        <div data-saves-zoom="${i}" style="cursor:zoom-in" title="${esc(t('shots.zoom'))}">${pic}</div>
        <div class="mini-note" style="margin-top:5px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${esc(
          s.name,
        )}">${esc(s.name)}</div>
        <div class="mini-note mono-dim">${esc(fmtSize(s.size))} · ${esc(fmtSecs(s.modified))}</div>
        ${
          s.tooLarge
            ? `<div class="mini-note" style="color:var(--warn)">${esc(t('shots.tooLarge'))}</div>`
            : ''
        }
        <div class="hstack" style="gap:4px;margin-top:5px">
          <button class="btn sm" data-saves-shotdir="${esc(s.path)}">${esc(t('openDir'))}</button>
          <button class="btn sm danger" data-saves-delshot="${esc(s.path)}">${esc(t('delete'))}</button>
        </div>
      </div>`;
    });
    return `<div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:10px">${cards.join(
      '',
    )}</div>`;
  }

  // ---------------------------------------------------------------- 投影

  function schematicsHtml() {
    setCount('saves-sch-count', cache.schematics.length);
    if (!cache.schematics.length) return empty(t('schematics.empty'));
    return `<div class="row-list">${cache.schematics
      .map(
        (s) => `<div class="vrow">
          <div class="v-main">
            <div class="v-id">${esc(s.name)}</div>
            <div class="v-meta"><span class="badge-pill accent">${esc(s.format)}</span> · ${esc(
              fmtSize(s.size),
            )} · <span class="mono-dim">${esc(fmtSecs(s.modified))}</span></div>
          </div>
          <div class="v-actions">
            <button class="btn sm" data-saves-schdir="${esc(s.path)}">${esc(t('openDir'))}</button>
          </div>
        </div>`,
      )
      .join('')}</div>`;
  }

  function paint() {
    ensurePage();
    paintVersion();
    const worlds = document.getElementById('saves-worlds');
    const backups = document.getElementById('saves-backups');
    const shots = document.getElementById('saves-shots');
    const schs = document.getElementById('saves-schematics');
    if (worlds) worlds.innerHTML = cache.loaded ? worldsHtml() : empty(t('loading'));
    if (backups) backups.innerHTML = cache.loaded ? backupsHtml() : empty(t('loading'));
    if (shots) shots.innerHTML = cache.loaded ? shotsHtml() : empty(t('loading'));
    if (schs) schs.innerHTML = cache.loaded ? schematicsHtml() : empty(t('loading'));
  }

  // ---------------------------------------------------------------- 交互

  async function openDir(path) {
    try {
      await invoke('open_dir', { path });
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  function openBackupSheet(saveId) {
    GM.ui.openSheet(
      GM.ui.shell(
        esc(t('worlds.backupTitle', { id: saveId })),
        esc(t('worlds.backupSub')),
        `<div class="field">
          <div class="lbl">${esc(t('worlds.backupLabel'))}</div>
          <input type="text" id="saves-bk-label" maxlength="60" placeholder="${esc(t('worlds.backupLabel'))}">
        </div>`,
        `<button class="btn primary" id="saves-bk-go">${esc(t('worlds.backup'))}</button>
         <button class="btn" data-sheet-close>${esc(t('close'))}</button>`,
      ),
    );
    const go = document.getElementById('saves-bk-go');
    if (go)
      go.onclick = async () => {
        const el = document.getElementById('saves-bk-label');
        const label = ((el && el.value) || '').trim();
        GM.ui.closeSheet();
        const r = await run(
          'saves_backup',
          { versionId: currentVersion(), saveId, label },
          t('worlds.backupDone'),
        );
        if (r) loadAll();
      };
  }

  function packsRowHtml(p) {
    return `<div class="vrow">
      <div class="v-main">
        <div class="v-id">${esc(p.name)}</div>
        <div class="v-meta">
          <span class="badge-pill ${p.enabled ? 'ok' : 'warn'}">${esc(
            p.enabled ? t('packs.enabled') : t('packs.disabled'),
          )}</span>
          · ${esc(p.isDir ? t('packs.folder') : fmtSize(p.size))}
        </div>
      </div>
      <div class="v-actions">
        <button class="btn sm" data-saves-ptoggle="${esc(p.name)}" data-on="${p.enabled ? '1' : ''}">${esc(
          p.enabled ? t('packs.disable') : t('packs.enable'),
        )}</button>
      </div>
    </div>`;
  }

  async function openPacksSheet(saveId) {
    let packs = [];
    try {
      packs = (await invoke('saves_datapacks', { versionId: currentVersion(), saveId })) || [];
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    }
    const body =
      (packs.length ? `<div class="row-list">${packs.map(packsRowHtml).join('')}</div>` : empty(t('packs.empty'))) +
      `<div class="mini-note" style="margin-top:8px">${esc(t('packs.hint'))}</div>`;
    GM.ui.openSheet(
      GM.ui.shell(
        esc(t('packs.title', { id: saveId })),
        '',
        body,
        `<button class="btn" id="saves-pack-add">${esc(t('packs.add'))}</button>
         <button class="btn" data-sheet-close>${esc(t('close'))}</button>`,
      ),
    );
    const sheet = document.getElementById('sheet');
    if (sheet) {
      sheet.querySelectorAll('[data-saves-ptoggle]').forEach((btn) => {
        btn.onclick = async () => {
          const name = btn.getAttribute('data-saves-ptoggle');
          const toEnable = !btn.getAttribute('data-on');
          const r = await run(
            'saves_toggle_datapack',
            { versionId: currentVersion(), saveId, name, enabled: toEnable },
            t('packs.toggleDone'),
          );
          if (r) openPacksSheet(saveId);
        };
      });
    }
    const add = document.getElementById('saves-pack-add');
    if (add)
      add.onclick = async () => {
        const path = await invoke('pick_file', { kind: 'any' });
        if (!path) return;
        const r = await run(
          'saves_add_datapack',
          { versionId: currentVersion(), saveId, path },
          t('packs.addDone'),
        );
        if (r) openPacksSheet(saveId);
      };
  }

  function openZoom(i) {
    const src = cache.shotThumbs.get(i);
    const shot = cache.shots[i];
    if (!shot) return;
    if (!src) {
      openDir(shot.path);
      return;
    }
    GM.ui.openSheet(
      GM.ui.shell(
        esc(shot.name),
        '',
        `<div style="text-align:center"><img src="${src}" style="max-width:100%;max-height:70vh;border-radius:10px"></div>`,
        `<button class="btn" data-sheet-close>${esc(t('close'))}</button>`,
      ),
    );
  }

  function bind() {
    const page = document.querySelector('.page[data-page="saves"]');
    if (!page || page.dataset.savesBound) return;
    page.dataset.savesBound = '1';

    page.addEventListener('click', (e) => {
      const hit = (sel) => e.target.closest(sel);
      let el;

      if ((el = hit('[data-saves-locate]'))) return openDir(el.getAttribute('data-saves-locate'));
      if ((el = hit('[data-saves-schdir]'))) return openDir(el.getAttribute('data-saves-schdir'));
      if ((el = hit('[data-saves-shotdir]'))) return openDir(el.getAttribute('data-saves-shotdir'));
      if ((el = hit('[data-saves-backup]'))) return openBackupSheet(el.getAttribute('data-saves-backup'));
      if ((el = hit('[data-saves-packs]'))) return openPacksSheet(el.getAttribute('data-saves-packs'));
      if ((el = hit('[data-saves-zoom]'))) return openZoom(Number(el.getAttribute('data-saves-zoom')));
      if ((el = hit('#saves-refresh'))) return loadAll();
      if ((el = hit('[data-saves-schimport]'))) return importSchematic();
      if ((el = hit('[data-saves-restore]'))) {
        const id = el.getAttribute('data-saves-restore');
        return GM.ui.confirm(t('backups.restoreTitle'), t('backups.restoreSub'), async () => {
          if (await run('saves_restore', { versionId: currentVersion(), backupId: id }, t('backups.restoreDone')))
            loadAll();
        });
      }
      if ((el = hit('[data-saves-delbackup]'))) {
        const id = el.getAttribute('data-saves-delbackup');
        return GM.ui.confirm(t('backups.delTitle'), t('backups.delSub'), async () => {
          if (await run('saves_delete_backup', { backupId: id }, t('backups.delDone'))) loadAll();
        });
      }
      if ((el = hit('[data-saves-delshot]'))) {
        const path = el.getAttribute('data-saves-delshot');
        return GM.ui.confirm(t('shots.delTitle'), t('shots.delSub'), async () => {
          if (await run('screenshots_delete', { path }, t('shots.delDone'))) loadAll();
        });
      }
    });

    const sel = document.getElementById('saves-version');
    if (sel && !sel.dataset.savesBound) {
      sel.dataset.savesBound = '1';
      sel.addEventListener('change', () => {
        versionId = sel.value;
        loadAll();
      });
    }
  }

  async function importSchematic() {
    const path = await invoke('pick_file', { kind: 'any' });
    if (!path) return;
    const r = await run(
      'schematic_import',
      { versionId: currentVersion(), path },
      t('schematics.importDone'),
    );
    if (r) loadAll();
  }

  // ---------------------------------------------------------------- 接入

  GM.register({
    ready() {
      GM.addNav({
        id: 'saves',
        label: 'nav.saves',
        icon:
          '<svg class="ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"><path d="M3 2.2h8.2L13.8 4.8v9H3z"/><path d="M5.3 2.4v3.6h5V2.6M5.2 13.6V9.4h5.6v4.2"/></svg>',
      });
      ensurePage();
      bind();
      versionId = currentVersion();
      loadAll();
    },
    render() {
      if (cache.loaded) paint();
    },
    nav(page) {
      if (page !== 'saves') return;
      ensurePage();
      bind();
      if (!cache.loaded) loadAll();
    },
  });
})();
