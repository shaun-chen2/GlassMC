/* =============================================================================
   gm-ext.js —— 扩展模块的公共胶水
   -----------------------------------------------------------------------------
   新增功能只写自己的 features-*.js，通过 GM.register / GM.addNav / GM.addPage 接入，
   不需要改 app.js 或 index.html 的既有逻辑。加载顺序：features.js 之后。
   ========================================================================== */
(function () {
  'use strict';
  const GM = (window.GM = window.GM || {});
  const H = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });

  // app.js 会调这三个全局钩子；若 features.js 不在，这里兜底
  const safe = (fn) => (...args) => {
    try {
      return fn(...args);
    } catch (e) {
      console.error('[GM hook]', e);
      return undefined;
    }
  };
  if (!window.__GM_ON_READY) window.__GM_ON_READY = safe(() => H.ready.forEach((f) => safe(f)()));
  if (!window.__GM_ON_NAV) window.__GM_ON_NAV = safe((p) => H.nav.forEach((f) => safe(f)(p)));
  if (!window.__GM_ON_RENDER) window.__GM_ON_RENDER = safe(() => H.render.forEach((f) => safe(f)()));

  /** 注册生命周期钩子。ready 在首次 boot 后触发；render 在每次全量刷新后触发。 */
  GM.register = function ({ ready, render, nav } = {}) {
    if (typeof ready === 'function') H.ready.push(ready);
    if (typeof render === 'function') H.render.push(render);
    if (typeof nav === 'function') H.nav.push(nav);
  };

  /** 动态加侧栏入口。icon 是一段 <svg>…</svg>，label 同时作为 i18n key。 */
  GM.addNav = function ({ id, label, icon, title } = {}) {
    if (!id) return null;
    const sidebar = document.querySelector('.sidebar');
    if (!sidebar || sidebar.querySelector(`.nav-item[data-nav="${id}"]`)) {
      return sidebar && sidebar.querySelector(`.nav-item[data-nav="${id}"]`);
    }
    const btn = document.createElement('button');
    btn.className = 'nav-item';
    btn.dataset.nav = id;
    btn.setAttribute('aria-label', title || label || id);
    if (icon) btn.insertAdjacentHTML('beforeend', icon);
    const span = document.createElement('span');
    span.className = 'tx';
    span.textContent = GM.t ? GM.t(label || id) : label || id;
    if (label && GM.i18n) span.dataset.i18n = label;
    btn.appendChild(span);
    const grow = sidebar.querySelector('.grow');
    if (grow) sidebar.insertBefore(btn, grow);
    else sidebar.appendChild(btn);
    btn.onclick = () => GM.go(id);
    return btn;
  };

  /** 动态加页面（#content 下），返回 section 节点。 */
  GM.addPage = function (id, html) {
    const content = document.getElementById('content');
    if (!content) return null;
    let page = content.querySelector(`.page[data-page="${id}"]`);
    if (!page) {
      page = document.createElement('section');
      page.className = 'page';
      page.dataset.page = id;
      content.appendChild(page);
    }
    if (html != null) page.innerHTML = html;
    return page;
  };

  /** 简化 invoke：app.js 还没 boot 完时调用会给出清楚的报错。 */
  GM.invoke = function (cmd, args = {}) {
    if (!window.__GM || typeof window.__GM.invoke !== 'function') {
      return Promise.reject(new Error('后端尚未就绪：' + cmd));
    }
    return window.__GM.invoke(cmd, args);
  };

  GM.state = () => (window.__GM ? window.__GM.S : null);
  GM.go = (p) => window.__GM && window.__GM.go(p);
  GM.toast = (m, k) => {
    if (window.__GM && window.__GM.toast) window.__GM.toast(m, k);
    else console.log('[GlassMC]', m);
  };
  GM.renderAll = () => window.__GM && window.__GM.renderAll();
  GM.esc = (s) => (GM.ui ? GM.ui.escAttr(s) : String(s == null ? '' : s));
  GM.busy = (on, label) => (GM.ui ? (on ? GM.ui.showProgress(label || '处理中…') : GM.ui.hideProgress()) : null);
})();
