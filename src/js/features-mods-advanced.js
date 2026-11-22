/* =====================================================================
 * features-mods-advanced.js —— 模组进阶
 *
 * 四块：组合预设 / 冲突报告 / 拖拽导入 / 批量更新。
 * 所有 DOM 操作都在 GM.register 的回调（ready / render / nav）里完成；
 * 所有数据都走后端命令，页面自身不碰文件系统。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = window.GM;
  if (!GM || !GM.register) return;

  const t = (key, vars) => GM.t('modsplus.' + key, vars);
  const esc = (s) => GM.esc(s);

  let presets = [];
  let conflicts = [];
  let candidates = [];
  let importReport = null;
  const selected = Object.create(null);

  // ---------------------------------------------------------------- 工具

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function versionId() {
    const S = GM.state() || {};
    const list = S.installed || [];
    const v = list.find((x) => x.id === S.selected) || list[0];
    return (v && v.id) || S.selected || '';
  }

  function requireVersion() {
    const v = versionId();
    if (!v) GM.toast(t('noVersion'), 'err');
    return !!v;
  }

  function emptyBox(label) {
    return '<div class="empty"><span class="big-ico">◌</span><span>' + esc(label) + '</span></div>';
  }

  function fail(e) {
    GM.toast(t('fail', { err: brief(e) }), 'err');
  }

  // ---------------------------------------------------------------- 页面骨架

  function pageHtml() {
    const icon = '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">' +
      '<path fill="currentColor" d="M4 4h7v7H4V4zm9 0h7v7h-7V4zM4 13h7v7H4v-7zm9 0h7v7h-7v-7zm2 2v3h3v-3h-3z"/></svg>';

    GM.addNav({
      id: 'modsplus',
      label: 'nav.modsplus',
      icon,
      title: t('title'),
    });

    return (
      '<div class="card" style="margin-bottom:12px">' +
      '<div class="card-title"><span data-i18n="modsplus.title">' + esc(t('title')) + '</span>' +
      '<span class="k" id="mp-version">—</span></div>' +
      '<div class="desc" data-i18n="modsplus.subtitle">' + esc(t('subtitle')) + '</div>' +
      '</div>' +

      // 预设
      '<div class="card" style="margin-bottom:12px">' +
      '<div class="card-title"><span data-i18n="modsplus.preset.title">' + esc(t('preset.title')) + '</span>' +
      '<span class="k" id="mp-preset-count">0</span></div>' +
      '<div class="hstack" style="gap:8px;flex-wrap:wrap;margin-bottom:10px">' +
      '<input type="text" id="mp-preset-name" maxlength="48" style="flex:1;min-width:180px" ' +
      'data-i18n-placeholder="modsplus.preset.ph" placeholder="' + esc(t('preset.ph')) + '">' +
      '<button class="btn sm primary" id="mp-preset-save" data-i18n="modsplus.preset.save">' + esc(t('preset.save')) + '</button>' +
      '<button class="btn sm" id="mp-preset-refresh" data-i18n="modsplus.refresh">' + esc(t('refresh')) + '</button>' +
      '</div>' +
      '<div id="mp-presets" class="stack" style="gap:8px"></div>' +
      '</div>' +

      // 冲突报告
      '<div class="card" style="margin-bottom:12px">' +
      '<div class="card-title"><span data-i18n="modsplus.conflict.title">' + esc(t('conflict.title')) + '</span>' +
      '<span class="k" id="mp-conflict-count">—</span></div>' +
      '<div class="hstack" style="gap:8px;flex-wrap:wrap;align-items:center">' +
      '<button class="btn sm" id="mp-conflict-scan" data-i18n="modsplus.conflict.scan">' + esc(t('conflict.scan')) + '</button>' +
      '<span class="desc" data-i18n="modsplus.conflict.sub">' + esc(t('conflict.sub')) + '</span>' +
      '</div>' +
      '<div id="mp-conflicts" class="stack" style="gap:8px;margin-top:10px"></div>' +
      '</div>' +

      // 拖拽导入
      '<div class="card" style="margin-bottom:12px">' +
      '<div class="card-title"><span data-i18n="modsplus.import.title">' + esc(t('import.title')) + '</span>' +
      '<span class="k">.jar / .zip</span></div>' +
      '<div id="mp-drop" role="button" tabindex="0" style="border:1px dashed var(--card-border-hi);' +
      'border-radius:12px;padding:22px;text-align:center;cursor:pointer;background:var(--card-bg)">' +
      '<div style="font-size:13px;font-weight:600" data-i18n="modsplus.import.hint">' + esc(t('import.hint')) + '</div>' +
      '<div class="desc" style="margin-top:4px">.jar / .zip</div>' +
      '</div>' +
      '<div id="mp-import-report" class="stack" style="gap:6px;margin-top:10px"></div>' +
      '</div>' +

      // 批量更新
      '<div class="card">' +
      '<div class="card-title"><span data-i18n="modsplus.update.title">' + esc(t('update.title')) + '</span>' +
      '<span class="k" id="mp-update-count">—</span></div>' +
      '<div class="hstack" style="gap:8px;flex-wrap:wrap;align-items:center;margin-bottom:10px">' +
      '<button class="btn sm" id="mp-update-check" data-i18n="modsplus.update.check">' + esc(t('update.check')) + '</button>' +
      '<label class="hstack" style="gap:6px;font-size:12.5px;color:var(--text-dim)">' +
      '<input type="checkbox" id="mp-update-all"> <span data-i18n="modsplus.update.all">' + esc(t('update.all')) + '</span></label>' +
      '<button class="btn sm primary" id="mp-update-apply" data-i18n="modsplus.update.apply">' + esc(t('update.apply')) + '</button>' +
      '</div>' +
      '<div id="mp-updates" class="stack" style="gap:8px"></div>' +
      '<div class="desc" style="margin-top:8px" data-i18n="modsplus.update.backup">' + esc(t('update.backup')) + '</div>' +
      '</div>'
    );
  }

  // ---------------------------------------------------------------- 绘制

  function paintVersion() {
    const el = document.getElementById('mp-version');
    if (el) el.textContent = versionId() || t('noVersion');
  }

  function paintPresets() {
    const box = document.getElementById('mp-presets');
    if (!box) return;
    const count = document.getElementById('mp-preset-count');
    if (count) count.textContent = String(presets.length);
    if (!versionId()) {
      box.innerHTML = emptyBox(t('noVersion'));
      return;
    }
    if (!presets.length) {
      box.innerHTML = emptyBox(t('preset.empty'));
      return;
    }
    box.innerHTML = presets
      .map((p) => {
        const n = Array.isArray(p.enabled) ? p.enabled.length : 0;
        const when = p.createdAt ? new Date(p.createdAt * 1000).toLocaleString() : '';
        const meta = [esc(p.versionId || ''), n + ' ' + esc(t('unit.mod')), when]
          .filter(Boolean)
          .join(' · ');
        return (
          '<div class="hstack" style="gap:8px;align-items:center;padding:8px 10px;border-radius:10px;background:var(--card-bg)">' +
          '<div style="flex:1;min-width:0">' +
          '<div style="font-size:13px;font-weight:600">' + esc(p.name) + '</div>' +
          '<div class="desc mono-dim" style="font-size:11.5px">' + meta + '</div>' +
          '</div>' +
          '<button class="btn sm primary" data-mp-apply="' + esc(p.id) + '">' + esc(t('preset.apply')) + '</button>' +
          '<button class="btn sm" data-mp-del="' + esc(p.id) + '">' + esc(t('preset.delete')) + '</button>' +
          '</div>'
        );
      })
      .join('');
  }

  function paintConflicts() {
    const box = document.getElementById('mp-conflicts');
    if (!box) return;
    const count = document.getElementById('mp-conflict-count');
    if (count) count.textContent = String(conflicts.length);
    if (!conflicts.length) {
      box.innerHTML = emptyBox(t('conflict.empty'));
      return;
    }
    const cls = { error: 'danger', warning: 'warn', info: '' };
    box.innerHTML = conflicts
      .map((c) => {
        const sev = c.severity === 'error' ? 'error' : c.severity === 'warning' ? 'warning' : 'info';
        const files = (c.files || [])
          .map((f) => '<div class="mono-dim" style="font-size:11.5px">' + esc(f) + '</div>')
          .join('');
        return (
          '<div class="card tight" style="background:var(--card-bg)">' +
          '<div class="hstack" style="gap:8px;align-items:center">' +
          '<span class="badge-pill ' + esc(cls[sev]) + '">' + esc(t('conflict.' + sev)) + '</span>' +
          '<span style="font-size:13px;font-weight:600">' + esc(c.description) + '</span>' +
          '</div>' +
          '<div style="margin-top:6px">' + files + '</div>' +
          '<div class="desc" style="margin-top:6px">' + esc(t('conflict.suggest')) + '：' + esc(c.suggestion) + '</div>' +
          '</div>'
        );
      })
      .join('');
  }

  function paintImport() {
    const box = document.getElementById('mp-import-report');
    if (!box) return;
    if (!importReport) {
      box.innerHTML = '';
      return;
    }
    const line = (label, items) => {
      if (!items || !items.length) return '';
      return (
        '<div class="desc" style="margin-top:4px">' + esc(label) + '：' +
        items
          .map((x) => esc(typeof x === 'string' ? x : x.fileName + (x.reason ? '（' + x.reason + '）' : '')))
          .join('、') + '</div>'
      );
    };
    box.innerHTML =
      '<div class="hstack" style="gap:8px;align-items:center">' +
      '<span class="badge-pill ok">' + esc(t('import.done', { n: importReport.count || 0 })) + '</span>' +
      '</div>' +
      line(t('import.imported'), importReport.imported) +
      line(t('import.skipped'), importReport.skipped) +
      line(t('import.failed'), importReport.failed);
  }

  function paintUpdates() {
    const box = document.getElementById('mp-updates');
    if (!box) return;
    const count = document.getElementById('mp-update-count');
    const good = candidates.filter((c) => c.ok);
    if (count) count.textContent = String(good.length);

    const all = document.getElementById('mp-update-all');
    if (all) {
      all.checked = good.length > 0 && good.every((c) => !!selected[c.fileName]);
      all.indeterminate = !all.checked && good.some((c) => !!selected[c.fileName]);
    }

    if (!candidates.length) {
      box.innerHTML = emptyBox(t('update.empty'));
      return;
    }
    box.innerHTML = candidates
      .map((c) => {
        if (!c.ok) {
          return (
            '<div class="hstack" style="gap:8px;align-items:center;padding:8px 10px;border-radius:10px;background:var(--card-bg)">' +
            '<span class="badge-pill warn">' + esc(t('update.issued')) + '</span>' +
            '<span class="mono-dim" style="font-size:11.5px">' + esc(c.fileName) + '</span>' +
            '<span class="desc" style="font-size:11.5px">' + esc(c.reason) + '</span>' +
            '</div>'
          );
        }
        const url = String(c.downloadUrl || '');
        const link = url.indexOf('https://') === 0
          ? '<a class="btn sm" href="' + esc(url) + '" target="_blank" rel="noopener noreferrer">' +
            esc(t('update.open')) + '</a>'
          : '';
        return (
          '<div class="hstack" style="gap:8px;align-items:center;padding:8px 10px;border-radius:10px;background:var(--card-bg)">' +
          '<input type="checkbox" data-mp-upd="' + esc(c.fileName) + '"' + (selected[c.fileName] ? ' checked' : '') + '>' +
          '<div style="flex:1;min-width:0">' +
          '<div style="font-size:13px;font-weight:600">' + esc(c.project || t('update.unknown')) + '</div>' +
          '<div class="mono-dim" style="font-size:11.5px">' + esc(c.fileName) + ' · ' +
          esc(c.current || '?') + ' → ' + esc(c.latest) + '</div>' +
          '</div>' +
          '<span class="desc" style="font-size:11.5px">' + esc(c.reason) + '</span>' +
          link +
          '</div>'
        );
      })
      .join('');
  }

  // ---------------------------------------------------------------- 数据操作

  async function loadPresets() {
    try {
      presets = (await GM.invoke('preset_list', {})) || [];
    } catch (e) {
      presets = [];
      fail(e);
    }
    paintPresets();
  }

  async function savePreset() {
    if (!requireVersion()) return;
    const input = document.getElementById('mp-preset-name');
    const name = ((input && input.value) || '').trim();
    if (!name) {
      GM.toast(t('preset.nameEmpty'), 'err');
      if (input) input.focus();
      return;
    }
    GM.busy(true, t('preset.saving'));
    try {
      presets = (await GM.invoke('preset_save', { name, versionId: versionId() })) || [];
      GM.busy(false);
      if (input) input.value = '';
      paintPresets();
      GM.toast(t('preset.saved', { name }), 'ok');
    } catch (e) {
      GM.busy(false);
      fail(e);
    }
  }

  async function applyPreset(id) {
    if (!requireVersion()) return;
    GM.busy(true, t('preset.applying'));
    try {
      const r = await GM.invoke('preset_apply', { presetId: id, versionId: versionId() });
      GM.busy(false);
      if (!r) throw new Error(t('mock'));
      GM.toast(
        t('preset.applied', { on: r.enabled || 0, off: r.disabled || 0, fail: (r.failed || []).length }),
        'ok'
      );
      if (GM.renderAll) GM.renderAll();
    } catch (e) {
      GM.busy(false);
      fail(e);
    }
  }

  async function deletePreset(id) {
    GM.busy(true, t('preset.deleting'));
    try {
      presets = (await GM.invoke('preset_delete', { presetId: id })) || [];
      GM.busy(false);
      paintPresets();
      GM.toast(t('preset.deleted'), 'ok');
    } catch (e) {
      GM.busy(false);
      fail(e);
    }
  }

  async function scanConflicts(silent) {
    if (!requireVersion()) return;
    if (!silent) GM.busy(true, t('conflict.scanning'));
    try {
      conflicts = (await GM.invoke('mods_conflicts', { versionId: versionId() })) || [];
      if (!silent) GM.busy(false);
      paintConflicts();
      if (!silent) GM.toast(t('conflict.done', { n: conflicts.length }), 'ok');
    } catch (e) {
      if (!silent) GM.busy(false);
      fail(e);
    }
  }

  async function doImport(paths) {
    if (!requireVersion()) return;
    GM.busy(true, t('import.running'));
    try {
      const r = await GM.invoke('mods_drop_import', { versionId: versionId(), paths });
      GM.busy(false);
      if (!r) throw new Error(t('mock'));
      importReport = r;
      paintImport();
      GM.toast(
        t('import.finished', {
          n: r.count || 0,
          skip: (r.skipped || []).length,
          fail: (r.failed || []).length,
        }),
        'ok'
      );
      if (GM.renderAll) GM.renderAll();
    } catch (e) {
      GM.busy(false);
      fail(e);
    }
  }

  async function checkUpdates() {
    if (!requireVersion()) return;
    GM.busy(true, t('update.checking'));
    try {
      candidates = (await GM.invoke('mods_update_check', { versionId: versionId() })) || [];
      GM.busy(false);
      paintUpdates();
      GM.toast(t('update.done', { n: candidates.filter((c) => c.ok).length }), 'ok');
    } catch (e) {
      GM.busy(false);
      fail(e);
    }
  }

  async function applyUpdates() {
    if (!requireVersion()) return;
    const items = candidates
      .filter((c) => c.ok && selected[c.fileName])
      .map((c) => ({ fileName: c.fileName, downloadUrl: c.downloadUrl }));
    if (!items.length) {
      GM.toast(t('update.noneSelected'), 'err');
      return;
    }
    GM.busy(true, t('update.applying'));
    try {
      const r = await GM.invoke('mods_update_apply', { versionId: versionId(), items });
      GM.busy(false);
      if (!r) throw new Error(t('mock'));
      const done = r.succeeded || [];
      candidates = candidates.filter((c) => done.indexOf(c.fileName) < 0);
      done.forEach((name) => {
        delete selected[name];
      });
      paintUpdates();
      GM.toast(t('update.applied', { ok: done.length, fail: (r.failed || []).length }), 'ok');
      if (GM.renderAll) GM.renderAll();
    } catch (e) {
      GM.busy(false);
      fail(e);
    }
  }

  // ---------------------------------------------------------------- 事件绑定

  function bindPage(page) {
    if (!page || page.dataset.mpBound) return;
    page.dataset.mpBound = '1';

    page.addEventListener('click', (e) => {
      const btn = e.target.closest('button');
      if (!btn) return;
      if (btn.id === 'mp-preset-save') return savePreset();
      if (btn.id === 'mp-preset-refresh') return loadPresets();
      if (btn.id === 'mp-conflict-scan') return scanConflicts(false);
      if (btn.id === 'mp-update-check') return checkUpdates();
      if (btn.id === 'mp-update-apply') return applyUpdates();
      const apply = btn.getAttribute('data-mp-apply');
      if (apply) return applyPreset(apply);
      const del = btn.getAttribute('data-mp-del');
      if (del) return deletePreset(del);
    });

    page.addEventListener('change', (e) => {
      if (e.target.id === 'mp-update-all') {
        const on = !!e.target.checked;
        candidates.filter((c) => c.ok).forEach((c) => {
          selected[c.fileName] = on;
        });
        paintUpdates();
        return;
      }
      const cb = e.target.closest('[data-mp-upd]');
      if (cb) {
        selected[cb.dataset.mpUpd] = !!cb.checked;
        const all = document.getElementById('mp-update-all');
        const good = candidates.filter((c) => c.ok);
        if (all) {
          all.checked = good.length > 0 && good.every((c) => !!selected[c.fileName]);
          all.indeterminate = !all.checked && good.some((c) => !!selected[c.fileName]);
        }
      }
    });

    page.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && e.target.id === 'mp-preset-name') savePreset();
    });

    const zone = document.getElementById('mp-drop');
    if (zone) {
      ['dragenter', 'dragover'].forEach((ev) =>
        zone.addEventListener(ev, (e) => {
          e.preventDefault();
          e.stopPropagation();
          zone.style.borderColor = 'var(--accent-hi)';
        })
      );
      ['dragleave', 'drop'].forEach((ev) =>
        zone.addEventListener(ev, (e) => {
          e.preventDefault();
          e.stopPropagation();
          zone.style.borderColor = 'var(--card-border-hi)';
        })
      );
      zone.addEventListener('drop', (e) => {
        const dt = e.dataTransfer;
        const paths = [];
        if (dt && dt.files) {
          Array.prototype.forEach.call(dt.files, (f) => {
            if (f.path) paths.push(f.path);
          });
        }
        if (!paths.length) GM.toast(t('import.noPaths'), 'err');
        else doImport(paths);
      });
      zone.addEventListener('click', () => GM.toast(t('import.hint')));
    }
  }

  // ---------------------------------------------------------------- 钩子

  GM.register({
    ready: () => {
      const page = GM.addPage('modsplus', pageHtml());
      if (GM.i18n && GM.i18n.apply) GM.i18n.apply(document);
      bindPage(page);
      paintVersion();
      loadPresets();
    },
    render: () => {
      paintVersion();
      paintPresets();
      paintConflicts();
      paintImport();
      paintUpdates();
    },
    nav: (p) => {
      if (p !== 'modsplus') return;
      paintVersion();
      loadPresets();
      scanConflicts(true);
      paintConflicts();
    },
  });
})();
