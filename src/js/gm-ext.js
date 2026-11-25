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

  // =================================================================== 侧栏分组
  // 功能补齐后侧栏有 17 个入口，直接铺开会把账号卡挤到滚动区外。这里按
  // 「游戏 / 内容 / 联机 / 工具 / 社区 / 系统」分组，支持折叠与搜索；
  // 窄屏（<900px）仍保持纯图标模式，分组头与搜索框自动隐藏。

  const NAV_GROUPS = [
    { key: 'play', label: 'html.nav.group.play', ids: ['home', 'versions', 'install'] },
    { key: 'content', label: 'html.nav.group.content', ids: ['mods', 'modsplus', 'packplus', 'saves'] },
    { key: 'online', label: 'html.nav.group.online', ids: ['multiplayer', 'servers'] },
    { key: 'tools', label: 'html.nav.group.tools', ids: ['quick', 'diagnostics', 'tweaks', 'security'] },
    { key: 'community', label: 'html.nav.group.community', ids: ['community'] },
    { key: 'system', label: 'html.nav.group.system', ids: ['import', 'accounts', 'settings'] },
  ];
  const NAV_COLLAPSE_KEY = 'glassmc.nav.collapsed.v1';
  const NAV_KNOWN = new Set(NAV_GROUPS.flatMap((g) => g.ids));

  const navLabel = (key, fallback) => (GM.t ? GM.t(key) : fallback);
  const navInput = () => document.getElementById('nav-search-input');

  function loadCollapsed() {
    try {
      const raw = localStorage.getItem(NAV_COLLAPSE_KEY);
      if (raw) return JSON.parse(raw) || {};
    } catch (e) {
      /* localStorage 不可用时按默认展开 */
    }
    // 首次进入：窗口偏矮时先收起「工具 / 社区」，避免一上来就出现滚动条
    return window.innerHeight < 800 ? { tools: true, community: true } : {};
  }

  function saveCollapsed(state) {
    try {
      localStorage.setItem(NAV_COLLAPSE_KEY, JSON.stringify(state || {}));
    } catch (e) {
      /* 忽略 */
    }
  }

  function groupHead(group) {
    return group.querySelector('.nav-group-head');
  }

  function ensureGroup(scroll, key, labelKey) {
    let group = scroll.querySelector(`.nav-group[data-group="${key}"]`);
    if (group) return group;
    group = document.createElement('div');
    group.className = 'nav-group';
    group.dataset.group = key;
    group.innerHTML =
      '<button class="nav-group-head" type="button" aria-expanded="true">' +
      '<span class="g-label"></span><span class="g-count"></span>' +
      '<svg class="g-chev" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" ' +
      'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 4.6L6 7.6l3-3"/></svg>' +
      '</button>';
    group.dataset.labelKey = labelKey;
    scroll.appendChild(group);
    return group;
  }

  function bindGroupHead(group) {
    const head = groupHead(group);
    if (!head || head.dataset.bound) return;
    head.dataset.bound = '1';
    head.addEventListener('click', () => {
      const next = !group.classList.contains('collapsed');
      group.classList.toggle('collapsed', next);
      head.setAttribute('aria-expanded', String(!next));
      const state = loadCollapsed();
      state[group.dataset.group] = next;
      saveCollapsed(state);
    });
  }

  function ensureSearch(scroll) {
    let box = document.getElementById('nav-search-box');
    if (!box) {
      box = document.createElement('div');
      box.className = 'nav-search';
      box.id = 'nav-search-box';
      box.innerHTML =
        '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" ' +
        'stroke-linecap="round" aria-hidden="true"><circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L14 14"/></svg>' +
        '<input id="nav-search-input" type="search" autocomplete="off" spellcheck="false">' +
        '<kbd id="nav-search-kbd"></kbd>';
      scroll.insertBefore(box, scroll.firstChild);
    }
    const input = box.querySelector('input');
    const kbd = box.querySelector('kbd');
    input.placeholder = navLabel('html.nav.search', '搜索功能');
    input.setAttribute('aria-label', navLabel('html.nav.search', '搜索功能'));
    if (kbd) kbd.textContent = document.documentElement.dataset.os === 'windows' ? 'Ctrl K' : '⌘K';
    if (!input.dataset.bound) {
      input.dataset.bound = '1';
      input.addEventListener('input', () => filterNav(input.value));
      input.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          input.value = '';
          filterNav('');
          input.blur();
        }
      });
      document.addEventListener(
        'keydown',
        (e) => {
          if ((e.metaKey || e.ctrlKey) && String(e.key).toLowerCase() === 'k') {
            e.preventDefault();
            input.focus();
            input.select();
          }
        },
        true
      );
    }
    return box;
  }

  function filterNav(value) {
    const scroll = document.getElementById('nav-scroll');
    if (!scroll) return;
    const q = String(value || '').trim().toLowerCase();
    scroll.classList.toggle('nav-filtering', !!q);
    let any = false;
    scroll.querySelectorAll('.nav-group').forEach((group) => {
      let visible = 0;
      group.querySelectorAll('.nav-item').forEach((item) => {
        const hit = !q || (item.dataset.nav + ' ' + item.textContent).toLowerCase().includes(q);
        item.classList.toggle('nav-hide', !hit);
        if (hit) visible += 1;
      });
      const hasItems = group.querySelectorAll('.nav-item').length > 0;
      group.classList.toggle('nav-hide', q ? visible === 0 : !hasItems);
      any = any || visible > 0;
    });
    let empty = document.getElementById('nav-empty');
    if (!empty) {
      empty = document.createElement('div');
      empty.id = 'nav-empty';
      empty.className = 'nav-empty';
      empty.textContent = navLabel('html.nav.noMatch', '没有匹配的功能');
      scroll.appendChild(empty);
    }
    empty.textContent = navLabel('html.nav.noMatch', '没有匹配的功能');
    empty.classList.toggle('nav-hide', !q || any);
  }

  function organizeSidebar() {
    const sidebar = document.querySelector('.sidebar');
    if (!sidebar) return;
    const scroll = sidebar.querySelector('#nav-scroll') || sidebar;
    const grow = scroll.querySelector('.grow');
    ensureSearch(scroll);

    // 先把分组容器按固定顺序建好；搜索框始终在最上面
    const groups = new Map();
    NAV_GROUPS.forEach((def) => {
      const group = ensureGroup(scroll, def.key, def.label);
      groups.set(def.key, group);
      bindGroupHead(group);
    });

    // 再按定义顺序把入口搬进对应分组，不依赖各模块的加载顺序
    NAV_GROUPS.forEach((def) => {
      const group = groups.get(def.key);
      def.ids.forEach((id) => {
        const item = sidebar.querySelector(`.nav-item[data-nav="${id}"]`);
        if (item) group.appendChild(item);
      });
    });

    // 未来新增的、还没归类的入口统一放进「更多」
    const unknowns = [...sidebar.querySelectorAll('.nav-item')].filter((it) => !NAV_KNOWN.has(it.dataset.nav));
    let more = null;
    if (unknowns.length) {
      more = ensureGroup(scroll, 'more', 'html.nav.group.more');
      bindGroupHead(more);
      groups.set('more', more);
      unknowns.forEach((it) => more.appendChild(it));
    }

    // 更新文案、数量与折叠状态
    const collapsed = loadCollapsed();
    groups.forEach((group, key) => {
      const label = group.querySelector('.g-label');
      const count = group.querySelector('.g-count');
      const head = groupHead(group);
      const def = NAV_GROUPS.find((g) => g.key === key);
      if (label) label.textContent = navLabel(def ? def.label : 'html.nav.group.more', key);
      const n = group.querySelectorAll('.nav-item').length;
      if (count) count.textContent = String(n);
      group.classList.toggle('nav-hide', n === 0);
      const isCollapsed = !!collapsed[key];
      group.classList.toggle('collapsed', isCollapsed);
      if (head) head.setAttribute('aria-expanded', String(!isCollapsed));
    });

    // 给图标模式补原生 tooltip；自动生成的 title 要跟着语言切换更新
    sidebar.querySelectorAll('.nav-item').forEach((item) => {
      const tx = item.querySelector('.tx');
      if (!item.title || item.dataset.navTitleAuto === '1') {
        if (tx && tx.textContent.trim()) {
          item.title = tx.textContent.trim();
          item.dataset.navTitleAuto = '1';
        }
      }
    });

    if (grow) scroll.appendChild(grow);
    const box = document.getElementById('nav-search-box');
    if (box && scroll.firstChild !== box) scroll.insertBefore(box, scroll.firstChild);
    syncActive();
    if (navInput() && navInput().value) filterNav(navInput().value);
  }

  let organizeTick = 0;
  function scheduleOrganize() {
    if (organizeTick) return;
    const run = () => {
      organizeTick = 0;
      organizeSidebar();
    };
    organizeTick = window.requestAnimationFrame ? window.requestAnimationFrame(run) : setTimeout(run, 16);
  }

  function syncActive() {
    document.querySelectorAll('.nav-group').forEach((group) => {
      group.classList.toggle('has-active', !!group.querySelector('.nav-item.active'));
    });
  }

  function revealActive(page) {
    // 从搜索结果里点进某个页面后，清掉过滤，回到完整导航
    const input = navInput();
    if (input && input.value) {
      input.value = '';
      filterNav('');
    }
    const item = document.querySelector(`.nav-item[data-nav="${page}"]`);
    if (item) {
      const group = item.closest('.nav-group');
      if (group && group.classList.contains('collapsed')) {
        group.classList.remove('collapsed');
        const head = groupHead(group);
        if (head) head.setAttribute('aria-expanded', 'true');
        const state = loadCollapsed();
        state[group.dataset.group] = false;
        saveCollapsed(state);
      }
    }
    syncActive();
  }

  // 语言切换后分组标题、搜索占位符要跟着变
  if (GM.i18n && typeof GM.i18n.onChange === 'function') {
    GM.i18n.onChange(() => organizeSidebar());
  }

  GM.register({ nav: revealActive });

  // DOM 已就绪时立即分组；否则等 DOMContentLoaded（脚本在 body 末尾，通常已就绪）
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', scheduleOrganize, { once: true });
  } else {
    scheduleOrganize();
  }

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
    if (label) btn.dataset.i18nAriaLabel = label;
    btn.setAttribute('aria-label', GM.t ? GM.t(label || id) : title || label || id);
    if (icon) btn.insertAdjacentHTML('beforeend', icon);
    const span = document.createElement('span');
    span.className = 'tx';
    span.textContent = GM.t ? GM.t(label || id) : label || id;
    if (label && GM.i18n) span.dataset.i18n = label;
    btn.appendChild(span);
    const grow = sidebar.querySelector('.grow');
    // grow 现在位于 #nav-scroll 内部，必须插到它的父节点，不能再直接用 sidebar
    const host = grow && grow.parentNode ? grow.parentNode : sidebar;
    if (grow) host.insertBefore(btn, grow);
    else host.appendChild(btn);
    btn.onclick = () => GM.go(id);
    scheduleOrganize();
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
