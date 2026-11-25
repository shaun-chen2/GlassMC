/* =====================================================================
 * features-autoupdate.js —— 自动更新（设置页卡片）
 *
 * 只往 #settings 幂等注入一张卡片，不新增 nav、不改 app.js：
 * - 开关 / 模式走 auto_config_set；
 * - 状态走 auto_status（纯本地读取，离线也能秒回）；
 * - 立即检查走 auto_check_now（失败返回 ok:false，不会把界面挂住）；
 * - 「安装并重启」走 auto_install_staged：macOS 替换 .app，Windows 静默跑安装包，
 *   Linux 打开安装包；也保留「打开安装包」给不想自动安装的用户。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = (window.GM = window.GM || {});
  const HOOKS = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });

  const t = (key, vars) => (typeof GM.t === 'function' ? GM.t('auto.' + key, vars) : key);
  const esc = (s) => (typeof GM.esc === 'function' ? GM.esc(s) : String(s == null ? '' : s));
  const toast = (m, k) => GM.toast(m, k);
  const invoke = (cmd, args) => GM.invoke(cmd, args || {});
  const firstLine = (e) => String((e && e.message) || e || '').split('\n')[0];

  let lastStatus = null;
  let refreshToken = 0;

  // ---------------------------------------------------------------- 卡片

  function cardHtml() {
    return (
      '<div class="card" id="auto-card" data-auto-card="1">' +
        '<div class="card-title" data-i18n="auto.title">自动更新</div>' +
        '<div class="desc" data-i18n="auto.desc">后台检查新版本；可以只提示、静默下载，或下载后自动安装并重启。</div>' +
        '<div class="field">' +
          '<label class="hstack" style="gap:8px">' +
            '<input type="checkbox" id="auto-enabled" data-i18n-aria-label="auto.enable">' +
            '<span data-i18n="auto.enable">启用自动更新检查</span>' +
          '</label>' +
          '<div class="desc" data-i18n="auto.enableDesc">关闭时不会联网检查，也不会下载任何文件。</div>' +
        '</div>' +
        '<div class="field">' +
          '<div class="lbl" data-i18n="auto.mode">发现新版本时</div>' +
          '<select id="auto-mode">' +
            '<option value="notify" data-i18n="auto.mode.notify">只提示我</option>' +
            '<option value="silent-download" data-i18n="auto.mode.download">静默下载安装包</option>' +
            '<option value="auto-install" data-i18n="auto.mode.install">自动安装并重启</option>' +
          '</select>' +
          '<div class="desc" data-i18n="auto.modeDesc">自动安装会替换当前程序并重启；macOS 需要当前 .app 可写，Windows 可能弹管理员确认，Linux 会打开安装包。</div>' +
        '</div>' +
        '<div class="field">' +
          '<div class="lbl" data-i18n="auto.status">状态</div>' +
          '<div class="desc" id="auto-status-line">—</div>' +
          '<div class="hstack" id="auto-staged" style="gap:8px;flex-wrap:wrap;margin-top:6px"></div>' +
        '</div>' +
        '<div class="hstack" style="gap:8px;flex-wrap:wrap">' +
          '<button class="btn primary" id="auto-check" data-i18n="auto.checkNow">立即检查</button>' +
          '<button class="btn" id="auto-install" style="display:none" data-i18n="auto.install">安装并重启</button>' +
          '<button class="btn" id="auto-open" style="display:none" data-i18n="auto.open">打开安装包</button>' +
        '</div>' +
      '</div>'
    );
  }

  function inject() {
    const page = document.querySelector('.page[data-page="settings"]');
    if (!page) return;
    let card = document.getElementById('auto-card');
    if (!card) {
      const grid = page.querySelector('.grid-2') || page;
      grid.insertAdjacentHTML('beforeend', cardHtml());
      card = document.getElementById('auto-card');
    }
    // data-i18n 节点由 i18n.apply(document) 统一刷新；这里只绑定一次事件。
    if (card && !card.dataset.bound) {
      card.dataset.bound = '1';
      bind(card);
    }
  }

  function bind(card) {
    const enabled = card.querySelector('#auto-enabled');
    if (enabled) enabled.addEventListener('change', saveConfig);
    const mode = card.querySelector('#auto-mode');
    if (mode) mode.addEventListener('change', saveConfig);
    const check = card.querySelector('#auto-check');
    if (check) check.addEventListener('click', () => checkNow(check));
    const install = card.querySelector('#auto-install');
    if (install) install.addEventListener('click', installNow);
    const open = card.querySelector('#auto-open');
    if (open) open.addEventListener('click', openStaged);
  }

  // ---------------------------------------------------------------- 渲染

  function formatTime(ts) {
    const n = Number(ts) || 0;
    if (n <= 0) return t('never');
    const d = new Date(n * 1000);
    if (isNaN(d.getTime())) return t('never');
    try {
      return d.toLocaleString();
    } catch (_) {
      return String(n);
    }
  }

  function formatSize(n) {
    const v = Number(n) || 0;
    if (v < 1024) return v + ' B';
    if (v < 1048576) return (v / 1024).toFixed(1) + ' KB';
    return (v / 1048576).toFixed(1) + ' MB';
  }

  function applyConfig(c) {
    const box = document.getElementById('auto-enabled');
    if (box && box.checked !== !!c.enabled) box.checked = !!c.enabled;
    const sel = document.getElementById('auto-mode');
    if (sel) {
      const next = c.mode === 'silent-download' ? 'silent-download' : 'notify';
      if (sel.value !== next) sel.value = next;
    }
  }

  function paintStatus(status) {
    lastStatus = status || null;
    const cfg = (status && status.config) || {};
    applyConfig(cfg);

    const line = document.getElementById('auto-status-line');
    if (line) {
      line.textContent = t('status.line', {
        last: formatTime(cfg.lastCheck),
        version: cfg.lastVersion || t('never'),
        failures: Number(cfg.failures) || 0,
      });
    }

    const staged = document.getElementById('auto-staged');
    const install = document.getElementById('auto-install');
    const open = document.getElementById('auto-open');
    if (staged) {
      if (status && status.stagedExists) {
        staged.innerHTML =
          '<span class="badge-pill">' + esc(t('staged.badge')) + '</span>' +
          '<span class="mono-dim" title="' + esc(status.stagedPath || '') + '">' +
          esc(status.stagedName || '') + ' · ' + esc(formatSize(status.stagedSize)) + '</span>';
        if (install) install.style.display = '';
        if (open) open.style.display = '';
      } else {
        staged.textContent = t('staged.none');
        if (install) install.style.display = 'none';
        if (open) open.style.display = 'none';
      }
    }
  }

  /** auto_status 不联网，每次 render 调也不会卡界面；token 防异步乱序。 */
  async function refresh() {
    const token = ++refreshToken;
    try {
      const s = await invoke('auto_status');
      if (token !== refreshToken) return;
      paintStatus(s);
    } catch (e) {
      console.debug('[autoupdate] status', e);
    }
  }

  // ---------------------------------------------------------------- 操作

  async function saveConfig() {
    const box = document.getElementById('auto-enabled');
    const sel = document.getElementById('auto-mode');
    if (!box || !sel) return;
    try {
      const next = await invoke('auto_config_set', {
        enabled: !!box.checked,
        mode: sel.value,
      });
      applyConfig(next || {});
      toast(t('toast.config'), 'ok');
    } catch (e) {
      toast(t('toast.configFail', { err: firstLine(e) }), 'err');
      refresh(); // 失败就回滚到后端真实值
    }
  }

  async function checkNow(btn) {
    if (!btn || btn.disabled) return;
    btn.disabled = true;
    GM.busy(true, t('checking'));
    try {
      const r = await invoke('auto_check_now');
      if (!r || !r.ok) {
        toast(t('toast.offline', { err: firstLine(r && r.message) }), 'err');
      } else if (r.downloaded) {
        toast(t('toast.staged', { version: r.latest || '' }), 'ok');
      } else if (r.skipped) {
        toast(t('toast.checkOk'), 'ok');
      } else if (!r.hasUpdate) {
        toast(t('toast.latest'), 'ok');
      } else {
        toast(t('toast.checkOk'), 'ok');
      }
      await refresh();
    } catch (e) {
      toast(t('toast.offline', { err: firstLine(e) }), 'err');
    } finally {
      GM.busy(false);
      btn.disabled = false;
    }
  }

  async function installNow(btn) {
    if (!btn || btn.disabled) return;
    const path = lastStatus && lastStatus.stagedPath;
    if (!path) {
      toast(t('toast.noStaged'), 'err');
      return;
    }
    if (!window.confirm(t('installConfirm'))) return;
    btn.disabled = true;
    GM.busy(true, t('installing'));
    try {
      const msg = await invoke('auto_install_staged');
      toast(msg || t('toast.installing'), 'ok');
      await refresh();
    } catch (e) {
      toast(t('toast.installFail', { err: firstLine(e) }), 'err');
    } finally {
      GM.busy(false);
      btn.disabled = false;
    }
  }

  function openStaged() {
    const path = lastStatus && lastStatus.stagedPath;
    if (!path) {
      toast(t('toast.noStaged'), 'err');
      return;
    }
    GM.invoke('open_path', { path }).catch((e) => {
      toast(t('toast.installFail', { err: firstLine(e) }), 'err');
    });
  }

  // ---------------------------------------------------------------- 钩子

  function ready() {
    inject();
    refresh();
  }

  function render() {
    inject();
    refresh();
  }

  if (typeof GM.register === 'function') {
    GM.register({ ready, render });
  } else {
    HOOKS.ready.push(ready);
    HOOKS.render.push(render);
  }

  // 语言切换：静态节点由 i18n.apply 负责，动态状态行在这里重绘。
  if (GM.i18n && typeof GM.i18n.onChange === 'function') {
    GM.i18n.onChange(() => {
      if (lastStatus) paintStatus(lastStatus);
    });
  }
})();
