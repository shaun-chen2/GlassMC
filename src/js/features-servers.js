/* =====================================================================
 * features-servers.js —— 联机：服务器列表+延迟、公共浏览器、FRP 客户端
 *
 * 通过 gm-ext.js 的 GM.register / GM.addNav / GM.addPage 接入，不改 app.js / index.html。
 * 所有命令都带超时与离线回退；FRP 二进制只在用户点「下载」时才下载，且不自动启动。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = window.GM;
  if (!GM || typeof GM.register !== 'function') {
    console.warn('[servers] gm-ext.js 尚未加载，联机模块跳过');
    return;
  }
  const esc = (s) => GM.esc(s);
  const t = (k, v) => GM.t(k, v);
  const L = (k) => esc(GM.t(k));
  const L2 = (k, v) => esc(GM.t(k, v));

  let page = null;
  let myServers = [];
  let browser = null;
  let frpCfg = null;
  let frpStatus = null;
  let frpLogText = '';
  const browserPing = Object.create(null);
  let browserPinged = false;

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function keyOf(it) {
    return it.address + ':' + it.port;
  }

  /** <80 绿 / <200 黄 / 其余红；延迟条越短越慢。 */
  function latency(ms) {
    if (ms == null) {
      return { color: 'var(--text-dimmer)', width: 4, text: GM.t('servers.ping.na') };
    }
    let color = 'var(--danger)';
    if (ms < 80) color = 'var(--ok)';
    else if (ms < 200) color = 'var(--warn)';
    const width = Math.max(8, Math.min(100, Math.round(120 - ms / 5)));
    return { color, width, text: ms + ' ms' };
  }

  async function copyText(text) {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
        return true;
      }
    } catch (e) {
      /* 落到下面的兜底 */
    }
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand('copy');
      document.body.removeChild(ta);
      return ok;
    } catch (e) {
      return false;
    }
  }

  // ---------------------------------------------------------------- 页面骨架

  function field(id, labelKey, type, value) {
    return (
      `<div class="field"><div class="lbl" data-i18n="${labelKey}">${L(labelKey)}</div>` +
      `<input type="${type}" id="${id}" value="${esc(value)}"></div>`
    );
  }

  function shellHtml() {
    return `
      <div class="page-head">
        <h1 data-i18n="servers.title">${L('servers.title')}</h1>
        <div class="sub" data-i18n="servers.sub">${L('servers.sub')}</div>
      </div>
      <div class="tabs">
        <button class="tab on" data-srv-tab="mine" data-i18n="servers.tab.mine">${L('servers.tab.mine')}</button>
        <button class="tab" data-srv-tab="browser" data-i18n="servers.tab.browser">${L('servers.tab.browser')}</button>
        <button class="tab" data-srv-tab="frp" data-i18n="servers.tab.frp">${L('servers.tab.frp')}</button>
      </div>

      <div data-srv-panel="mine">
        <div class="card tight" style="margin-bottom:12px">
          <div class="hstack" style="gap:10px;flex-wrap:wrap">
            <input type="text" id="srv-add-name" data-i18n-placeholder="servers.field.name" placeholder="${L('servers.field.name')}" style="min-width:120px">
            <input type="text" id="srv-add-address" data-i18n-placeholder="servers.field.address" placeholder="${L('servers.field.address')}" style="min-width:200px;flex:1">
            <input type="number" id="srv-add-port" value="25565" min="1" max="65535" title="${L('servers.field.port')}" style="width:96px">
            <button class="btn primary" data-srv-act="add" style="flex:none">${L('servers.btn.add')}</button>
            <button class="btn" data-srv-act="ping-mine" style="flex:none">${L('servers.btn.pingAll')}</button>
          </div>
          <div class="mini-note" style="margin-top:8px" data-i18n="servers.mine.hint">${L('servers.mine.hint')}</div>
        </div>
        <div class="row-list" id="srv-mine-list"></div>
      </div>

      <div data-srv-panel="browser" style="display:none">
        <div class="card tight" style="margin-bottom:12px">
          <div class="mini-note" data-i18n="servers.browser.notice">${L('servers.browser.notice')}</div>
          <div class="mini-note" style="margin-top:6px" data-i18n="servers.browser.hint">${L('servers.browser.hint')}</div>
          <button class="btn sm" style="margin-top:10px" data-srv-act="ping-browser">${L('servers.browser.pingAll')}</button>
        </div>
        <div class="row-list" id="srv-browser-list"></div>
      </div>

      <div data-srv-panel="frp" style="display:none">
        <div class="card">
          <div class="card-title"><span data-i18n="servers.frp.status">${L('servers.frp.status')}</span><span class="k" id="srv-frp-state">—</span></div>
          <div id="srv-frp-status" class="stack" style="gap:8px"></div>
          <div class="mini-note" style="margin-top:10px" data-i18n="servers.frp.hint">${L('servers.frp.hint')}</div>
          <div class="wrap-gap" style="margin-top:10px">
            <button class="btn" data-srv-act="frp-download">${L('servers.frp.btn.download')}</button>
            <button class="btn primary" data-srv-act="frp-start">${L('servers.frp.btn.start')}</button>
            <button class="btn danger" data-srv-act="frp-stop">${L('servers.frp.btn.stop')}</button>
            <button class="btn" data-srv-act="frp-refresh">${L('servers.frp.btn.refresh')}</button>
          </div>
        </div>
        <div class="card section-gap">
          <div class="card-title" data-i18n="servers.frp.form.title">${L('servers.frp.form.title')}</div>
          <div class="mini-note" id="srv-frp-form-sub"></div>
          <div class="grid-2" style="margin-top:10px">
            ${field('srv-frp-server-addr', 'servers.frp.form.serverAddr', 'text', '')}
            ${field('srv-frp-server-port', 'servers.frp.form.serverPort', 'number', 7000)}
            ${field('srv-frp-token', 'servers.frp.form.token', 'text', '')}
            ${field('srv-frp-remote-port', 'servers.frp.form.remotePort', 'number', 25565)}
            ${field('srv-frp-local-port', 'servers.frp.form.localPort', 'number', 25565)}
            <div class="field">
              <div class="lbl" data-i18n="servers.frp.form.protocol">${L('servers.frp.form.protocol')}</div>
              <select id="srv-frp-protocol">
                <option value="tcp">TCP</option>
                <option value="udp">UDP</option>
              </select>
            </div>
          </div>
          <button class="btn primary" style="margin-top:10px" data-srv-act="frp-save">${L('servers.frp.form.save')}</button>
        </div>
        <div class="card section-gap">
          <div class="card-title" data-i18n="servers.frp.log">${L('servers.frp.log')}</div>
          <pre id="srv-frp-log" style="max-height:220px;overflow:auto;background:var(--input-bg);border:1px solid var(--input-border);border-radius:var(--radius-md);padding:10px;font:12px var(--font-mono);white-space:pre-wrap;word-break:break-all"></pre>
        </div>
      </div>`;
  }

  // ---------------------------------------------------------------- 渲染

  function renderMine() {
    const box = document.getElementById('srv-mine-list');
    if (!box) return;
    if (!myServers.length) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${L('servers.empty.mine')}</span></div>`;
      return;
    }
    box.innerHTML = myServers
      .map((s) => {
        const lat = latency(s.lastPingMs);
        const players =
          s.playersOnline != null
            ? L2('servers.players', { online: s.playersOnline, max: '?' })
            : '';
        const motd = s.motd ? esc(String(s.motd).slice(0, 90)) : L('servers.motd.na');
        return `<div class="vrow" data-srv-id="${esc(s.id)}">
          <button class="btn sm icon" data-srv-act="pin" title="${L(s.pinned ? 'servers.btn.unpin' : 'servers.btn.pin')}">${s.pinned ? '★' : '☆'}</button>
          <div class="v-main">
            <div class="v-id" style="font-size:13px">${esc(s.name)}${s.pinned ? ' <span class="badge-pill accent">★</span>' : ''}</div>
            <div class="v-meta">
              <span class="mono-dim">${esc(s.address)}:${esc(s.port)}</span>
              ${players ? `<span class="tag">${players}</span>` : ''}
              <span class="mono-dim" style="max-width:46%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${motd}</span>
            </div>
            <div style="height:4px;border-radius:4px;background:var(--hover);margin-top:6px;overflow:hidden">
              <i style="display:block;height:100%;width:${lat.width}%;background:${lat.color};border-radius:4px"></i>
            </div>
          </div>
          <div class="v-actions" style="align-items:center">
            <span class="mono-dim" style="color:${lat.color};font-size:12px;white-space:nowrap">${esc(lat.text)}</span>
            <button class="btn sm primary" data-srv-act="join">${L('servers.btn.join')}</button>
            <button class="btn sm danger" data-srv-act="remove">${L('servers.btn.remove')}</button>
          </div>
        </div>`;
      })
      .join('');
  }

  function renderBrowser() {
    const box = document.getElementById('srv-browser-list');
    if (!box) return;
    if (!browser || !browser.items.length) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${L('servers.browser.empty')}</span></div>`;
      return;
    }
    box.innerHTML = browser.items
      .map((it) => {
        const k = keyOf(it);
        const p = browserPing[k];
        let lat;
        if (p === 'pinging') {
          lat = { color: 'var(--text-dim)', width: 30, text: GM.t('servers.ping.pinging') };
        } else if (p && p.ok) {
          lat = latency(p.latencyMs);
        } else if (p && p.error) {
          lat = { color: 'var(--text-dim)', width: 4, text: p.error };
        } else {
          lat = latency(null);
        }
        const tags = (it.tags || [])
          .map((g) => `<span class="tag">${esc(g)}</span>`)
          .join('');
        const players =
          p && p.playersOnline != null
            ? L2('servers.players', { online: p.playersOnline, max: p.playersMax == null ? '?' : p.playersMax })
            : '';
        return `<div class="vrow" data-srv-key="${esc(k)}">
          <div class="v-main">
            <div class="v-id" style="font-size:13px">${esc(it.name)} ${tags}</div>
            <div class="v-meta">
              <span class="mono-dim">${esc(it.address)}:${esc(it.port)}</span>
              ${players ? `<span class="tag">${players}</span>` : ''}
            </div>
            <div style="height:4px;border-radius:4px;background:var(--hover);margin-top:6px;overflow:hidden">
              <i style="display:block;height:100%;width:${lat.width}%;background:${lat.color};border-radius:4px"></i>
            </div>
          </div>
          <div class="v-actions" style="align-items:center">
            <span class="mono-dim" style="color:${lat.color};font-size:12px;white-space:nowrap">${esc(lat.text)}</span>
            <button class="btn sm" data-srv-act="browser-detail">${L('servers.browser.detail')}</button>
            <button class="btn sm primary" data-srv-act="browser-add">${L('servers.browser.add')}</button>
          </div>
        </div>`;
      })
      .join('');
  }

  function renderFrp() {
    const st = document.getElementById('srv-frp-status');
    if (st && frpStatus) {
      const running = !!frpStatus.running;
      st.innerHTML = `<div class="hstack" style="gap:10px">
          <span class="badge-pill ${running ? 'ok' : 'warn'}"><span class="d"></span>${L(running ? 'servers.frp.running' : 'servers.frp.stopped')}</span>
          ${running ? `<span class="mono-dim">${L2('servers.frp.pid', { pid: frpStatus.pid })}</span>` : ''}
        </div>
        <div class="mini-note">${L('servers.frp.binary')}：${L(frpStatus.binaryInstalled ? 'servers.frp.yes' : 'servers.frp.no')} · ${L('servers.frp.configFile')}：${L(frpStatus.configExists ? 'servers.frp.yes' : 'servers.frp.no')}</div>`;
      const chip = document.getElementById('srv-frp-state');
      if (chip) chip.textContent = running ? 'PID ' + frpStatus.pid : '—';
    }
    const log = document.getElementById('srv-frp-log');
    if (log) log.textContent = frpLogText || GM.t('servers.frp.logEmpty');
  }

  function fillFrpForm(cfg) {
    cfg = cfg || {};
    const set = (id, v) => {
      const el = document.getElementById(id);
      if (el) el.value = v == null ? '' : v;
    };
    set('srv-frp-server-addr', cfg.serverAddr || '');
    set('srv-frp-server-port', cfg.serverPort || 7000);
    set('srv-frp-token', cfg.token || '');
    set('srv-frp-remote-port', cfg.remotePort || 25565);
    set('srv-frp-local-port', cfg.localPort || 25565);
    const sel = document.getElementById('srv-frp-protocol');
    if (sel) sel.value = cfg.protocol === 'udp' ? 'udp' : 'tcp';
    const sub = document.getElementById('srv-frp-form-sub');
    if (sub) {
      sub.textContent = GM.t('servers.frp.form.sub', {
        localPort: cfg.localPort || 25565,
        remotePort: cfg.remotePort || 25565,
      });
    }
  }

  function switchTab(name) {
    if (!page) return;
    page.querySelectorAll('[data-srv-tab]').forEach((b) =>
      b.classList.toggle('on', b.dataset.srvTab === name),
    );
    page.querySelectorAll('[data-srv-panel]').forEach((p) => {
      p.style.display = p.dataset.srvPanel === name ? '' : 'none';
    });
    if (name === 'browser' && !browserPinged) pingBrowser();
    if (name === 'frp') refreshFrp();
  }

  // ---------------------------------------------------------------- 操作

  async function addMine() {
    const nameEl = document.getElementById('srv-add-name');
    const addrEl = document.getElementById('srv-add-address');
    const portEl = document.getElementById('srv-add-port');
    const name = ((nameEl && nameEl.value) || '').trim();
    const address = ((addrEl && addrEl.value) || '').trim();
    const port = parseInt((portEl && portEl.value) || '25565', 10) || 25565;
    if (!name || !address) {
      GM.toast(t('servers.toast.nameReq'), 'err');
      return;
    }
    myServers = (await GM.invoke('servers_add', { name, address, port })) || [];
    if (nameEl) nameEl.value = '';
    if (addrEl) addrEl.value = '';
    renderMine();
    GM.toast(t('servers.toast.added', { name }), 'ok');
  }

  async function pingMine() {
    GM.busy(true, t('servers.busy.ping'));
    try {
      myServers = (await GM.invoke('servers_ping_all')) || myServers;
      renderMine();
    } finally {
      GM.busy(false);
    }
  }

  async function togglePin(id) {
    const s = myServers.find((x) => x.id === id);
    if (!s) return;
    const next = !s.pinned;
    myServers = (await GM.invoke('servers_pin', { id, pinned: next })) || myServers;
    renderMine();
    GM.toast(t(next ? 'servers.toast.pinned' : 'servers.toast.unpinned'), 'ok');
  }

  function confirmRemove(id) {
    const s = myServers.find((x) => x.id === id);
    if (!s) return;
    GM.ui.openSheet(
      GM.ui.shell(
        L('servers.confirm.remove'),
        L2('servers.confirm.removeSub', { name: s.name }),
        '',
        `<button class="btn" data-sheet-close>${L('servers.confirm.cancel')}</button>
         <button class="btn danger" id="srv-remove-ok">${L('servers.confirm.ok')}</button>`,
      ),
    );
    const ok = document.getElementById('srv-remove-ok');
    if (ok) {
      ok.onclick = async () => {
        GM.ui.closeSheet();
        try {
          myServers = (await GM.invoke('servers_remove', { id })) || [];
          renderMine();
          GM.toast(t('servers.toast.removed'), 'ok');
        } catch (e) {
          GM.toast(t('servers.fail', { err: brief(e) }), 'err');
        }
      };
    }
  }

  async function join(id) {
    const s = myServers.find((x) => x.id === id);
    if (!s) return;
    const addr = s.address + ':' + s.port;
    try {
      await GM.invoke('servers_join', { address: s.address, port: s.port });
      GM.toast(t('servers.toast.joined', { addr }), 'ok');
    } catch (e) {
      // 命令未集成 / 还没启动过版本时回退为复制地址
      const copied = await copyText(addr);
      GM.toast(
        copied ? t('servers.toast.copy', { addr }) : t('servers.fail', { err: brief(e) }),
        copied ? 'ok' : 'err',
      );
    }
  }

  async function pingBrowser() {
    if (!browser) return;
    browserPinged = true;
    browser.items.forEach((it) => {
      browserPing[keyOf(it)] = 'pinging';
    });
    renderBrowser();
    await Promise.all(
      browser.items.map(async (it) => {
        const k = keyOf(it);
        try {
          browserPing[k] = await GM.invoke('servers_ping', {
            address: it.address,
            port: it.port,
          });
        } catch (e) {
          browserPing[k] = { ok: false, latencyMs: null, error: brief(e) };
        }
        renderBrowser();
      }),
    );
  }

  async function browserAdd(key) {
    const it = browser && browser.items.find((x) => keyOf(x) === key);
    if (!it) return;
    myServers = (await GM.invoke('servers_add', {
      name: it.name,
      address: it.address,
      port: it.port,
    })) || [];
    renderMine();
    GM.toast(t('servers.browser.added', { name: it.name }), 'ok');
  }

  async function browserDetail(key) {
    const it = browser && browser.items.find((x) => keyOf(x) === key);
    if (!it) return;
    let info = null;
    try {
      info = await GM.invoke('server_browser_info', { host: it.address });
    } catch (e) {
      info = null;
    }
    const body = info
      ? `<pre style="white-space:pre-wrap;word-break:break-word;font:12.5px var(--font-mono)">${esc(
          t('servers.browser.info', {
            desc: info.description || '-',
            ver: info.version || '-',
            online: info.playersOnline,
            max: info.playersMax,
          }),
        )}</pre>`
      : `<div class="mini-note">${L('servers.browser.detailNone')}</div>`;
    GM.ui.openSheet(
      GM.ui.shell(esc(it.name), esc(it.address + ':' + it.port), body,
        `<button class="btn" data-sheet-close>${L('servers.browser.close')}</button>`),
    );
  }

  async function frpDownload() {
    GM.busy(true, t('servers.frp.busy.download'));
    try {
      await GM.invoke('frp_download');
      await refreshFrp();
      GM.toast(t('servers.frp.toast.downloadOk'), 'ok');
    } finally {
      GM.busy(false);
    }
  }

  async function frpStart() {
    const pid = await GM.invoke('frp_start');
    await refreshFrp();
    GM.toast(t('servers.frp.toast.started', { pid: pid == null ? '?' : pid }), 'ok');
  }

  async function frpStop() {
    await GM.invoke('frp_stop');
    await refreshFrp();
    GM.toast(t('servers.frp.toast.stopped'), 'ok');
  }

  async function refreshFrp() {
    try {
      frpStatus = await GM.invoke('frp_status');
    } catch (e) {
      frpStatus = null;
    }
    try {
      frpLogText = (await GM.invoke('frp_log')) || '';
    } catch (e) {
      frpLogText = '';
    }
    renderFrp();
  }

  async function frpSave() {
    const val = (id) => {
      const el = document.getElementById(id);
      return ((el && el.value) || '').trim();
    };
    const sel = document.getElementById('srv-frp-protocol');
    const cfg = {
      serverAddr: val('srv-frp-server-addr'),
      serverPort: parseInt(val('srv-frp-server-port'), 10) || 7000,
      token: val('srv-frp-token'),
      remotePort: parseInt(val('srv-frp-remote-port'), 10) || 25565,
      localPort: parseInt(val('srv-frp-local-port'), 10) || 25565,
      protocol: (sel && sel.value) || 'tcp',
    };
    if (!cfg.serverAddr) {
      GM.toast(t('servers.frp.toast.cfgReq'), 'err');
      return;
    }
    frpCfg = await GM.invoke('frp_config_save', { cfg });
    fillFrpForm(frpCfg);
    await refreshFrp();
    GM.toast(t('servers.frp.toast.saved'), 'ok');
  }

  // ---------------------------------------------------------------- 数据加载

  async function loadMine() {
    try {
      myServers = (await GM.invoke('servers_list')) || [];
    } catch (e) {
      myServers = [];
    }
    renderMine();
  }

  async function loadBrowser() {
    try {
      browser = await GM.invoke('server_browser_list');
    } catch (e) {
      browser = { items: [], notice: '' };
    }
    browserPinged = false;
    renderBrowser();
  }

  async function loadFrp() {
    try {
      frpCfg = (await GM.invoke('frp_config_get')) || {};
    } catch (e) {
      frpCfg = {};
    }
    fillFrpForm(frpCfg);
    await refreshFrp();
  }

  async function loadAll() {
    await Promise.all([loadMine(), loadBrowser(), loadFrp()]);
  }

  // ---------------------------------------------------------------- 事件

  function bind() {
    page.addEventListener('click', async (e) => {
      const tab = e.target.closest('[data-srv-tab]');
      if (tab) {
        switchTab(tab.dataset.srvTab);
        return;
      }
      const btn = e.target.closest('[data-srv-act]');
      if (!btn) return;
      const act = btn.dataset.srvAct;
      const mineRow = e.target.closest('[data-srv-id]');
      const id = mineRow ? mineRow.dataset.srvId : null;
      const browRow = e.target.closest('[data-srv-key]');
      const key = browRow ? browRow.dataset.srvKey : null;
      try {
        if (act === 'add') await addMine();
        else if (act === 'ping-mine') await pingMine();
        else if (act === 'pin' && id) await togglePin(id);
        else if (act === 'remove' && id) confirmRemove(id);
        else if (act === 'join' && id) await join(id);
        else if (act === 'ping-browser') await pingBrowser();
        else if (act === 'browser-add' && key) await browserAdd(key);
        else if (act === 'browser-detail' && key) await browserDetail(key);
        else if (act === 'frp-download') await frpDownload();
        else if (act === 'frp-start') await frpStart();
        else if (act === 'frp-stop') await frpStop();
        else if (act === 'frp-refresh') await refreshFrp();
        else if (act === 'frp-save') await frpSave();
      } catch (err) {
        GM.toast(t('servers.fail', { err: brief(err) }), 'err');
      }
    });

    const addr = document.getElementById('srv-add-address');
    if (addr && !addr.dataset.bound) {
      addr.dataset.bound = '1';
      addr.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') addMine();
      });
    }
  }

  // ---------------------------------------------------------------- 钩子

  function ready() {
    GM.addNav({
      id: 'servers',
      label: 'nav.servers',
      title: GM.t('nav.servers'),
      icon:
        '<svg class="ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">' +
        '<rect x="2.2" y="2.6" width="11.6" height="4.2" rx="1"/><rect x="2.2" y="9.2" width="11.6" height="4.2" rx="1"/>' +
        '<path d="M4.6 4.7h.01M4.6 11.3h.01M7.4 4.7h3.2M7.4 11.3h3.2"/></svg>',
    });
    page = GM.addPage('servers', shellHtml());
    if (!page) return;
    if (!page.dataset.srvBound) {
      page.dataset.srvBound = '1';
      bind();
    }
    loadAll();
  }

  GM.register({
    ready,
    render() {
      if (!page) return;
      renderMine();
      renderBrowser();
      renderFrp();
    },
    nav(p) {
      if (p === 'servers') loadAll();
    },
  });
})();
