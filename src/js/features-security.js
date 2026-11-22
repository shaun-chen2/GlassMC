/* =====================================================================
 * features-security.js —— 安全中心
 *   1) 恶意模组扫描：启发式、纯只读，结论不替代杀毒软件
 *   2) Minecraft 官方新闻：RSS 抓取 + 本地缓存 + 离线示例回退
 *
 * 只通过 GM.register 的钩子碰 DOM；后端命令见 src-tauri/src/security.rs。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = (window.GM = window.GM || {});
  const HOOKS = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });
  const t = (k, v) => GM.t('security.' + k, v);
  const esc = (s) => (GM.ui && GM.ui.escAttr ? GM.ui.escAttr(s) : String(s == null ? '' : s));
  const toast = (m, k) => GM.toast(m, k);

  const PAGE = 'security';
  const ICON =
    '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" ' +
    'stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">' +
    '<path d="M12 3l7 3v5c0 4.6-3 8.2-7 10-4-1.8-7-5.4-7-10V6l7-3z"/>' +
    '<path d="M9.2 12.2l1.9 1.9 3.8-4"/></svg>';
  const SEV_CLASS = { high: 'danger', medium: 'warn', low: 'accent' };

  let scanReport = null; // { results, scanned }
  let scanRunning = false;
  let news = null; // { items, cached, demo, updatedAt }
  let newsRunning = false;

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function versions() {
    const S = typeof GM.state === 'function' ? GM.state() : null;
    const list = S && Array.isArray(S.installed) ? S.installed : [];
    return list.map((v) => (v && v.id) || '').filter(Boolean);
  }

  // ---------------------------------------------------------------- 页面骨架

  function pageHtml() {
    return `
      <div class="page-head">
        <div>
          <h1 data-i18n="security.title">安全中心</h1>
          <div class="sub" data-i18n="security.sub">恶意模组启发式扫描 · Minecraft 官方新闻</div>
        </div>
      </div>
      <div class="mini-note" data-i18n="security.disclaimer" style="color:var(--warn)">
        启发式提示，不能替代杀毒软件；扫描只读，不会删除或隔离任何文件。
      </div>

      <section class="card section-gap">
        <div class="card-title">
          <span data-i18n="security.scan.title">恶意模组扫描</span>
          <span class="k">jar / zip</span>
        </div>
        <div class="hstack" style="gap:8px;flex-wrap:wrap;margin-bottom:10px">
          <select id="sec-version" aria-label="version" style="min-width:180px"></select>
          <button class="btn primary" data-sec-act="scanOne" data-i18n="security.scan.one">扫描所选版本</button>
          <button class="btn" data-sec-act="scanAll" data-i18n="security.scan.all">扫描全部版本</button>
        </div>
        <div id="sec-scan-summary"></div>
        <div id="sec-scan-list"></div>
      </section>

      <section class="card section-gap">
        <div class="card-title">
          <span data-i18n="security.news.title">Minecraft 新闻</span>
          <button class="btn sm" data-sec-act="newsRefresh" data-i18n="security.news.refresh">刷新</button>
        </div>
        <div id="sec-news-list"></div>
      </section>`;
  }

  function fillVersions() {
    const sel = document.getElementById('sec-version');
    if (!sel) return;
    const list = versions();
    const cur = sel.value;
    sel.innerHTML = list.length
      ? list.map((id) => `<option value="${esc(id)}">${esc(id)}</option>`).join('')
      : `<option value="">${esc(t('scan.noVersion'))}</option>`;
    if (cur && list.indexOf(cur) >= 0) sel.value = cur;
  }

  // ---------------------------------------------------------------- 扫描结果

  function topSeverity(r) {
    const s = ((r && r.findings) || []).map((f) => f.severity);
    if (s.indexOf('high') >= 0) return 'high';
    if (s.indexOf('medium') >= 0) return 'medium';
    if (s.indexOf('low') >= 0) return 'low';
    return 'none';
  }

  function renderScan() {
    fillVersions();
    const sum = document.getElementById('sec-scan-summary');
    const list = document.getElementById('sec-scan-list');
    if (!sum || !list) return;

    if (scanRunning) {
      sum.innerHTML = `<div class="desc">${esc(t('scan.running'))}</div>`;
      list.innerHTML = '';
      return;
    }
    if (!scanReport) {
      sum.innerHTML = '';
      list.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${esc(t('scan.hint'))}</span></div>`;
      return;
    }

    const results = scanReport.results || [];
    if (!scanReport.scanned) {
      sum.innerHTML = `<div class="desc">${esc(t('scan.empty'))}</div>`;
      list.innerHTML = '';
      return;
    }

    const counts = { high: 0, medium: 0, low: 0, none: 0 };
    results.forEach((r) => counts[topSeverity(r)]++);
    sum.innerHTML = `<div class="chips" style="margin-bottom:8px">
      <span class="badge-pill">${esc(t('scan.count', { n: scanReport.scanned }))}</span>
      <span class="badge-pill danger">${esc(t('risk.high'))} ${counts.high}</span>
      <span class="badge-pill warn">${esc(t('risk.medium'))} ${counts.medium}</span>
      <span class="badge-pill accent">${esc(t('risk.low'))} ${counts.low}</span>
      <span class="badge-pill ok">${esc(t('risk.none'))} ${counts.none}</span>
    </div>`;

    if (!results.length) {
      list.innerHTML = `<div class="empty"><span class="big-ico">✓</span><span>${esc(t('scan.clean'))}</span></div>`;
      return;
    }

    list.innerHTML = results
      .map((r, i) => {
        const sev = topSeverity(r);
        const cls = SEV_CLASS[sev] || 'ok';
        const findings = ((r.findings || []).length
          ? r.findings
          : [{ severity: 'none', title: t('scan.noFinding'), detail: '' }]
        )
          .map(
            (f) =>
              `<div class="kv"><span class="badge-pill ${SEV_CLASS[f.severity] || 'ok'}">${esc(
                t('risk.' + (f.severity || 'none')),
              )}</span><span class="v">${esc(f.title)}${f.detail ? ' — ' + esc(f.detail) : ''}</span></div>`,
          )
          .join('');
        const shortHash = (r.sha256 || '').slice(0, 20);
        return `<div class="vrow">
          <div class="v-main">
            <div class="v-id">${esc(r.file)} <span class="badge-pill ${cls}">${esc(
              t('risk.score', { s: r.score || 0 }),
            )}</span></div>
            <div class="v-meta mono-dim">sha256 ${esc(shortHash || '—')}${
              shortHash ? '…' : ''
            }</div>
          </div>
          <div class="v-actions">
            <button class="btn sm" data-sec-act="detail" data-i="${i}">${esc(t('scan.detail'))}</button>
            <button class="btn sm" data-sec-act="openDir" data-i="${i}">${esc(t('scan.openDir'))}</button>
          </div>
          <div class="stack" data-sec-detail="${i}" style="display:none;gap:6px;margin-top:10px">${findings}</div>
        </div>`;
      })
      .join('');
  }

  async function runScan(vid) {
    if (scanRunning) return;
    scanRunning = true;
    GM.busy(true, t('scan.running'));
    renderScan();
    try {
      scanReport = await GM.invoke('mods_security_scan', { versionId: vid });
      renderScan();
      toast(t('scan.done', { n: scanReport.scanned }), 'ok');
    } catch (e) {
      scanReport = null;
      renderScan();
      toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      scanRunning = false;
      GM.busy(false);
      renderScan();
    }
  }

  // ---------------------------------------------------------------- 新闻

  function renderNews() {
    const box = document.getElementById('sec-news-list');
    if (!box) return;
    if (newsRunning) {
      box.innerHTML = `<div class="desc">${esc(t('news.loading'))}</div>`;
      return;
    }
    if (!news) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${esc(t('news.hint'))}</span></div>`;
      return;
    }

    let head = '';
    if (news.demo) {
      head = `<div class="mini-note" style="color:var(--warn);margin-bottom:8px">${esc(
        t('news.demoNote'),
      )}</div>`;
    } else if (news.cached) {
      head = `<div class="mini-note" style="margin-bottom:8px">${esc(t('news.cachedNote'))}</div>`;
    }

    const items = news.items || [];
    if (!items.length) {
      box.innerHTML = head + `<div class="empty"><span class="big-ico">◌</span><span>${esc(t('news.empty'))}</span></div>`;
      return;
    }
    box.innerHTML =
      head +
      items
        .map(
          (n) => `<div class="vrow">
            <div class="v-main">
              <div class="v-id">${esc(n.title)}</div>
              <div class="v-meta">${esc(n.pubDate || '')}</div>
              ${
                n.description
                  ? `<div class="desc" style="margin-top:4px">${esc(n.description)}</div>`
                  : ''
              }
            </div>
            <div class="v-actions"><button class="btn sm" data-sec-act="openUrl" data-url="${esc(
              n.link,
            )}">${esc(t('news.open'))}</button></div>
          </div>`,
        )
        .join('');
  }

  async function runNews(force) {
    if (newsRunning) return;
    newsRunning = true;
    GM.busy(true, t('news.loading'));
    renderNews();
    try {
      news = await GM.invoke(force ? 'mc_news_refresh' : 'mc_news', {});
      renderNews();
    } catch (e) {
      toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      newsRunning = false;
      GM.busy(false);
      renderNews();
    }
  }

  // ---------------------------------------------------------------- 事件

  function bind(page) {
    if (page.dataset.secBound) return;
    page.dataset.secBound = '1';
    page.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-sec-act]');
      if (!btn) return;
      const act = btn.dataset.secAct;
      if (act === 'scanOne' || act === 'scanAll') {
        const sel = document.getElementById('sec-version');
        runScan(act === 'scanAll' ? '' : sel ? sel.value : '');
      } else if (act === 'newsRefresh') {
        runNews(true);
      } else if (act === 'detail') {
        const d = page.querySelector('[data-sec-detail="' + btn.dataset.i + '"]');
        if (d) d.style.display = d.style.display === 'none' ? '' : 'none';
      } else if (act === 'openDir') {
        const r = scanReport && scanReport.results && scanReport.results[Number(btn.dataset.i)];
        if (r && r.path) {
          GM.invoke('open_dir', { path: r.path }).catch((er) =>
            toast(t('fail', { err: brief(er) }), 'err'),
          );
        }
      } else if (act === 'openUrl') {
        const url = btn.dataset.url;
        if (url) {
          GM.invoke('open_url', { url: url }).catch((er) =>
            toast(t('fail', { err: brief(er) }), 'err'),
          );
        }
      }
    });
  }

  // ---------------------------------------------------------------- 钩子

  function init() {
    GM.addNav({ id: PAGE, label: 'nav.security', icon: ICON, title: t('title') });
    const page = GM.addPage(PAGE, pageHtml());
    if (page) bind(page);
    if (GM.i18n && typeof GM.i18n.onChange === 'function') {
      GM.i18n.onChange(() => {
        renderScan();
        renderNews();
      });
    }
    renderScan();
    renderNews();
  }

  function onNav(page) {
    if (page !== PAGE) return;
    renderScan();
    renderNews();
    if (!news && !newsRunning) runNews(false);
  }

  function onRender() {
    if (!document.querySelector('[data-page="' + PAGE + '"]')) return;
    renderScan();
    renderNews();
  }

  if (typeof GM.register === 'function') {
    GM.register({ ready: init, render: onRender, nav: onNav });
  } else {
    HOOKS.ready.push(init);
    HOOKS.render.push(onRender);
    HOOKS.nav.push(onNav);
  }
})();
