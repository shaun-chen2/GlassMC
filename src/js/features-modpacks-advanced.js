/* =====================================================================
 * features-modpacks-advanced.js —— 整合包进阶
 *
 * 三块内容：短码工具 / 差异报告 / 应用增量。
 * 只做界面与流程编排：生成、解码、比对、备份都在后端；
 * 导入安装复用现有 modrinth_install，短码安装不重复实现下载器。
 * DOM 只在 GM.register 回调里创建，语言切换由 i18n.onChange 兜底。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = (window.GM = window.GM || {});
  const t = (key, vars) => GM.t('packplus.' + key, vars);
  const T = (key, vars) => GM.esc(t(key, vars));
  const invoke = (cmd, args) => GM.invoke(cmd, args || {});

  let packs = [];
  let currentId = '';
  let lastPreview = null;
  let diffReport = null;
  let lastResults = null;

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function fail(e) {
    GM.toast(t('fail', { err: brief(e) }), 'err');
  }

  function needPack() {
    if (!currentId) {
      GM.toast(t('err.nopack'), 'err');
      return false;
    }
    return true;
  }

  function codeInput() {
    const el = document.getElementById('pp-code-in');
    return ((el && el.value) || '').trim();
  }

  // ---------------------------------------------------------------- 页面骨架

  function pageHtml() {
    return `
      <div class="page-head">
        <h1 data-i18n="packplus.title">${T('title')}</h1>
        <div class="sub" data-i18n="packplus.sub">${T('sub')}</div>
      </div>

      <div class="card tight" style="margin-bottom:12px">
        <div class="field">
          <div class="lbl" data-i18n="packplus.pack.label">${T('pack.label')}</div>
          <select id="pp-pack" data-i18n-title="packplus.pack.label"></select>
          <div class="mini-note" id="pp-pack-hint" style="margin-top:6px"></div>
        </div>
      </div>

      <div class="card section-gap">
        <div class="card-title" data-i18n="packplus.code.title">${T('code.title')}</div>
        <div class="sub" style="margin-bottom:10px" data-i18n="packplus.code.sub">${T('code.sub')}</div>
        <div class="stack" style="gap:8px">
          <div class="hstack" style="gap:8px;flex-wrap:wrap">
            <button class="btn primary sm" data-pp="gen" data-i18n="packplus.code.gen">${T('code.gen')}</button>
            <button class="btn sm" data-pp="copy" data-i18n="packplus.code.copy">${T('code.copy')}</button>
          </div>
          <textarea id="pp-code-out" class="mono" rows="3" readonly placeholder="${T('code.out.ph')}" data-i18n-placeholder="packplus.code.out.ph"></textarea>
          <textarea id="pp-code-in" class="mono" rows="3" placeholder="${T('code.in.ph')}" data-i18n-placeholder="packplus.code.in.ph"></textarea>
          <div class="hstack" style="gap:8px;flex-wrap:wrap">
            <button class="btn sm" data-pp="preview" data-i18n="packplus.code.preview">${T('code.preview')}</button>
            <button class="btn sm" data-pp="install" data-i18n="packplus.code.install">${T('code.install')}</button>
            <input id="pp-target" type="text" style="max-width:240px" placeholder="${T('code.target.ph')}" data-i18n-placeholder="packplus.code.target.ph" data-i18n-title="packplus.code.target" title="${T('code.target')}">
          </div>
        </div>
        <div id="pp-preview" style="margin-top:12px"></div>
      </div>

      <div class="card section-gap">
        <div class="hstack" style="justify-content:space-between;flex-wrap:wrap;gap:8px">
          <div>
            <div class="card-title" data-i18n="packplus.diff.title">${T('diff.title')}</div>
            <div class="sub" data-i18n="packplus.diff.sub">${T('diff.sub')}</div>
          </div>
          <button class="btn sm" data-pp="scan" data-i18n="packplus.diff.scan">${T('diff.scan')}</button>
        </div>
        <div id="pp-diff" style="margin-top:10px"></div>
      </div>

      <div class="card section-gap">
        <div class="card-title" data-i18n="packplus.apply.title">${T('apply.title')}</div>
        <div class="sub" style="margin-bottom:10px" data-i18n="packplus.apply.sub">${T('apply.sub')}</div>
        <div class="hstack" style="gap:10px;flex-wrap:wrap">
          <button class="btn primary sm" data-pp="apply" id="pp-apply-btn">${T('apply.go', { n: 0 })}</button>
          <span class="mono-dim" id="pp-apply-count"></span>
        </div>
        <div id="pp-apply-out" style="margin-top:10px"></div>
      </div>
    `;
  }

  // ---------------------------------------------------------------- 渲染

  async function refreshPacks() {
    try {
      packs = (await invoke('modpack_list')) || [];
    } catch (e) {
      packs = [];
      GM.toast(t('fail', { err: brief(e) }), 'err');
    }
    const sel = document.getElementById('pp-pack');
    if (!sel) return;
    const options = ['<option value="">' + T('pack.ph') + '</option>'].concat(
      packs.map(
        (p) =>
          `<option value="${GM.esc(p.instanceId || '')}">${GM.esc(p.name || p.instanceId || '')} · ${GM.esc(p.gameVersion || '')} ${GM.esc(p.loader || '')}</option>`,
      ),
    );
    sel.innerHTML = options.join('');
    if (!packs.some((p) => p.instanceId === currentId)) {
      currentId = packs.length ? packs[0].instanceId : '';
    }
    sel.value = currentId;

    const hint = document.getElementById('pp-pack-hint');
    if (hint) hint.textContent = packs.length ? '' : t('pack.empty');
  }

  function groupRows(group, rows, chosen) {
    if (!rows || !rows.length) return '';
    const label = T('diff.grp.' + group);
    const items = rows
      .map((f, i) => {
        const key = group + ':' + i;
        const on = chosen && chosen.has(f.path) ? ' checked' : '';
        const size = Number(f.size || 0);
        const kb = size >= 1024 ? (size / 1024).toFixed(1) + ' KB' : size + ' B';
        return `<label class="hstack" style="gap:8px;font-size:12.5px">
          <input type="checkbox" data-pp-item="${key}"${on}>
          <span class="mono" style="word-break:break-all;flex:1">${GM.esc(f.path)}</span>
          <span class="mono-dim">${kb}</span>
        </label>`;
      })
      .join('');
    return `<div class="stack" style="gap:4px;margin-top:8px">
      <div class="mini-note">${label}（${rows.length}）</div>${items}</div>`;
  }

  function renderPreview(pv, extra) {
    const box = document.getElementById('pp-preview');
    if (!box) return;
    if (!pv) {
      box.innerHTML = '';
      return;
    }
    const mods = pv.mods || [];
    const shown = mods.slice(0, 20);
    const list = shown.length
      ? shown
          .map(
            (m) =>
              `<div class="mono-dim" style="font-size:12px;word-break:break-all">${GM.esc(m.projectId)} → ${GM.esc(m.versionId)}</div>`,
          )
          .join('')
      : `<div class="empty">${T('preview.none')}</div>`;
    const more = mods.length > shown.length ? `<div class="mini-note">${T('preview.more', { n: mods.length - shown.length })}</div>` : '';
    const note = pv.truncated
      ? `<div class="badge-pill warn" style="margin-top:6px">${T('preview.note')}：${GM.esc(pv.note || '')}</div>`
      : '';
    box.innerHTML = `<div class="card tight">
      <div class="card-title">${T('preview.title')}<span class="k">${pv.codeLength || 0}</span></div>
      <div class="stack" style="gap:4px;margin-top:6px;font-size:12.5px">
        <div>${T('preview.name')}：<b>${GM.esc(pv.name)}</b></div>
        <div>${T('preview.game')}：${GM.esc(pv.game || '—')} · ${T('preview.loader')}：${GM.esc(pv.loader || '—')}</div>
        <div>${T('preview.mods')}：${pv.modCount}</div>
        ${note}${extra || ''}
        <div class="mini-note" style="margin-top:6px">${T('preview.modhead')}</div>
        ${list}${more}
      </div>
    </div>`;
  }

  function renderDiff(r, chosen) {
    const box = document.getElementById('pp-diff');
    if (!box) return;
    if (!r) {
      box.innerHTML = '';
      updateApplyCount();
      return;
    }
    if (r.baselineCreated) {
      box.innerHTML = `<div class="badge-pill ok">${T('diff.baseline', { n: (r.unchanged || []).length })}</div>`;
      updateApplyCount();
      return;
    }
    const total = (r.added || []).length + (r.changed || []).length + (r.removed || []).length;
    if (!total) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${T('diff.empty')}</span></div>`;
      updateApplyCount();
      return;
    }
    box.innerHTML =
      `<div class="badge-pill">${T('diff.summary', {
        a: (r.added || []).length,
        c: (r.changed || []).length,
        r: (r.removed || []).length,
        u: (r.unchanged || []).length,
      })}</div>` +
      groupRows('added', r.added, chosen) +
      groupRows('changed', r.changed, chosen) +
      groupRows('removed', r.removed, chosen);
    updateApplyCount();
  }

  function renderApplyResults(results) {
    const box = document.getElementById('pp-apply-out');
    if (!box) return;
    if (!results || !results.length) {
      box.innerHTML = '';
      return;
    }
    box.innerHTML = `<div class="stack" style="gap:3px;font-size:12px">${results
      .map(
        (x) =>
          `<div class="hstack" style="gap:8px"><span class="badge-pill ${x.ok ? 'ok' : 'warn'}">${x.ok ? 'OK' : '!'}</span>` +
          `<span class="mono" style="word-break:break-all;flex:1">${GM.esc(x.path)}</span>` +
          `<span class="mono-dim">${GM.esc(x.message)}</span></div>`,
      )
      .join('')}</div>`;
  }

  function updateApplyCount() {
    const btn = document.getElementById('pp-apply-btn');
    const cnt = document.getElementById('pp-apply-count');
    const n = document.querySelectorAll('#pp-diff [data-pp-item]:checked').length;
    if (btn) btn.textContent = t('apply.go', { n });
    if (cnt) cnt.textContent = n ? t('diff.pick') : '';
  }

  // ---------------------------------------------------------------- 操作

  async function doGen() {
    if (!needPack()) return;
    try {
      const code = await invoke('pack_share_code', { packId: currentId });
      const out = document.getElementById('pp-code-out');
      if (out) out.value = code;
    } catch (e) {
      fail(e);
    }
  }

  async function doCopy() {
    const out = document.getElementById('pp-code-out');
    const text = ((out && out.value) || '').trim();
    if (!text) {
      GM.toast(t('err.nocode'), 'err');
      return;
    }
    let ok = false;
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
        ok = true;
      }
    } catch (e) {
      ok = false;
    }
    if (!ok && out) {
      try {
        out.select();
        ok = document.execCommand('copy');
      } catch (e) {
        ok = false;
      }
    }
    GM.toast(ok ? t('code.copied') : t('fail', { err: 'clipboard' }), ok ? 'ok' : 'err');
  }

  async function doPreview() {
    const code = codeInput();
    if (!code) {
      GM.toast(t('err.nocode'), 'err');
      return;
    }
    try {
      lastPreview = await invoke('pack_preview_code', { code });
      renderPreview(lastPreview);
    } catch (e) {
      lastPreview = null;
      renderPreview(null);
      fail(e);
    }
  }

  async function doInstall() {
    const code = codeInput();
    if (!code) {
      GM.toast(t('err.nocode'), 'err');
      return;
    }
    const targetEl = document.getElementById('pp-target');
    const target = ((targetEl && targetEl.value) || '').trim() || currentId;
    if (!target) {
      GM.toast(t('err.notarget'), 'err');
      return;
    }
    GM.busy(true, t('busy'));
    try {
      const manifest = await invoke('pack_install_code', { code });
      let ok = 0;
      let bad = 0;
      for (let i = 0; i < manifest.items.length; i++) {
        const item = manifest.items[i];
        GM.busy(true, t('install.running', { i: i + 1, n: manifest.items.length, id: item.projectId }));
        try {
          await invoke('modrinth_install', {
            versionId: target,
            projectId: item.projectId,
            game: manifest.game,
            loader: manifest.loader,
            mrVersion: item.versionId,
            includeOptional: false,
          });
          ok++;
        } catch (e) {
          bad++;
        }
      }
      GM.busy(false);
      const extra = `<div class="mini-note" style="margin-top:6px">${T('install.saved', { id: manifest.id })} · ${T('install.done', { ok, fail: bad })}</div>`;
      renderPreview(lastPreview || { name: manifest.name, game: manifest.game, loader: manifest.loader, modCount: manifest.items.length, mods: manifest.items.map((x) => ({ projectId: x.projectId, versionId: x.versionId })), truncated: manifest.truncated, note: manifest.note, codeLength: code.length }, extra);
      GM.toast(t('install.done', { ok, fail: bad }), bad ? 'warn' : 'ok');
    } catch (e) {
      GM.busy(false);
      fail(e);
    }
  }

  async function doScan() {
    if (!needPack()) return;
    GM.busy(true, t('busy'));
    try {
      diffReport = await invoke('pack_diff', { packId: currentId });
      renderDiff(diffReport);
      GM.toast(
        diffReport.baselineCreated
          ? t('diff.baseline', { n: (diffReport.unchanged || []).length })
          : t('diff.summary', { a: diffReport.added.length, c: diffReport.changed.length, r: diffReport.removed.length, u: diffReport.unchanged.length }),
        'ok',
      );
    } catch (e) {
      fail(e);
    } finally {
      GM.busy(false);
    }
  }

  function collectSelected() {
    const picks = [];
    if (!diffReport) return picks;
    document.querySelectorAll('#pp-diff [data-pp-item]:checked').forEach((cb) => {
      const parts = (cb.dataset.ppItem || '').split(':');
      const group = parts[0];
      const idx = Number(parts[1]);
      const rows = group === 'added' ? diffReport.added : group === 'changed' ? diffReport.changed : group === 'removed' ? diffReport.removed : [];
      const f = rows && rows[idx];
      if (f) {
        picks.push({
          action: group,
          path: f.path,
          size: f.size || 0,
          sha256: f.sha256 || '',
          downloads: f.downloads || [],
        });
      }
    });
    return picks;
  }

  async function doApply() {
    if (!needPack()) return;
    const picks = collectSelected();
    if (!picks.length) {
      GM.toast(t('apply.none'), 'err');
      return;
    }
    GM.busy(true, t('busy'));
    try {
      lastResults = await invoke('pack_apply_diff', { packId: currentId, items: picks });
      renderApplyResults(lastResults);
      const ok = lastResults.filter((x) => x.ok).length;
      GM.toast(t('apply.done', { ok, fail: lastResults.length - ok }), ok === lastResults.length ? 'ok' : 'warn');
      await doScan();
    } catch (e) {
      fail(e);
    } finally {
      GM.busy(false);
    }
  }

  // ---------------------------------------------------------------- 事件

  function bind(page) {
    if (!page || page.dataset.ppBound) return;
    page.dataset.ppBound = '1';

    page.addEventListener('change', (e) => {
      const target = e.target;
      if (!target) return;
      if (target.id === 'pp-pack') {
        currentId = target.value || '';
        return;
      }
      if (target.matches && target.matches('[data-pp-item]')) updateApplyCount();
    });

    page.addEventListener('click', (e) => {
      const btn = e.target.closest && e.target.closest('[data-pp]');
      if (!btn) return;
      const act = btn.dataset.pp;
      if (act === 'gen') doGen();
      else if (act === 'copy') doCopy();
      else if (act === 'preview') doPreview();
      else if (act === 'install') doInstall();
      else if (act === 'scan') doScan();
      else if (act === 'apply') doApply();
    });
  }

  function mount() {
    GM.addNav({
      id: 'packplus',
      label: 'nav.packplus',
      title: t('title'),
      icon: '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 7h7l2 2h7v9H4z"/><path d="M9 13h6M12 10v6"/></svg>',
    });
    const page = GM.addPage('packplus', pageHtml());
    bind(page);
    refreshPacks();
  }

  GM.register({
    ready: mount,
    nav: (p) => {
      if (p === 'packplus') refreshPacks();
    },
    render: () => {
      if (document.querySelector('.page[data-page="packplus"]')) refreshPacks();
    },
  });

  if (GM.i18n && GM.i18n.onChange) {
    GM.i18n.onChange(() => {
      const chosen = new Set(
        Array.from(document.querySelectorAll('#pp-diff [data-pp-item]:checked')).map((cb) => {
          const parts = (cb.dataset.ppItem || '').split(':');
          const rows = parts[0] === 'added' ? diffReport && diffReport.added : parts[0] === 'changed' ? diffReport && diffReport.changed : diffReport && diffReport.removed;
          const f = rows && rows[Number(parts[1])];
          return f ? f.path : '';
        }),
      );
      if (lastPreview) renderPreview(lastPreview);
      if (diffReport) renderDiff(diffReport, chosen);
      if (lastResults) renderApplyResults(lastResults);
    });
  }
})();
