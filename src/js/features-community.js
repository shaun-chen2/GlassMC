/* =====================================================================
 * features-community.js —— 分享社区 + 硬件面板
 *
 * 五块内容：分享包 / MCBBS 模板 / 服务器面板 / 便携版导出 / 硬件仪表盘。
 * 所有数据都来自本地命令：不上传文件、不伪造论坛 API、不自动发帖。
 * 页面与事件只通过 GM.register 钩子创建，不碰 app.js 的既有逻辑。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = window.GM;
  if (!GM || !GM.register) return;

  const NAV_ID = 'community';
  const ICON =
    '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" ' +
    'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">' +
    '<path d="M4 12a8 8 0 0 1 13.7-5.7L20 8.6M20 4v4.6h-4.6' +
    'M20 12a8 8 0 0 1-13.7 5.7L4 15.4M4 20v-4.6h4.6"/></svg>';

  const t = (key, vars) => GM.t('community.' + key, vars);
  const esc = (s) => GM.esc(s);
  const invoke = (cmd, args) => GM.invoke(cmd, args || {});
  const toast = (msg, kind) => GM.toast(msg, kind);
  const busy = (on, label) => GM.busy(on, label);

  let usageTimer = null;
  let lastLocale = GM.i18n ? GM.i18n.locale : 'zh-CN';

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function fail(e) {
    toast(t('fail', { err: brief(e) }), 'err');
  }

  function versionValue() {
    const sel = document.getElementById('cm-version');
    return sel ? sel.value : '';
  }

  function copyText(text) {
    if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
      return navigator.clipboard.writeText(text);
    }
    return new Promise((resolve, reject) => {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      let ok = false;
      try {
        ok = document.execCommand('copy');
      } catch (_) {
        ok = false;
      }
      document.body.removeChild(ta);
      if (ok) resolve();
      else reject(new Error('copy failed'));
    });
  }

  function doCopy(text) {
    if (!text) {
      toast(t('empty'), 'err');
      return;
    }
    copyText(text)
      .then(() => toast(t('copied'), 'ok'))
      .catch(() => toast(t('copyFail'), 'err'));
  }

  // ---------------------------------------------------------------- 页面结构

  function versionField() {
    return (
      '<div class="field">' +
      '<div class="lbl">' + esc(t('share.version')) + '</div>' +
      '<select id="cm-version" style="width:100%"></select>' +
      '</div>'
    );
  }

  function barHtml(id, label) {
    return (
      '<div class="cm-bar" id="' + id + '" style="margin-top:8px">' +
      '<div class="hstack" style="justify-content:space-between;font-size:12.5px">' +
      '<span>' + esc(label) + '</span>' +
      '<span class="cm-bar-val mono-dim">' + esc(t('hw.unknown')) + '</span>' +
      '</div>' +
      '<div style="height:8px;border-radius:6px;background:rgba(128,128,128,.22);' +
      'overflow:hidden;margin-top:4px">' +
      '<div class="cm-bar-fill" style="height:100%;width:0%;border-radius:6px;' +
      'background:linear-gradient(90deg,var(--accent),var(--accent-hi));' +
      'transition:width .3s ease"></div></div></div>'
    );
  }

  function pageHtml() {
    return [
      '<div class="page-head">',
      '  <h1>' + esc(t('title')) + '</h1>',
      '  <div class="sub">' + esc(t('sub')) + '</div>',
      '</div>',
      '',
      // 1. 分享
      '<section class="card section-gap">',
      '  <div class="card-title">' + esc(t('share.title')) + '</div>',
      '  <div class="desc">' + esc(t('share.sub')) + '</div>',
      '  <div style="margin-top:10px;max-width:420px">' + versionField() + '</div>',
      '  <div class="hstack" style="gap:8px;flex-wrap:wrap;margin-top:10px">',
      '    <button class="btn primary sm" data-cm-act="export">' + esc(t('share.export')) + '</button>',
      '    <button class="btn sm" data-cm-act="intro">' + esc(t('share.intro')) + '</button>',
      '  </div>',
      '  <div class="mini-note mono-dim" id="cm-export-path" style="margin-top:8px;word-break:break-all"></div>',
      '  <div class="card-title" style="margin-top:14px">' + esc(t('share.platforms')) + '</div>',
      '  <div class="desc">' + esc(t('share.platformHint')) + '</div>',
      '  <div class="hstack" style="gap:8px;flex-wrap:wrap;margin-top:8px">',
      '    <button class="btn sm" data-cm-act="github">' + esc(t('share.github')) + '</button>',
      '    <button class="btn sm" data-cm-act="bilibili">' + esc(t('share.bilibili')) + '</button>',
      '    <button class="btn sm" data-cm-act="mcbbs">' + esc(t('share.mcbbs')) + '</button>',
      '  </div>',
      '</section>',
      '',
      // 2. MCBBS 模板
      '<section class="card section-gap">',
      '  <div class="card-title">' + esc(t('tpl.title')) + '</div>',
      '  <div class="desc">' + esc(t('tpl.sub')) + '</div>',
      '  <div class="field" style="margin-top:10px">',
      '    <div class="lbl">' + esc(t('tpl.label')) + '</div>',
      '    <textarea id="cm-tpl" rows="6" style="width:100%;font-family:monospace" ' +
      'placeholder="' + esc(t('tpl.ph')) + '"></textarea>',
      '    <div class="mini-note" style="margin-top:4px">' + esc(t('tpl.hint')) + '</div>',
      '  </div>',
      '  <div class="field" style="margin-top:8px">',
      '    <div class="lbl">' + esc(t('tpl.link')) + '</div>',
      '    <input type="text" id="cm-link" style="width:100%" placeholder="' + esc(t('tpl.linkPh')) + '">',
      '  </div>',
      '  <div class="hstack" style="gap:8px;flex-wrap:wrap;margin-top:8px">',
      '    <button class="btn sm" data-cm-act="tpl-save">' + esc(t('tpl.save')) + '</button>',
      '    <button class="btn sm" data-cm-act="tpl-render">' + esc(t('tpl.render')) + '</button>',
      '    <button class="btn sm" data-cm-act="tpl-copy">' + esc(t('tpl.copy')) + '</button>',
      '  </div>',
      '  <div class="field" style="margin-top:10px">',
      '    <div class="lbl">' + esc(t('tpl.result')) + '</div>',
      '    <textarea id="cm-bbcode" rows="7" style="width:100%;font-family:monospace" readonly ' +
      'placeholder="' + esc(t('tpl.emptyResult')) + '"></textarea>',
      '  </div>',
      '</section>',
      '',
      // 3. 服务器面板
      '<section class="card section-gap">',
      '  <div class="card-title">' + esc(t('srv.title')) + '</div>',
      '  <div class="desc">' + esc(t('srv.sub')) + '</div>',
      '  <div id="cm-servers" style="margin-top:10px"></div>',
      '</section>',
      '',
      // 4. 便携版导出
      '<section class="card section-gap">',
      '  <div class="card-title">' + esc(t('portable.title')) + '</div>',
      '  <div class="desc">' + esc(t('portable.sub')) + '</div>',
      '  <div class="hstack" style="gap:8px;margin-top:10px">',
      '    <button class="btn primary sm" data-cm-act="portable">' + esc(t('portable.go')) + '</button>',
      '  </div>',
      '  <div class="mini-note" style="margin-top:8px">' + esc(t('portable.hint')) + '</div>',
      '</section>',
      '',
      // 5. 硬件仪表盘
      '<section class="card section-gap">',
      '  <div class="hstack" style="justify-content:space-between;gap:8px">',
      '    <div class="card-title">' + esc(t('hw.title')) + '</div>',
      '    <button class="btn sm" data-cm-act="hw-refresh">' + esc(t('hw.refresh')) + '</button>',
      '  </div>',
      '  <div class="desc">' + esc(t('hw.sub')) + '</div>',
      '  <div id="cm-hw-info" class="stack" style="gap:6px;font-size:13px;margin-top:10px">' +
      esc(t('loading')) + '</div>',
      '  <div style="margin-top:12px">',
      barHtml('cm-cpu-bar', t('hw.cpuUsage')),
      barHtml('cm-mem-bar', t('hw.memUsage')),
      '  </div>',
      '</section>',
      '',
    ].join('\n');
  }

  function buildPage() {
    const page = GM.addPage(NAV_ID, pageHtml());
    if (!page) return;
    bindPage(page);
    fillVersions();
  }

  // ---------------------------------------------------------------- 版本下拉

  function fillVersions() {
    const sel = document.getElementById('cm-version');
    if (!sel) return;
    const st = GM.state() || {};
    const list = Array.isArray(st.installed) ? st.installed : [];
    const prev = sel.value;
    if (!list.length) {
      sel.innerHTML = '<option value="">' + esc(t('noVersions')) + '</option>';
      return;
    }
    sel.innerHTML = list
      .map((v) => '<option value="' + esc(v.id) + '">' + esc(v.id) + '</option>')
      .join('');
    if (prev && list.some((v) => v.id === prev)) sel.value = prev;
  }

  // ---------------------------------------------------------------- 1. 分享

  async function doExport() {
    const id = versionValue();
    if (!id) {
      toast(t('needVersion'), 'err');
      return;
    }
    busy(true, t('share.exporting'));
    try {
      const res = await invoke('share_export', { versionId: id });
      const out = document.getElementById('cm-export-path');
      if (out) out.textContent = t('share.done', { path: res.path, files: res.files });
      toast(t('share.done', { path: res.path, files: res.files }), 'ok');
    } catch (e) {
      fail(e);
    } finally {
      busy(false);
    }
  }

  async function doShareText() {
    const id = versionValue();
    if (!id) {
      toast(t('needVersion'), 'err');
      return;
    }
    try {
      doCopy(await invoke('share_text', { versionId: id }));
    } catch (e) {
      fail(e);
    }
  }

  function openPlatform(kind) {
    const query = encodeURIComponent((versionValue() || 'Minecraft') + ' 整合包');
    const urls = {
      github: 'https://github.com/search?q=' + query,
      bilibili: 'https://search.bilibili.com/all?keyword=' + query,
      mcbbs: 'https://www.mcbbs.net/search.php?mod=forum&srchtxt=' + query,
    };
    invoke('open_url', { url: urls[kind] }).catch(fail);
  }

  // ---------------------------------------------------------------- 2. MCBBS

  async function loadTemplate() {
    try {
      const tpl = await invoke('mcbbs_template_get');
      const box = document.getElementById('cm-tpl');
      if (box && !box.value) box.value = tpl || '';
    } catch (e) {
      fail(e);
    }
  }

  async function saveTemplate() {
    const box = document.getElementById('cm-tpl');
    try {
      await invoke('mcbbs_template_set', { template: box ? box.value : '' });
      toast(t('tpl.saved'), 'ok');
    } catch (e) {
      fail(e);
    }
  }

  async function renderBbcode() {
    const id = versionValue();
    if (!id) {
      toast(t('tpl.noVersion'), 'err');
      return;
    }
    const tpl = document.getElementById('cm-tpl');
    const linkEl = document.getElementById('cm-link');
    const link = linkEl ? linkEl.value.trim() : '';
    try {
      if (tpl) await invoke('mcbbs_template_set', { template: tpl.value });
      const text = await invoke('mcbbs_render', { versionId: id, link: link });
      const out = document.getElementById('cm-bbcode');
      if (out) out.value = text || '';
    } catch (e) {
      fail(e);
    }
  }

  function copyBbcode() {
    const out = document.getElementById('cm-bbcode');
    doCopy(out ? out.value : '');
  }

  // ---------------------------------------------------------------- 3. 服务器

  function emptyBox(msg) {
    return '<div class="empty"><span class="big-ico">◌</span><span>' + esc(msg) + '</span></div>';
  }

  /** 服务器 MOTD 常带 § 颜色 / 格式代码，展示前去掉。 */
  function cleanMotd(s) {
    return String(s == null ? '' : s).replace(/§[0-9a-fk-or]/gi, '').trim();
  }

  function serverCard(s) {
    const name = s.name || s.address || t('srv.unnamed');
    const ping = s.lastPingMs > 0 ? s.lastPingMs + ' ms' : t('srv.unknown');
    const players = s.playersOnline > 0 ? String(s.playersOnline) : '—';
    const motd = cleanMotd(s.motd);
    return [
      '<div class="card" style="padding:10px;background:rgba(128,128,128,.07)">',
      '  <div class="card-title" style="font-size:13px">',
      esc(name),
      s.pinned ? ' <span class="badge-pill warn">★</span>' : '',
      '  </div>',
      '  <div class="desc mono">' + esc(s.address || '') + '</div>',
      '  <div class="hstack" style="gap:12px;font-size:12.5px;flex-wrap:wrap;margin-top:6px">',
      '    <span>' + esc(t('srv.pingLabel')) + '：<b>' + esc(ping) + '</b></span>',
      '    <span>' + esc(t('srv.players')) + '：<b>' + esc(players) + '</b></span>',
      '    <span class="badge-pill ' + (s.online ? 'ok' : '') + '">' +
      esc(s.online ? t('srv.onlineTag') : t('srv.offlineTag')) + '</span>',
      '  </div>',
      motd
        ? '  <div class="mini-note" style="margin-top:6px;word-break:break-all">' + esc(motd) + '</div>'
        : '',
      '</div>',
    ].join('');
  }

  async function refreshServers() {
    const box = document.getElementById('cm-servers');
    if (!box) return;
    box.innerHTML = '<div class="desc">' + esc(t('loading')) + '</div>';
    try {
      const data = await invoke('server_dashboard');
      if (!data || !data.total || !data.items || !data.items.length) {
        box.innerHTML = emptyBox(t('srv.empty'));
        return;
      }
      box.innerHTML =
        '<div class="hstack" style="gap:8px;flex-wrap:wrap">' +
        '<span class="badge-pill">' + esc(t('srv.total', { n: data.total })) + '</span>' +
        '<span class="badge-pill ok">' + esc(t('srv.online', { n: data.online })) + '</span>' +
        '<span class="badge-pill warn">' + esc(t('srv.pinned', { n: data.pinned })) + '</span>' +
        '<button class="btn sm" data-cm-act="srv-ping">' + esc(t('srv.ping')) + '</button>' +
        '</div>' +
        '<div class="stack" style="gap:8px;margin-top:10px">' +
        data.items.map(serverCard).join('') +
        '</div>';
    } catch (e) {
      box.innerHTML = emptyBox(t('fail', { err: brief(e) }));
    }
  }

  async function pingAll() {
    busy(true, t('srv.pinging'));
    try {
      await invoke('servers_ping_all');
      await refreshServers();
    } catch (e) {
      // 该命令由「联机」模块实现；缺失或失败时只给友好提示。
      toast(t('srv.pingFail', { err: brief(e) }), 'err');
    } finally {
      busy(false);
    }
  }

  // ---------------------------------------------------------------- 4. 便携版

  async function doPortable() {
    let dir;
    try {
      dir = await invoke('pick_folder');
    } catch (e) {
      fail(e);
      return;
    }
    if (!dir) return;
    busy(true, t('portable.running'));
    try {
      const res = await invoke('portable_export', { targetDir: dir });
      toast(t('portable.done', { files: res.files, path: res.path }), 'ok');
    } catch (e) {
      fail(e);
    } finally {
      busy(false);
    }
  }

  // ---------------------------------------------------------------- 5. 硬件

  function setBar(id, pct) {
    const wrap = document.getElementById(id);
    if (!wrap) return;
    const fill = wrap.querySelector('.cm-bar-fill');
    const label = wrap.querySelector('.cm-bar-val');
    const ok = typeof pct === 'number' && isFinite(pct);
    const value = ok ? Math.max(0, Math.min(100, pct)) : null;
    if (fill) fill.style.width = (value == null ? 0 : value) + '%';
    if (label) label.textContent = value == null ? t('hw.unknown') : Math.round(value) + '%';
  }

  async function refreshHardware() {
    try {
      const info = await invoke('hardware_info');
      const box = document.getElementById('cm-hw-info');
      if (!box) return;
      const rams = info.totalRamGb ? info.totalRamGb + ' GB' : t('hw.unknown');
      const rows = [
        [t('hw.cpu'), info.cpu],
        [t('hw.cores'), info.cores],
        [t('hw.ram'), rams],
        [t('hw.gpu'), info.gpu],
      ];
      box.innerHTML = rows
        .map((row) => {
          const value = row[1] == null || row[1] === '' ? t('hw.unknown') : String(row[1]);
          return (
            '<div class="hstack" style="justify-content:space-between;gap:10px">' +
            '<span>' + esc(row[0]) + '</span><b style="font-weight:600">' + esc(value) + '</b></div>'
          );
        })
        .join('');
    } catch (e) {
      const box = document.getElementById('cm-hw-info');
      if (box) box.textContent = t('fail', { err: brief(e) });
    }
  }

  async function refreshUsage() {
    try {
      const u = await invoke('hardware_usage');
      setBar('cm-cpu-bar', u ? u.cpuPercent : null);
      setBar('cm-mem-bar', u ? u.memPercent : null);
    } catch (_) {
      setBar('cm-cpu-bar', null);
      setBar('cm-mem-bar', null);
    }
  }

  function startUsageTimer() {
    stopUsageTimer();
    refreshUsage();
    usageTimer = setInterval(refreshUsage, 5000);
  }

  function stopUsageTimer() {
    if (usageTimer) {
      clearInterval(usageTimer);
      usageTimer = null;
    }
  }

  function enter() {
    fillVersions();
    loadTemplate();
    refreshServers();
    refreshHardware();
    startUsageTimer();
  }

  // ---------------------------------------------------------------- 事件绑定

  function bindPage(page) {
    if (page.dataset.cmBound) return;
    page.dataset.cmBound = '1';
    page.addEventListener('click', (ev) => {
      const btn = ev.target.closest('[data-cm-act]');
      if (!btn || !page.contains(btn)) return;
      switch (btn.dataset.cmAct) {
        case 'export': doExport(); break;
        case 'intro': doShareText(); break;
        case 'github': openPlatform('github'); break;
        case 'bilibili': openPlatform('bilibili'); break;
        case 'mcbbs': openPlatform('mcbbs'); break;
        case 'tpl-save': saveTemplate(); break;
        case 'tpl-render': renderBbcode(); break;
        case 'tpl-copy': copyBbcode(); break;
        case 'srv-ping': pingAll(); break;
        case 'portable': doPortable(); break;
        case 'hw-refresh': refreshHardware(); refreshUsage(); break;
        default: break;
      }
    });
  }

  // ---------------------------------------------------------------- 钩子

  GM.register({
    ready() {
      GM.addNav({ id: NAV_ID, label: 'nav.community', icon: ICON, title: t('title') });
      buildPage();
      lastLocale = GM.i18n ? GM.i18n.locale : 'zh-CN';
    },
    nav(page) {
      if (page === NAV_ID) enter();
      else stopUsageTimer();
    },
    render() {
      const loc = GM.i18n ? GM.i18n.locale : 'zh-CN';
      if (loc !== lastLocale) {
        lastLocale = loc;
        buildPage(); // 语言切换：保留事件委托，重建文案
      } else {
        fillVersions();
      }
    },
  });
})();
