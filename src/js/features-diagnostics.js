/* =====================================================================
 * features-diagnostics.js —— 诊断：崩溃解释 / 日志导出 / 清理冗余
 *
 * 只通过 gm-ext.js 的 GM.register / GM.addNav / GM.addPage 接入，
 * 不改 app.js / index.html。扫描与清理严格两步：先 scan，再由用户勾选后 apply。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = (window.GM = window.GM || {});
  const t = (key, vars) => GM.t('diagnostics.' + key, vars);
  const esc = (s) => GM.esc(s);
  const PAGE = 'diagnostics';

  // 模块内状态：重复渲染不丢分析结果，也不丢勾选状态。
  const st = {
    version: '',
    explain: null,
    exportPath: '',
    items: [],
    scanned: false,
  };

  function tr(key, fallback) {
    const full = 'diagnostics.' + key;
    const v = GM.t(full);
    return v === full ? fallback : v;
  }

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function versions() {
    const s = GM.state();
    return (s && s.installed) || [];
  }

  function backendOk() {
    const g = window.__GM;
    if (g && g.IS_TAURI === false) {
      GM.toast(t('mock'), 'err');
      return false;
    }
    return true;
  }

  function fmtSize(bytes) {
    const n = Number(bytes) || 0;
    if (!n) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i++;
    }
    return (v >= 100 ? v.toFixed(0) : v.toFixed(1)) + ' ' + units[i];
  }

  function fileName(path) {
    const s = String(path || '');
    const parts = s.split(/[\\/]/);
    return parts[parts.length - 1] || s;
  }

  function syncVersion() {
    const list = versions().map((v) => v.id);
    if (st.version && list.indexOf(st.version) < 0) st.version = '';
  }

  // ---------------------------------------------------------------- 骨架

  const SKELETON = `
    <div class="page-head">
      <h1 data-i18n="diagnostics.title">诊断</h1>
      <div class="sub" data-i18n="diagnostics.subtitle">崩溃解释、日志导出与冗余清理</div>
      <div class="spacer"></div>
      <div class="hstack" style="gap:8px">
        <span class="mini-note" data-i18n="diagnostics.instance">实例</span>
        <select id="diag-version" style="min-width:190px"></select>
      </div>
    </div>

    <div class="stack" style="gap:16px">
      <section class="card">
        <div class="card-title" data-i18n="diagnostics.crash.title">最近崩溃解释</div>
        <div class="mini-note" data-i18n="diagnostics.crash.sub">从最近的日志与崩溃报告里找出错误，给出通俗解释和处理建议</div>
        <div id="diag-crash-body" style="margin-top:12px"></div>
        <div class="hstack" style="margin-top:12px;gap:8px">
          <button class="btn primary" id="diag-crash-refresh" data-i18n="diagnostics.crash.refresh">重新分析</button>
        </div>
      </section>

      <section class="card">
        <div class="card-title" data-i18n="diagnostics.export.title">日志导出分享</div>
        <div class="mini-note" data-i18n="diagnostics.export.sub">打包最近 7 天日志与该实例的崩溃报告到下载目录</div>
        <div id="diag-export-body" style="margin-top:12px"></div>
        <div class="hstack" style="margin-top:12px;gap:8px">
          <button class="btn primary" id="diag-export-go" data-i18n="diagnostics.export.go">导出并打开</button>
          <button class="btn" id="diag-export-only" data-i18n="diagnostics.export.only">仅导出</button>
        </div>
      </section>

      <section class="card">
        <div class="card-title" data-i18n="diagnostics.cleanup.title">清理冗余</div>
        <div class="mini-note" data-i18n="diagnostics.cleanup.sub">先扫描，再勾选要删除的项；默认不勾选任何项</div>
        <div class="hstack" style="margin-top:12px;gap:8px;flex-wrap:wrap">
          <button class="btn primary" id="diag-clean-scan" data-i18n="diagnostics.cleanup.scan">扫描冗余</button>
          <label class="hstack mini-note" style="gap:6px;margin:0">
            <input type="checkbox" id="diag-clean-all">
            <span data-i18n="diagnostics.cleanup.selectAll">全选</span>
          </label>
          <button class="btn danger" id="diag-clean-apply" data-i18n="diagnostics.cleanup.apply">清理所选</button>
          <span class="mono-dim" id="diag-clean-count"></span>
        </div>
        <div id="diag-clean-body" style="margin-top:12px"></div>
      </section>
    </div>`;

  // ---------------------------------------------------------------- 绘制

  function paintVersion() {
    const sel = document.getElementById('diag-version');
    if (!sel) return;
    syncVersion();
    const parts = ['<option value="">' + esc(t('allInstances')) + '</option>'];
    versions().forEach((v) => {
      const on = v.id === st.version ? ' selected' : '';
      parts.push('<option value="' + esc(v.id) + '"' + on + '>' + esc(v.id) + '</option>');
    });
    sel.innerHTML = parts.join('');
  }

  function crashCode(r) {
    return (r && r.code) || 'unknown';
  }

  function crashSuggestions(r) {
    try {
      const arr = JSON.parse(GM.t('diagnostics.crash.kind.' + crashCode(r) + '.suggestions'));
      if (Array.isArray(arr) && arr.length) return arr;
    } catch (_) {
      /* 语言表缺项时退回后端中文文案 */
    }
    return Array.isArray(r.suggestions) ? r.suggestions : [];
  }

  function paintCrash() {
    const box = document.getElementById('diag-crash-body');
    if (!box) return;
    const r = st.explain;
    if (!r) {
      box.innerHTML =
        '<div class="empty"><span class="big-ico">◌</span><span>' +
        esc(t('crash.empty')) +
        '</span></div>';
      return;
    }
    if (!r.source) {
      box.innerHTML =
        '<div class="empty"><span class="big-ico">!</span><span>' +
        esc(tr('crash.kind.' + crashCode(r) + '.title', r.title)) +
        ' · ' +
        esc(t('crash.noneHint')) +
        '</span></div>';
      return;
    }

    const code = crashCode(r);
    const title = tr('crash.kind.' + code + '.title', r.title);
    const reason = tr('crash.kind.' + code + '.reason', r.reason);
    const sugs = crashSuggestions(r);
    const src = fileName(r.source);
    const at = r.at ? new Date(r.at * 1000).toLocaleString() : '—';
    const raw = r.excerpt
      ? '<details style="margin-top:14px">' +
        '<summary class="mini-note" style="cursor:pointer" data-i18n="diagnostics.crash.raw">' +
        esc(t('crash.raw')) +
        '</summary>' +
        '<pre class="code-box" style="white-space:pre-wrap;word-break:break-all;max-height:320px;overflow:auto">' +
        esc(r.excerpt) +
        '</pre></details>'
      : '';

    box.innerHTML =
      '<div class="hstack" style="gap:8px;flex-wrap:wrap">' +
      '<span class="badge-pill accent">' + esc(title) + '</span>' +
      '<span class="mono-dim">' + esc(t('crash.source')) + '：' + esc(src) +
      ' · ' + esc(t('crash.at')) + ' ' + esc(at) + '</span>' +
      '</div>' +
      '<div style="margin-top:12px;font-size:13px;line-height:1.75">' +
      '<div class="card-title" style="margin-bottom:4px" data-i18n="diagnostics.crash.reason">' +
      esc(t('crash.reason')) + '</div>' +
      '<div>' + esc(reason) + '</div>' +
      '<div class="card-title" style="margin:14px 0 4px" data-i18n="diagnostics.crash.suggestions">' +
      esc(t('crash.suggestions')) + '</div>' +
      '<ol style="margin:0;padding-left:20px">' +
      sugs.map((s) => '<li style="margin-bottom:5px">' + esc(s) + '</li>').join('') +
      '</ol></div>' + raw;
  }

  function paintExport() {
    const box = document.getElementById('diag-export-body');
    if (!box) return;
    if (!st.exportPath) {
      box.innerHTML =
        '<div class="empty"><span class="big-ico">◌</span><span>' +
        esc(t('export.empty')) +
        '</span></div>';
      return;
    }
    box.innerHTML =
      '<div class="mini-note">' + esc(t('export.last')) + '</div>' +
      '<pre class="code-box" style="white-space:pre-wrap;word-break:break-all">' +
      esc(st.exportPath) +
      '</pre>';
  }

  const KIND_BADGE = {
    duplicate: 'warn',
    temp: 'warn',
    crash: '',
    log: '',
    junk: 'ok',
    emptyDir: 'accent',
  };

  function paintCleanup() {
    const body = document.getElementById('diag-clean-body');
    const count = document.getElementById('diag-clean-count');
    const all = document.getElementById('diag-clean-all');
    const applyBtn = document.getElementById('diag-clean-apply');
    if (!body) return;

    const checked = st.items.filter((x) => x.checked).length;
    if (count) count.textContent = t('cleanup.count', { n: st.items.length, m: checked });
    if (applyBtn) applyBtn.disabled = checked === 0;
    if (all) {
      all.checked = st.items.length > 0 && checked === st.items.length;
      all.indeterminate = checked > 0 && checked < st.items.length;
    }

    if (!st.scanned) {
      body.innerHTML =
        '<div class="empty"><span class="big-ico">◌</span><span>' +
        esc(t('cleanup.notScanned')) +
        '</span></div>';
      return;
    }
    if (!st.items.length) {
      body.innerHTML =
        '<div class="empty"><span class="big-ico">✓</span><span>' +
        esc(t('cleanup.empty')) +
        '</span></div>';
      return;
    }

    body.innerHTML =
      '<div class="row-list">' +
      st.items
        .map((it) => {
          const kindLabel = tr('cleanup.kind.' + it.kind, it.kind);
          const badge = KIND_BADGE[it.kind] || '';
          return (
            '<div class="vrow" style="align-items:flex-start">' +
            '<input type="checkbox" data-diag-check="' + esc(it.id) + '"' +
            (it.checked ? ' checked' : '') + ' style="margin-top:3px;flex:none">' +
            '<div class="v-main">' +
            '<div class="v-id" title="' + esc(it.path) + '">' + esc(fileName(it.path)) + '</div>' +
            '<div class="mini-note" style="margin-top:4px">' +
            esc(it.description || kindLabel) + ' · ' + esc(fmtSize(it.size)) +
            '</div>' +
            '<div class="mono-dim" style="margin-top:3px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">' +
            esc(it.path) + '</div>' +
            '</div>' +
            '<span class="badge-pill ' + badge + '">' + esc(kindLabel) + '</span>' +
            '</div>'
          );
        })
        .join('') +
      '</div>';
  }

  function repaint() {
    paintVersion();
    paintCrash();
    paintExport();
    paintCleanup();
  }

  // ---------------------------------------------------------------- 动作

  async function doExplain() {
    if (!backendOk()) return;
    GM.busy(true, t('crash.busy'));
    try {
      st.explain = await GM.invoke('crash_explain', { versionId: st.version });
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      GM.busy(false);
      paintCrash();
    }
  }

  async function doExport(reveal) {
    if (!backendOk()) return;
    GM.busy(true, t('export.busy'));
    try {
      const path = await GM.invoke('logs_export', { versionId: st.version });
      if (!path) throw new Error(t('mock'));
      if (reveal) await GM.invoke('logs_reveal', { path });
      st.exportPath = path;
      GM.toast(t('export.done', { path }), 'ok');
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      GM.busy(false);
      paintExport();
    }
  }

  async function doScan() {
    if (!backendOk()) return;
    GM.busy(true, t('cleanup.busy'));
    try {
      const items = await GM.invoke('cleanup_scan', { versionId: st.version });
      st.items = (Array.isArray(items) ? items : []).map((x) => Object.assign({}, x, { checked: false }));
      st.scanned = true;
    } catch (e) {
      st.items = [];
      st.scanned = false;
      GM.toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      GM.busy(false);
      paintCleanup();
    }
  }

  function doApply() {
    if (!backendOk()) return;
    const ids = st.items.filter((x) => x.checked).map((x) => x.id);
    if (!ids.length) {
      GM.toast(t('cleanup.pickFirst'), 'err');
      return;
    }
    if (!GM.ui || !GM.ui.confirm) return;
    GM.ui.confirm(t('cleanup.confirmTitle'), t('cleanup.confirmDesc', { n: ids.length }), async () => {
      GM.busy(true, t('cleanup.busy'));
      try {
        const res = await GM.invoke('cleanup_apply', { ids });
        const removed = res && typeof res.removed === 'number' ? res.removed : 0;
        const failing = res && Array.isArray(res.failed) ? res.failed : [];
        GM.toast(t('cleanup.done', { n: removed, f: failing.length }), failing.length ? 'err' : 'ok');
        // 成功删掉的移出列表；失败的留着并取消勾选，方便用户看到后再处理。
        const failedIds = failing.map((f) => f.id);
        st.items = st.items
          .filter((x) => ids.indexOf(x.id) < 0 || failedIds.indexOf(x.id) >= 0)
          .map((x) =>
            failedIds.indexOf(x.id) >= 0 ? Object.assign({}, x, { checked: false }) : x,
          );
      } catch (e) {
        GM.toast(t('fail', { err: brief(e) }), 'err');
      } finally {
        GM.busy(false);
        paintCleanup();
      }
    });
  }

  // ---------------------------------------------------------------- 事件（只在 ready 里绑定一次）

  function bind(page) {
    if (!page || page.dataset.diagBound) return;
    page.dataset.diagBound = '1';

    page.addEventListener('click', (e) => {
      if (e.target.closest('#diag-crash-refresh')) return doExplain();
      if (e.target.closest('#diag-export-go')) return doExport(true);
      if (e.target.closest('#diag-export-only')) return doExport(false);
      if (e.target.closest('#diag-clean-scan')) return doScan();
      if (e.target.closest('#diag-clean-apply')) return doApply();
    });

    page.addEventListener('change', (e) => {
      const el = e.target;
      if (el.id === 'diag-version') {
        st.version = el.value || '';
        st.explain = null;
        st.exportPath = '';
        st.items = [];
        st.scanned = false;
        repaint();
        return;
      }
      if (el.id === 'diag-clean-all') {
        st.items.forEach((x) => (x.checked = !!el.checked));
        paintCleanup();
        return;
      }
      const box = el.getAttribute && el.getAttribute('data-diag-check');
      if (box) {
        const item = st.items.find((x) => x.id === box);
        if (item) {
          item.checked = !!el.checked;
          paintCleanup();
        }
      }
    });
  }

  // ---------------------------------------------------------------- 注册

  GM.register({
    ready() {
      const icon =
        '<svg class="ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M8 1.8l5.4 2.1v4.2c0 3-2.2 5.3-5.4 6.1C4.8 13.4 2.6 11.1 2.6 8.1V3.9z"/>' +
        '<path d="M5.7 8.1l1.6 1.6 3.2-3.4"/></svg>';
      GM.addNav({ id: PAGE, label: 'nav.diagnostics', icon, title: t('title') });
      const page = GM.addPage(PAGE, SKELETON);
      bind(page);
      if (GM.i18n) GM.i18n.apply(document);
      repaint();
    },
    render() {
      repaint();
    },
    nav(page) {
      if (page === PAGE) repaint();
    },
  });
})();
