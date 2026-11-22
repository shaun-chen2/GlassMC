/* =====================================================================
 * features-accessibility.js —— 界面补齐：背景预览 / 无障碍 / 快捷键
 *
 * 与 features-prefs.js 的分工：
 * - 背景/音效的「选择 / 清除 / 透明度 / 试听 / 语言」已由 features-prefs 接管，
 *   这里不重复造控件，只补它缺的「卡片内实时预览」；
 * - 快捷键里的 1-9 / Enter / F / Esc 也已有实现，这里用 e.defaultPrevented
 *   让开，只补 L / , / / / Shift+R 与一个兜底 Esc；
 * - 本文件只往 #settings 幂等注入卡片，且只写一个 <style id="gm-a11y-style">，
 *   不改 app.css，也不改任何现有文件。
 * ===================================================================== */
(function () {
  'use strict';

  const GM = (window.GM = window.GM || {});
  const HOOKS = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });
  const $ = (sel, root) => (root || document).querySelector(sel);
  const $$ = (sel, root) => Array.from((root || document).querySelectorAll(sel));
  const t = (key, vars) => GM.t(key, vars);
  const esc = (s) => (GM.ui && GM.ui.escAttr ? GM.ui.escAttr(s) : String(s == null ? '' : s));
  const toast = (msg, kind) => GM.toast(msg, kind);
  const invoke = (cmd, args) => GM.invoke(cmd, args);
  const cfg = () => {
    const S = typeof GM.state === 'function' ? GM.state() : null;
    return (S && S.config) || {};
  };
  const backendReady = () => !!(window.__GM && typeof window.__GM.invoke === 'function');

  const BG_MAX_BYTES = 16 * 1024 * 1024; // 与 features-prefs.js 保持一致
  const STYLE_ID = 'gm-a11y-style';
  const LS_KEY = 'glassmc.a11y';
  const DEFAULTS = { largeText: false, highContrast: false, reduceMotion: false };

  let prefs = loadPrefs();
  let bgToken = 0; // 背景异步读取，连续变化时只认最后一次

  // ---------------------------------------------------------------- 偏好存取

  function loadPrefs() {
    try {
      return Object.assign({}, DEFAULTS, JSON.parse(localStorage.getItem(LS_KEY) || '{}'));
    } catch (e) {
      return Object.assign({}, DEFAULTS);
    }
  }

  function savePrefs() {
    try {
      localStorage.setItem(LS_KEY, JSON.stringify(prefs));
    } catch (e) {
      /* 隐私模式下 localStorage 可能不可写，忽略即可 */
    }
  }

  function applyPrefs() {
    const b = document.body;
    if (!b) return;
    b.classList.toggle('gm-large-text', !!prefs.largeText);
    b.classList.toggle('gm-high-contrast', !!prefs.highContrast);
    b.classList.toggle('gm-reduce-motion', !!prefs.reduceMotion);
  }

  // ---------------------------------------------------------------- 作用域 CSS

  const CSS = `
/* --- 键盘焦点：高对比描边，Tab 导航时清晰可见 --- */
body :focus-visible {
  outline: 3px solid #7DD3FC !important;
  outline-offset: 2px !important;
}

/* --- 减少动画：跟随系统，或被手动开关强制 --- */
@media (prefers-reduced-motion: reduce) {
  body *, body *::before, body *::after {
    animation-duration: .001ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: .001ms !important;
    scroll-behavior: auto !important;
  }
}
body.gm-reduce-motion *, body.gm-reduce-motion *::before, body.gm-reduce-motion *::after {
  animation-duration: .001ms !important;
  animation-iteration-count: 1 !important;
  transition-duration: .001ms !important;
  scroll-behavior: auto !important;
}

/* --- 大字体：必须 !important，因为 app.css 里大多是固定 px --- */
body.gm-large-text .nav-item { font-size: 15px !important; height: 42px !important; }
body.gm-large-text .page-head h1 { font-size: 24px !important; }
body.gm-large-text .card-title { font-size: 15px !important; }
body.gm-large-text .lbl { font-size: 14px !important; }
body.gm-large-text .desc,
body.gm-large-text .mini-note { font-size: 12.5px !important; }
body.gm-large-text .btn { font-size: 13px !important; }
body.gm-large-text .btn.sm { font-size: 12.5px !important; }
body.gm-large-text .v-id { font-size: 14px !important; }
body.gm-large-text .key-list { font-size: 13.5px !important; }
body.gm-large-text input,
body.gm-large-text select,
body.gm-large-text textarea { font-size: 14px !important; }

/* --- 高对比：覆盖主题变量，文字/边框拉到可读级别 --- */
body.gm-high-contrast {
  --shell-rgb: 0, 0, 0;
  --shell-base: 1;
  --sidebar-rgb: 0, 0, 0;
  --sidebar-base: 1;
  --card-bg: rgba(0, 0, 0, .92);
  --card-bg-hi: rgba(0, 0, 0, .96);
  --card-border: #B9C9DE;
  --card-border-hi: #FFFFFF;
  --text: #FFFFFF;
  --text-dim: #F0F6FF;
  --text-dimmer: #D6E2F2;
  --input-bg: rgba(0, 0, 0, .95);
  --input-border: #B9C9DE;
  --hover: rgba(255, 255, 255, .14);
  --active: rgba(154, 216, 255, .24);
  --accent-hi: #9AD8FF;
  --sheet-bg: #05080E;
  --sheet-mask: rgba(0, 0, 0, .82);
}
body.gm-high-contrast .card,
body.gm-high-contrast .vrow,
body.gm-high-contrast .btn,
body.gm-high-contrast .badge-pill,
body.gm-high-contrast input,
body.gm-high-contrast select,
body.gm-high-contrast textarea {
  border-color: #B9C9DE !important;
}

/* --- 跳到主内容：平时藏在屏幕外，聚焦时滑入 --- */
a#gm-skip-link {
  position: fixed;
  top: -80px;
  left: 16px;
  z-index: 2147483000;
  padding: 9px 15px;
  border-radius: 10px;
  background: var(--accent-hi, #93C7F0);
  color: #06121F;
  font-size: 12.5px;
  font-weight: 650;
  text-decoration: none;
  box-shadow: var(--shadow-pop, 0 12px 30px rgba(0, 0, 0, .4));
  transition: top .15s ease;
}
a#gm-skip-link:focus { top: 10px; }

/* --- 背景预览缩略图 --- */
.gm-bg-preview {
  height: 120px;
  border-radius: 12px;
  border: 1px solid var(--card-border);
  background-color: rgba(0, 0, 0, .18);
  background-size: cover;
  background-position: center;
  background-repeat: no-repeat;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.gm-bg-preview .gm-empty {
  font-size: 12px;
  color: var(--text-dimmer);
}
`;

  function ensureStyle() {
    let style = document.getElementById(STYLE_ID);
    if (!style) {
      style = document.createElement('style');
      style.id = STYLE_ID;
      document.head.appendChild(style);
    }
    if (style.textContent !== CSS) style.textContent = CSS;
  }

  // ---------------------------------------------------------------- 跳到主内容

  function ensureSkipLink() {
    let link = document.getElementById('gm-skip-link');
    if (!link) {
      link = document.createElement('a');
      link.id = 'gm-skip-link';
      link.href = '#content';
      link.setAttribute('data-i18n', 'a11y.skip');
      link.textContent = t('a11y.skip');
      link.onclick = (e) => {
        e.preventDefault();
        const content = document.getElementById('content');
        if (!content) return;
        if (content.tabIndex < 0) content.tabIndex = -1; // <main> 默认不可聚焦
        content.focus();
        content.scrollTop = 0;
      };
      document.body.appendChild(link);
    }
    // 文案与 i18n.apply 保持一致；onChange 里也会再刷一次
    link.textContent = t('a11y.skip');
    return link;
  }

  // ---------------------------------------------------------------- 动态按钮读屏标签

  /** 清掉本模块加过的标签。i18n.apply 先跑，所以这里再按新文案重算一遍。 */
  function clearA11yLabels() {
    $$('[data-gm-a11y-labeled]').forEach((btn) => {
      btn.removeAttribute('data-gm-a11y-labeled');
      btn.removeAttribute('aria-label');
      if (btn.getAttribute('data-gm-a11y-titled') === '1') {
        btn.removeAttribute('title');
        btn.removeAttribute('data-gm-a11y-titled');
      }
    });
  }

  function improveButtonLabels(root) {
    $$('button', root || document).forEach((btn) => {
      if (btn.hasAttribute('data-gm-a11y-labeled')) return;
      if (btn.hasAttribute('aria-label') || btn.hasAttribute('data-i18n-aria-label')) return;

      const raw = btn.getAttribute('title') || btn.textContent || btn.id || '';
      const label = String(raw).replace(/^btn-/, '').replace(/[_-]+/g, ' ').replace(/\s+/g, ' ').trim();
      btn.setAttribute('data-gm-a11y-labeled', '1');
      if (label) btn.setAttribute('aria-label', label);
      // 纯图标按钮没有可读文字，补一个 title 当悬浮提示；有文字的不加，免得满屏 tooltip
      if (!btn.textContent.trim() && !btn.hasAttribute('title') && label) {
        btn.setAttribute('title', label);
        btn.setAttribute('data-gm-a11y-titled', '1');
      }
    });
  }

  // ---------------------------------------------------------------- 设置页卡片

  function settingsRoot() {
    return document.getElementById('settings') || $('.page[data-page="settings"]');
  }

  function insertAfter(node, ref) {
    if (!ref || !ref.parentNode) return;
    ref.parentNode.insertBefore(node, ref.nextSibling);
  }

  /** 预览卡片：features-prefs 只把背景铺到 #bg-layer，这里补一张卡片内的实时缩略图。 */
  function injectPreviewCard(settings) {
    let card = document.getElementById('gm-bg-preview-card');
    if (!card) {
      card = document.createElement('div');
      card.id = 'gm-bg-preview-card';
      card.className = 'card section-gap';
      card.setAttribute('data-gm-a11y-card', '1');
      card.innerHTML =
        '<div class="card-title"><span data-i18n="a11y.preview.title">' + esc(t('a11y.preview.title')) + '</span>' +
        '<span class="k" id="gm-bg-preview-state">' + esc(t('a11y.preview.off')) + '</span></div>' +
        '<div class="gm-bg-preview" id="gm-bg-preview" role="img">' +
        '<span class="gm-empty" id="gm-bg-preview-empty" data-i18n="a11y.preview.empty">' + esc(t('a11y.preview.empty')) + '</span>' +
        '</div>' +
        '<div class="mini-note" style="margin-top:6px" data-i18n="a11y.preview.hint">' + esc(t('a11y.preview.hint')) + '</div>' +
        '<div class="hstack" style="margin-top:8px">' +
        '<button class="btn sm" id="gm-bg-preview-refresh" data-i18n="a11y.preview.refresh">' + esc(t('a11y.preview.refresh')) + '</button>' +
        '</div>';
      const personal = $('#set-language');
      const anchor = (personal && personal.closest('.card')) || settings.lastElementChild;
      insertAfter(card, anchor);

      const refresh = document.getElementById('gm-bg-preview-refresh');
      if (refresh) refresh.onclick = () => refreshPreview();
    }
    return card;
  }

  /** 无障碍卡片：三个开关 + 快捷键说明，状态存 localStorage。 */
  function injectA11yCard(settings, after) {
    let card = document.getElementById('gm-a11y-card');
    if (!card) {
      card = document.createElement('div');
      card.id = 'gm-a11y-card';
      card.className = 'card section-gap';
      card.setAttribute('data-gm-a11y-card', '1');

      const toggle = (id, key) =>
        '<liquid-glass-toggle id="' + id + '" size="sm"' +
        ' data-i18n-aria-label="a11y.toggle.' + key + '"' +
        ' aria-label="' + esc(t('a11y.toggle.' + key)) + '"></liquid-glass-toggle>';
      const row = (id, key) =>
        '<div class="field">' +
        '<div class="hstack" style="gap:10px;align-items:center">' +
        toggle(id, key) +
        '<span class="lbl" style="margin:0" data-i18n="a11y.toggle.' + key + '">' + esc(t('a11y.toggle.' + key)) + '</span>' +
        '</div>' +
        '<div class="desc" data-i18n="a11y.toggle.' + key + '.desc">' + esc(t('a11y.toggle.' + key + '.desc')) + '</div>' +
        '</div>';

      card.innerHTML =
        '<div class="card-title" data-i18n="a11y.card.title">' + esc(t('a11y.card.title')) + '</div>' +
        '<div class="desc" style="margin-top:0" data-i18n="a11y.card.sub">' + esc(t('a11y.card.sub')) + '</div>' +
        '<div class="grid-2">' +
        '<div class="field">' +
        '<div class="lbl" data-i18n="a11y.shortcuts.title">' + esc(t('a11y.shortcuts.title')) + '</div>' +
        '<ul class="key-list">' +
        '<li><kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>1</kbd>…<kbd>9</kbd> <span data-i18n="a11y.shortcut.nav">' + esc(t('a11y.shortcut.nav')) + '</span></li>' +
        '<li><kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>L</kbd> <span data-i18n="a11y.shortcut.launch">' + esc(t('a11y.shortcut.launch')) + '</span></li>' +
        '<li><kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>,</kbd> <span data-i18n="a11y.shortcut.settings">' + esc(t('a11y.shortcut.settings')) + '</span></li>' +
        '<li><kbd>/</kbd> <span data-i18n="a11y.shortcut.search">' + esc(t('a11y.shortcut.search')) + '</span></li>' +
        '<li><kbd>Esc</kbd> <span data-i18n="a11y.shortcut.escape">' + esc(t('a11y.shortcut.escape')) + '</span></li>' +
        '<li><kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>⇧</kbd> + <kbd>R</kbd> <span data-i18n="a11y.shortcut.rerender">' + esc(t('a11y.shortcut.rerender')) + '</span></li>' +
        '</ul>' +
        '</div>' +
        '<div>' +
        '<div class="lbl" data-i18n="a11y.display.title">' + esc(t('a11y.display.title')) + '</div>' +
        row('gm-a11y-large', 'largeText') +
        row('gm-a11y-contrast', 'highContrast') +
        row('gm-a11y-motion', 'reduceMotion') +
        '</div>' +
        '</div>';
      insertAfter(card, after || settings.lastElementChild);

      const bindToggle = (id, key) => {
        const el = document.getElementById(id);
        if (!el) return;
        el.addEventListener('change', (e) => {
          prefs[key] = !!(e.detail && e.detail.checked);
          savePrefs();
          applyPrefs();
          toast(t('a11y.toast.saved'), 'ok');
        });
      };
      bindToggle('gm-a11y-large', 'largeText');
      bindToggle('gm-a11y-contrast', 'highContrast');
      bindToggle('gm-a11y-motion', 'reduceMotion');
    }
    return card;
  }

  function injectCards() {
    const settings = settingsRoot();
    if (!settings) return;
    const preview = injectPreviewCard(settings);
    injectA11yCard(settings, preview);
  }

  function refreshA11yCard() {
    const set = (id, on) => {
      const el = document.getElementById(id);
      if (el) el.checked = !!on;
    };
    set('gm-a11y-large', prefs.largeText);
    set('gm-a11y-contrast', prefs.highContrast);
    set('gm-a11y-motion', prefs.reduceMotion);
  }

  // ---------------------------------------------------------------- 背景预览刷新

  /** 走 media_data_url 读 data URL 铺到缩略图；路径为空时清空。 */
  async function refreshPreview() {
    const box = document.getElementById('gm-bg-preview');
    if (!box) return;
    const empty = document.getElementById('gm-bg-preview-empty');
    const state = document.getElementById('gm-bg-preview-state');
    const path = cfg().backgroundImage || '';
    const token = ++bgToken;

    box.setAttribute('aria-label', t('a11y.preview.alt'));
    if (!path) {
      box.style.backgroundImage = '';
      if (empty) empty.style.display = '';
      if (state) state.textContent = t('a11y.preview.off');
      return;
    }
    if (!backendReady()) return; // 首帧后端还没就绪，ready 钩子会再刷一次

    try {
      const url = await invoke('media_data_url', { path, maxBytes: BG_MAX_BYTES });
      if (token !== bgToken) return;
      box.style.backgroundImage = url ? 'url("' + url + '")' : '';
      if (empty) empty.style.display = url ? 'none' : '';
      if (state) state.textContent = t(url ? 'a11y.preview.on' : 'a11y.preview.failed');
    } catch (e) {
      if (token !== bgToken) return;
      box.style.backgroundImage = '';
      if (empty) empty.style.display = '';
      if (state) state.textContent = t('a11y.preview.failed');
    }
  }

  // ---------------------------------------------------------------- 快捷键

  function isTyping(el) {
    if (!el) return false;
    const tag = el.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable === true;
  }

  function focusSearch() {
    const selectors = ['#remote-filter', '#mods-filter', '#mr-filter', '#srv-name'];
    for (const sel of selectors) {
      const input = $(sel);
      // 只认当前可见页面里的搜索框
      if (input && input.getClientRects().length > 0) {
        input.focus();
        if (input.select) input.select();
        return true;
      }
    }
    return false;
  }

  function onKeydown(e) {
    // Esc 例外：输入框里也要能关弹层（与 features-prefs 的捕获监听一致，兜底用）
    if (e.key === 'Escape') {
      const modal = document.getElementById('modal');
      if (modal && modal.classList.contains('on')) {
        e.preventDefault();
        e.stopPropagation();
        if (GM.ui && GM.ui.closeSheet) GM.ui.closeSheet();
      }
      return;
    }

    if (e.repeat || e.defaultPrevented || isTyping(e.target)) return;

    const meta = e.metaKey || e.ctrlKey;
    const key = e.key;

    // "/" 聚焦搜索框（不需要修饰键）
    if (key === '/' && !meta) {
      if (focusSearch()) e.preventDefault();
      return;
    }
    if (!meta) return;

    // 1-9 切页：features-prefs 已经处理并 preventDefault，这里让开
    if (/^[1-9]$/.test(key)) return;

    if (key === 'l' || key === 'L') {
      const S = typeof GM.state === 'function' ? GM.state() : null;
      if (!S || !S.selected) return; // 没有选中实例时不抢快捷键
      const btn = document.getElementById('btn-play');
      if (btn && !btn.disabled) {
        e.preventDefault();
        btn.click(); // #btn-play 内部用的就是 selectedVersion()（即 S.selected）
      }
      return;
    }

    if (key === ',' || key === '，') {
      e.preventDefault();
      GM.go('settings');
      return;
    }

    if (e.shiftKey && (key === 'R' || key === 'r')) {
      e.preventDefault();
      if (typeof GM.renderAll === 'function') GM.renderAll();
      else if (window.__GM && window.__GM.renderAll) window.__GM.renderAll();
    }
  }

  // ---------------------------------------------------------------- 初始化

  function refreshAll() {
    ensureStyle();
    applyPrefs();
    injectCards();
    refreshA11yCard();
    ensureSkipLink();
    clearA11yLabels();
    improveButtonLabels(document.body);
    if (backendReady()) refreshPreview();
  }

  const hooks = {
    ready() {
      refreshAll();
      // features-prefs 换图/清图时会写 #bg-layer 的 style，借它触发预览刷新
      const layer = document.getElementById('bg-layer');
      if (layer && window.MutationObserver) {
        new MutationObserver(() => refreshPreview()).observe(layer, {
          attributes: true,
          attributeFilter: ['style'],
        });
      }
      document.addEventListener('keydown', onKeydown);
    },
    render() {
      // renderAll 在 boot 早期也会跑一次，那时 __GM 还没挂出来；refreshAll 会跳过需要后端的部分
      refreshAll();
    },
  };

  if (typeof GM.register === 'function') GM.register(hooks);
  else {
    if (hooks.ready) HOOKS.ready.push(hooks.ready);
    if (hooks.render) HOOKS.render.push(hooks.render);
  }

  // 语言切换：i18n.setLocale 先 apply(document) 再通知监听者，
  // 这里重算读屏标签（它们取自当前文案）并刷新预览状态。
  if (GM.i18n && GM.i18n.onChange) {
    GM.i18n.onChange(() => {
      ensureSkipLink();
      refreshA11yCard();
      clearA11yLabels();
      improveButtonLabels(document.body);
      GM.i18n.apply(document);
      refreshPreview();
    });
  }
})();
