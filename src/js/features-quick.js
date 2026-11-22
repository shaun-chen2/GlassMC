/* =====================================================================
 * features-quick.js —— 快捷：一步启动、固定世界/服务器、Discord 状态
 *
 * 全部通过 gm-ext.js 的 GM.register / addNav / addPage / invoke / ... 接入，
 * 不改 app.js、index.html 既有逻辑（集成由 team-lead 补两个 script 标签）。
 *
 * 重要：不使用 game 参数 demo / quickPlay。
 *  - 固定世界：只启动对应版本，提示玩家进游戏后点「上次世界」；
 *  - 快速加入：先 quick_prepare 写 quick-pending，再 launch_game，
 *    真实参数由 launch.rs 读取 quick-pending 后追加（--server/--port）。
 * Discord 区域只显示状态，不声称「已连接」。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = window.GM;
  if (!GM) return;

  const t = (k, v) => GM.t('quick.' + k, v);
  const esc = (s) => GM.esc(s);
  const ICON =
    '<svg class="ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M9.3 1.8L3.6 9.1h3.6l-.7 5.1 5.9-7.4H8.7z"/></svg>';

  let page = null;
  let pins = [];
  let rpc = { enabled: false, connected: false, lastError: '' };

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  function installed() {
    const s = GM.state() || {};
    return Array.isArray(s.installed) ? s.installed : [];
  }

  function lastVersionId() {
    const s = GM.state() || {};
    const cfg = s.config || {};
    const ids = installed().map((v) => v.id);
    const prefer = cfg.lastVersion || s.selected;
    if (prefer && ids.indexOf(prefer) >= 0) return prefer;
    return ids[0] || '';
  }

  // ---------------------------------------------------------------- 页面骨架

  function buildPage() {
    if (!GM.addPage) return;
    page = GM.addPage('quick', `
      <div class="page-head">
        <h1 data-i18n="quick.title">快速游玩</h1>
        <div class="sub" data-i18n="quick.sub">一步启动、固定项与 Discord 状态</div>
      </div>
      <div class="grid-2">
        <div class="card">
          <div class="card-title" data-i18n="quick.play.title">一步启动</div>
          <div class="stack" style="gap:12px">
            <button class="btn primary lg" id="quick-last" disabled
                    data-i18n="quick.play.last">启动上次版本</button>
            <div class="mini-note" data-i18n="quick.play.worldHint">
              固定世界会启动对应版本；进入游戏后请点「上次世界」。
            </div>
            <div class="field" style="margin-top:2px">
              <div class="lbl" data-i18n="quick.play.serverLbl">快速加入服务器（可选）</div>
              <div class="hstack">
                <input type="text" id="quick-server"
                       placeholder="mc.example.com:25565"
                       data-i18n-placeholder="quick.play.serverPh">
                <button class="btn" id="quick-join" style="flex:none"
                        data-i18n="quick.play.join">连接并启动</button>
              </div>
            </div>
          </div>
        </div>
        <div class="card">
          <div class="card-title" data-i18n="quick.pin.title">固定项</div>
          <div class="field">
            <div class="lbl" data-i18n="quick.pin.add">添加固定项</div>
            <div class="hstack" style="gap:6px;flex-wrap:wrap">
              <select id="quick-pin-kind" style="flex:none">
                <option value="world" data-i18n="quick.pin.world">世界</option>
                <option value="server" data-i18n="quick.pin.server">服务器</option>
              </select>
              <input type="text" id="quick-pin-name" style="min-width:88px"
                     placeholder="名称" data-i18n-placeholder="quick.pin.namePh">
              <input type="text" id="quick-pin-version" style="min-width:96px"
                     placeholder="版本ID" data-i18n-placeholder="quick.pin.versionPh">
              <input type="text" id="quick-pin-ref" style="min-width:116px"
                     placeholder="标识/地址" data-i18n-placeholder="quick.pin.refPh">
              <button class="btn sm" id="quick-pin-add" style="flex:none"
                      data-i18n="quick.pin.confirm">添加</button>
            </div>
            <div class="desc" style="margin-top:6px" data-i18n="quick.pin.formHint">
              世界填存档名或留空；服务器填 address:port。版本留空则用上次版本。
            </div>
          </div>
          <div id="quick-pin-list" class="row-list"></div>
        </div>
      </div>
      <div class="card section-gap">
        <div class="card-title" data-i18n="quick.rpc.title">Discord 状态</div>
        <div class="stack" style="gap:10px">
          <label class="hstack" style="gap:8px;font-size:12.5px">
            <input type="checkbox" id="quick-rpc-enabled">
            <span data-i18n="quick.rpc.enabled">启用 Discord 状态同步</span>
          </label>
          <div class="field" style="margin-bottom:0">
            <div class="lbl" data-i18n="quick.rpc.details">显示内容</div>
            <div class="hstack">
              <input type="text" id="quick-rpc-details"
                     placeholder="正在玩 GlassMC"
                     data-i18n-placeholder="quick.rpc.detailsPh">
              <button class="btn" id="quick-rpc-save" style="flex:none"
                      data-i18n="quick.rpc.save">保存</button>
              <button class="btn" id="quick-rpc-test" style="flex:none"
                      data-i18n="quick.rpc.test">测试发送</button>
            </div>
          </div>
          <div class="mini-note" id="quick-rpc-status">—</div>
        </div>
      </div>
    `);
    if (!page) return;

    // 显示内容从本地记忆恢复（后端契约只暴露 enabled/connected/lastError）
    try {
      const de = document.getElementById('quick-rpc-details');
      if (de) de.value = localStorage.getItem('glassmc.quick.rpcDetails') || '';
    } catch (_) { /* localStorage 不可用不影响功能 */ }

    bind();
  }

  // ---------------------------------------------------------------- 动态渲染

  function renderPlay() {
    if (!page) return;
    const btn = document.getElementById('quick-last');
    if (!btn) return;
    const id = lastVersionId();
    btn.disabled = !id;
    btn.textContent = id ? t('play.last', { id }) : t('play.none');
  }

  function renderPins() {
    if (!page) return;
    const box = document.getElementById('quick-pin-list');
    if (!box) return;
    if (!pins.length) {
      box.innerHTML =
        '<div class="empty"><span class="big-ico">◌</span><span>' +
        esc(t('pin.empty')) + '</span></div>';
      return;
    }
    box.innerHTML = pins
      .map((p) => {
        const kind = p.kind === 'server' ? 'server' : 'world';
        const name = p.name || p.versionId || p.ref || '';
        const meta = [
          t('pin.kind.' + kind),
          p.versionId || '—',
          p.ref || '—',
        ].join(' · ');
        return (
          '<div class="vrow">' +
            '<div class="v-main">' +
              '<div class="v-id">' + esc(name) + '</div>' +
              '<div class="v-meta mono-dim">' + esc(meta) + '</div>' +
            '</div>' +
            '<div class="v-actions">' +
              '<button class="btn sm primary" data-pin-play="' + esc(p.id) + '" data-i18n="pin.play">快速游玩</button>' +
              '<button class="btn sm" data-pin-locate="' + esc(p.id) + '" data-i18n="pin.locate">定位</button>' +
              '<button class="btn sm" data-pin-remove="' + esc(p.id) + '" data-i18n="pin.remove">移除</button>' +
            '</div>' +
          '</div>'
        );
      })
      .join('');
  }

  function paintRpcStatus() {
    if (!page) return;
    const el = document.getElementById('quick-rpc-status');
    if (!el) return;
    const yes = t('yes');
    const no = t('no');
    let s = t('rpc.status', {
      enabled: rpc.enabled ? yes : no,
      connected: rpc.connected ? yes : no,
    });
    if (rpc.lastError) s += ' · ' + t('rpc.error', { err: rpc.lastError });
    el.textContent = s;
  }

  function syncRpcForm() {
    if (!page) return;
    const box = document.getElementById('quick-rpc-enabled');
    if (box) box.checked = !!rpc.enabled;
  }

  function paintDynamic() {
    renderPlay();
    renderPins();
    paintRpcStatus();
    syncRpcForm();
  }

  // ---------------------------------------------------------------- 数据

  async function loadData() {
    try {
      const [p, s] = await Promise.all([
        GM.invoke('pins_list'),
        GM.invoke('rpc_status'),
      ]);
      pins = Array.isArray(p) ? p : [];
      if (s && typeof s === 'object') rpc = s;
    } catch (e) {
      // 后端未就绪时保持默认值，不影响其它功能
    }
    paintDynamic();
  }

  // ---------------------------------------------------------------- 操作

  async function launch(versionId, server) {
    if (!versionId) {
      GM.toast(t('play.none'), 'err');
      return;
    }
    GM.busy(true, t('busy'));
    try {
      if (server) {
        // 只写待连接信息，不伪造 demo / quickPlay 参数
        await GM.invoke('quick_prepare', { versionId, server });
      }
      await GM.invoke('launch_game', { versionId });
      GM.toast(
        server ? t('play.launched', { id: versionId }) : t('play.worldLaunched', { id: versionId }),
        'ok',
      );
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      GM.busy(false);
    }
  }

  async function addPin() {
    const kindEl = document.getElementById('quick-pin-kind');
    const nameEl = document.getElementById('quick-pin-name');
    const verEl = document.getElementById('quick-pin-version');
    const refEl = document.getElementById('quick-pin-ref');
    const kind = (kindEl && kindEl.value) || 'world';
    const name = ((nameEl && nameEl.value) || '').trim() || kind;
    const versionId = ((verEl && verEl.value) || '').trim() || lastVersionId();
    const ref = ((refEl && refEl.value) || '').trim();
    if (!versionId) {
      GM.toast(t('pin.needVersion'), 'err');
      return;
    }
    try {
      pins = await GM.invoke('pin_add', { kind, versionId, name, reference: ref });
      if (nameEl) nameEl.value = '';
      if (refEl) refEl.value = '';
      renderPins();
      GM.toast(t('pin.added'), 'ok');
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  async function removePin(id) {
    try {
      pins = await GM.invoke('pin_remove', { id });
      renderPins();
      GM.toast(t('pin.removed'), 'ok');
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  async function saveRpc() {
    const en = document.getElementById('quick-rpc-enabled');
    const de = document.getElementById('quick-rpc-details');
    const enabled = !!(en && en.checked);
    const details = ((de && de.value) || '').trim() || t('rpc.defaultDetails');
    try {
      const cfg = await GM.invoke('rpc_config', { enabled, details });
      rpc.enabled = !!(cfg && cfg.enabled);
      try {
        localStorage.setItem('glassmc.quick.rpcDetails', details);
      } catch (_) { /* ignore */ }
      paintRpcStatus();
      GM.toast(t('rpc.saved'), 'ok');
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  async function testRpc() {
    const de = document.getElementById('quick-rpc-details');
    const details = ((de && de.value) || '').trim() || t('rpc.defaultDetails');
    GM.busy(true, t('busy'));
    try {
      rpc = await GM.invoke('rpc_send_activity', {
        details,
        state: t('rpc.defaultState'),
      });
      paintRpcStatus();
      GM.toast(t('rpc.sent'), 'ok');
    } catch (e) {
      GM.toast(t('fail', { err: brief(e) }), 'err');
    } finally {
      GM.busy(false);
    }
  }

  // ---------------------------------------------------------------- 事件

  function bind() {
    if (!page || page.dataset.quickBound) return;
    page.dataset.quickBound = '1';
    page.addEventListener('click', (e) => {
      const el = e.target;
      if (el.closest('#quick-last')) {
        launch(lastVersionId(), '');
        return;
      }
      if (el.closest('#quick-join')) {
        const inp = document.getElementById('quick-server');
        const server = ((inp && inp.value) || '').trim();
        if (!server) {
          GM.toast(t('play.serverEmpty'), 'err');
          return;
        }
        launch(lastVersionId(), server);
        return;
      }
      if (el.closest('#quick-pin-add')) {
        addPin();
        return;
      }
      const play = el.closest('[data-pin-play]');
      if (play) {
        const p = pins.find((x) => x.id === play.dataset.pinPlay);
        if (p) launch(p.versionId, p.kind === 'server' ? p.ref || '' : '');
        return;
      }
      const loc = el.closest('[data-pin-locate]');
      if (loc) {
        const p = pins.find((x) => x.id === loc.dataset.pinLocate);
        if (p) GM.go(p.kind === 'server' ? 'multiplayer' : 'versions');
        return;
      }
      const rm = el.closest('[data-pin-remove]');
      if (rm) {
        removePin(rm.dataset.pinRemove);
        return;
      }
      if (el.closest('#quick-rpc-save')) {
        saveRpc();
        return;
      }
      if (el.closest('#quick-rpc-test')) {
        testRpc();
      }
    });
  }

  // ---------------------------------------------------------------- 接入

  if (GM.register) {
    GM.register({
      ready() {
        if (GM.addNav) {
          GM.addNav({
            id: 'quick',
            label: 'nav.quick',
            title: GM.t('nav.quick'),
            icon: ICON,
          });
        }
        buildPage();
        loadData();
      },
      render() {
        paintDynamic();
      },
      nav(name) {
        if (name === 'quick') loadData();
      },
    });
  }
  if (GM.i18n && GM.i18n.onChange) {
    GM.i18n.onChange(() => paintDynamic());
  }
})();
