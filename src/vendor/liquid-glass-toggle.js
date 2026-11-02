/*!
 * Liquid Glass Toggle — <liquid-glass-toggle>
 * 跨平台液态玻璃开关 · Web Component 版
 *
 * 覆盖：Windows (WebView2) / Linux (WebKitGTK) / Android (WebView) / 任意浏览器 / macOS (WKWebView)
 * 零依赖，Shadow DOM，所有样式自包含。
 *
 * 核心不是"把开关画成玻璃"，而是"判断该不该画玻璃，以及画不出来时怎么退得体面"。
 * 三档：liquid（真玻璃）→ frosted（磨砂）→ solid（实色），圆角在任何档位都不变。
 *
 * 用法：
 *   <liquid-glass-toggle checked size="md" glass="auto" label="启用"></liquid-glass-toggle>
 *
 * 属性：checked | disabled | size(sm|md|lg) | glass(auto|on|off) | label | tone
 * 事件：change → detail.checked
 * 方法：toggle() | refreshTier()
 * 变量：--lgt-accent | --lgt-accent-2 | --lgt-track | --lgt-radius
 * 调试：host.dataset.glass / host.dataset.glassReason
 */
(function () {
  'use strict';

  if (typeof window === 'undefined' || typeof document === 'undefined') return;
  if (window.customElements && customElements.get('liquid-glass-toggle')) return;

  /* ------------------------------------------------------------------ *
   * 0. 令牌
   * ------------------------------------------------------------------ */

  var TONES = {
    blue: ['#5F9FD6', '#8B7FE8'],
    purple: ['#8B7FE8', '#C77FE8'],
    green: ['#3FB98A', '#5FD6A8'],
    orange: ['#E59A4B', '#E5764B'],
    red: ['#E5675F', '#E54B8B']
  };

  /* ------------------------------------------------------------------ *
   * 1. 能力探测
   * ------------------------------------------------------------------ */

  function mq(query) {
    try {
      return window.matchMedia ? window.matchMedia(query) : null;
    } catch (e) {
      return null;
    }
  }

  function supportsBackdrop() {
    if (!window.CSS || !window.CSS.supports) return false;
    try {
      return (
        CSS.supports('backdrop-filter', 'blur(1px)') ||
        CSS.supports('-webkit-backdrop-filter', 'blur(1px)')
      );
    } catch (e) {
      return false;
    }
  }

  function prefersReduceTransparency() {
    var m = mq('(prefers-reduced-transparency: reduce)');
    // forced-colors（高对比度模式）同样意味着用户不要花哨效果
    var f = mq('(forced-colors: active)');
    return !!((m && m.matches) || (f && f.matches));
  }

  function prefersReduceMotion() {
    var m = mq('(prefers-reduced-motion: reduce)');
    return !!(m && m.matches);
  }

  function isWeakDevice() {
    var cores = navigator.hardwareConcurrency || 8;
    var mem = navigator.deviceMemory || 8;
    var conn = navigator.connection || navigator.mozConnection || null;
    var saveData = !!(conn && conn.saveData);
    return cores <= 2 || mem <= 2 || saveData;
  }

  /**
   * 降级决策链（严格按 core/spec.md 第 4 节，命中即停）
   * @returns {[string, string]} [tier, reason]
   */
  function resolveTier(el) {
    var mode = (el.getAttribute('glass') || 'auto').toLowerCase();

    if (mode === 'off') return ['solid', 'forced-off'];
    if (mode === 'on') return ['liquid', 'forced-on'];
    if (el.hasAttribute('data-force-frosted')) return ['frosted', 'forced-frosted'];

    if (prefersReduceTransparency()) return ['frosted', 'reduce-transparency'];
    if (!supportsBackdrop()) return ['frosted', 'no-backdrop-filter'];
    if (isWeakDevice()) return ['solid', 'weak-device'];

    return ['liquid', 'ok'];
  }

  /* ------------------------------------------------------------------ *
   * 2. 样式
   * ------------------------------------------------------------------ */

  var CSS_TEXT = [
    ':host {',
    '  --lgt-accent: #5F9FD6;',
    '  --lgt-accent-2: #8B7FE8;',
    '  --lgt-track: rgba(120, 134, 156, .20);',
    '  --lgt-radius: 999px;',
    '  --stroke-top: rgba(255, 255, 255, .78);',
    '  --stroke-bot: rgba(0, 0, 0, .08);',
    '  --stroke-out: rgba(255, 255, 255, .55);',
    '  --solid-off: #C9CFD9;',
    '  --knob: rgba(255, 255, 255, .96);',
    '',
    '  --w: 46px; --h: 26px; --knob-size: 20px; --inset: 3px;',
    '',
    '  display: inline-block;',
    '  vertical-align: middle;',
    '  cursor: pointer;',
    '  outline: none;',
    '  -webkit-tap-highlight-color: transparent;',
    '  contain: layout style;',
    '}',
    '',
    '@media (prefers-color-scheme: dark) {',
    '  :host {',
    '    --lgt-track: rgba(120, 134, 156, .26);',
    '    --stroke-top: rgba(255, 255, 255, .38);',
    '    --stroke-bot: rgba(0, 0, 0, .20);',
    '    --stroke-out: rgba(255, 255, 255, .18);',
    '    --solid-off: #5A6472;',
    '    --knob: rgba(255, 255, 255, .94);',
    '  }',
    '}',
    '',
    /* 几何：三档唯一真相，圆角恒为高度的一半 */
    ':host([size="sm"]) { --w: 36px; --h: 20px; --knob-size: 16px; --inset: 2px; }',
    ':host([size="lg"]) { --w: 58px; --h: 32px; --knob-size: 26px; --inset: 3px; }',
    '',
    ':host([hidden]) { display: none !important; }',
    ':host([disabled]) { cursor: not-allowed; opacity: .42; }',
    '',
    '.root {',
    '  position: relative;',
    '  width: var(--w);',
    '  height: var(--h);',
    '  border-radius: var(--lgt-radius);',
    '}',
    '',
    '.track {',
    '  position: absolute;',
    '  inset: 0;',
    '  border-radius: var(--lgt-radius);',
    '  transition: box-shadow .3s ease-out;',
    '}',
    '',
    /* --- 底层：玻璃本体（只有这一层带 backdrop-filter） --- */
    '.glass {',
    '  position: absolute;',
    '  inset: 0;',
    '  border-radius: inherit;',
    '  background: var(--lgt-track);',
    '  transition: background-color .22s ease-out;',
    '}',
    '',
    /* --- 开启态：accent 渐变 --- */
    '.fill {',
    '  position: absolute;',
    '  inset: 0;',
    '  border-radius: inherit;',
    '  background: linear-gradient(135deg, var(--lgt-accent), var(--lgt-accent-2));',
    '  opacity: 0;',
    '  transition: opacity .22s ease-out;',
    '}',
    ':host([checked]) .fill { opacity: 1; }',
    '',
    /* --- 折射层：上亮下暗，制造"玻璃有厚度"的错觉 --- */
    '.refract {',
    '  position: absolute;',
    '  inset: 0;',
    '  border-radius: inherit;',
    '  background: linear-gradient(180deg,',
    '    rgba(255, 255, 255, .24) 0%,',
    '    rgba(255, 255, 255, .04) 34%,',
    '    transparent 52%,',
    '    rgba(0, 0, 0, .12) 100%);',
    '  transition: opacity .22s ease-out;',
    '  pointer-events: none;',
    '}',
    '',
    /* --- 描边：顶部亮线 + 底部暗线 + 外圈细描边 --- */
    '.stroke {',
    '  position: absolute;',
    '  inset: 0;',
    '  border-radius: inherit;',
    '  border: 1px solid var(--stroke-out);',
    '  box-shadow:',
    '    inset 0 1px 0 var(--stroke-top),',
    '    inset 0 -1px 0 var(--stroke-bot);',
    '  transition: border-color .3s, box-shadow .3s;',
    '  pointer-events: none;',
    '}',
    '',
    /* --- 滑块 --- */
    '.knob {',
    '  position: absolute;',
    '  top: var(--inset);',
    '  left: var(--inset);',
    '  width: calc(var(--knob-size) * var(--stretch, 1));',
    '  height: var(--knob-size);',
    '  border-radius: 999px;',
    '  background: var(--knob);',
    '  box-shadow:',
    '    0 1px 1px rgba(0, 0, 0, .16),',
    '    0 2px 6px rgba(0, 0, 0, .28);',
    '  display: flex;',
    '  align-items: center;',
    '  justify-content: center;',
    '  overflow: hidden;',
    '  transition:',
    '    transform .34s cubic-bezier(.34, 1.42, .64, 1),',
    '    width .12s ease-out,',
    '    box-shadow .34s ease-out;',
    '  will-change: transform;',
    '}',
    ':host([checked]) .knob {',
    '  transform: translateX(calc(var(--w) - var(--knob-size) * var(--stretch, 1) - var(--inset) * 2));',
    '  box-shadow:',
    '    0 1px 1px rgba(0, 0, 0, .14),',
    '    0 3px 10px rgba(0, 0, 0, .34);',
    '}',
    '',
    /* 滑块顶部镜面高光 */
    '.spec {',
    '  width: 74%;',
    '  height: 46%;',
    '  margin-top: -14%;',
    '  border-radius: 50%;',
    '  background: radial-gradient(ellipse at 50% 8%,',
    '    rgba(255, 255, 255, .98) 0%,',
    '    rgba(255, 255, 255, .42) 42%,',
    '    rgba(255, 255, 255, 0) 74%);',
    '  pointer-events: none;',
    '}',
    '',
    /* --- 按下：滑块横向拉伸（果冻感） --- */
    ':host(:active) .knob { --stretch: 1.18; }',
    ':host([size="sm"]:active) .knob { --stretch: 1.12; }',
    '',
    /* --- 焦点环 --- */
    ':host(:focus-visible) .track {',
    '  box-shadow: 0 0 0 2px var(--lgt-accent), 0 0 0 4px rgba(95, 159, 214, .3);',
    '}',
    '',
    /* ================= 档位 1：liquid（真玻璃） ================= */
    ':host([data-glass="liquid"]) .glass {',
    '  -webkit-backdrop-filter: blur(12px) saturate(180%);',
    '  backdrop-filter: blur(12px) saturate(180%);',
    '}',
    ':host([data-glass="liquid"]) .track {',
    '  box-shadow:',
    '    0 1px 2px rgba(0, 0, 0, .18),',
    '    0 6px 18px -8px rgba(0, 0, 0, .45);',
    '}',
    ':host([data-glass="liquid"]) .refract { opacity: 1; }',
    '',
    /* ================= 档位 2：frosted（磨砂） ================= */
    ':host([data-glass="frosted"]) .glass {',
    '  background: rgba(120, 134, 156, .42);',
    '  -webkit-backdrop-filter: none;',
    '  backdrop-filter: none;',
    '}',
    '@media (prefers-color-scheme: dark) {',
    '  :host([data-glass="frosted"]) .glass { background: rgba(78, 90, 108, .58); }',
    '}',
    ':host([data-glass="frosted"]) .track {',
    '  box-shadow: 0 1px 2px rgba(0, 0, 0, .14), 0 3px 9px -6px rgba(0, 0, 0, .3);',
    '}',
    ':host([data-glass="frosted"]) .refract { opacity: .38; }',
    '',
    /* ================= 档位 3：solid（实色） ================= */
    ':host([data-glass="solid"]) .glass {',
    '  background: var(--solid-off);',
    '  -webkit-backdrop-filter: none;',
    '  backdrop-filter: none;',
    '}',
    ':host([data-glass="solid"]) .track {',
    '  box-shadow: 0 1px 2px rgba(0, 0, 0, .12);',
    '}',
    ':host([data-glass="solid"]) .refract { opacity: 0; }',
    ':host([data-glass="solid"]) .stroke {',
    '  border-color: rgba(0, 0, 0, .08);',
    '  box-shadow: inset 0 1px 0 rgba(255, 255, 255, .28);',
    '}',
    '',
    /* ================= 显式主题覆盖 =================
     * 页面配色不一定跟随系统（例如深色页面跑在浅色系统上）。
     * 给宿主一个 scheme="dark|light" 出口，未指定时才跟随 prefers-color-scheme。
     * 放在 media 查询之后，靠属性选择器的特异性取胜。 */
    ':host([scheme="light"]) {',
    '  --lgt-track: rgba(120, 134, 156, .20);',
    '  --stroke-top: rgba(255, 255, 255, .78);',
    '  --stroke-bot: rgba(0, 0, 0, .08);',
    '  --stroke-out: rgba(255, 255, 255, .55);',
    '  --solid-off: #C9CFD9;',
    '  --knob: rgba(255, 255, 255, .96);',
    '}',
    ':host([scheme="dark"]) {',
    '  --lgt-track: rgba(120, 134, 156, .26);',
    '  --stroke-top: rgba(255, 255, 255, .38);',
    '  --stroke-bot: rgba(0, 0, 0, .20);',
    '  --stroke-out: rgba(255, 255, 255, .18);',
    '  --solid-off: #5A6472;',
    '  --knob: rgba(255, 255, 255, .94);',
    '}',
    ':host([scheme="light"][data-glass="frosted"]) .glass { background: rgba(120, 134, 156, .42); }',
    ':host([scheme="dark"][data-glass="frosted"]) .glass { background: rgba(78, 90, 108, .58); }',
    '',
    /* ================= 减弱动态 ================= */
    '@media (prefers-reduced-motion: reduce) {',
    '  .knob, .fill, .glass, .refract, .track, .stroke {',
    '    transition-duration: .001ms !important;',
    '  }',
    '  :host(:active) .knob { --stretch: 1; }',
    '}'
  ].join('\n');

  var TEMPLATE = [
    '<div class="root" part="root">',
    '  <div class="track" part="track">',
    '    <div class="glass" part="glass"></div>',
    '    <div class="fill" part="fill"></div>',
    '    <div class="refract" part="refract"></div>',
    '    <div class="stroke" part="stroke"></div>',
    '    <div class="knob" part="knob"><i class="spec"></i></div>',
    '  </div>',
    '</div>'
  ].join('');

  /* ------------------------------------------------------------------ *
   * 3. 元素
   * ------------------------------------------------------------------ */

  var WATCHED_MQ = [
    '(prefers-reduced-transparency: reduce)',
    '(forced-colors: active)',
    '(prefers-reduced-motion: reduce)',
    '(prefers-color-scheme: dark)'
  ];

  class LiquidGlassToggle extends HTMLElement {
    static get observedAttributes() {
      return ['checked', 'disabled', 'size', 'glass', 'tone', 'label'];
    }

    constructor() {
      super();
      var root = this.attachShadow({ mode: 'open' });
      var style = document.createElement('style');
      style.textContent = CSS_TEXT;
      root.appendChild(style);

      var wrap = document.createElement('div');
      wrap.innerHTML = TEMPLATE;
      this._rootEl = wrap.firstElementChild;
      root.appendChild(this._rootEl);

      this._glassEl = root.querySelector('.glass');
      this._trackEl = root.querySelector('.track');
      this._listeners = [];
      this._tier = '';
      this._reason = '';
      this._selfChecked = false;
    }

    /* ---------- 生命周期 ---------- */

    connectedCallback() {
      if (!this.hasAttribute('role')) this.setAttribute('role', 'switch');
      if (!this.hasAttribute('tabindex') && !this.hasAttribute('disabled')) {
        this.setAttribute('tabindex', '0');
      }
      this._syncA11y();
      this._applyTone();
      this.refreshTier();
      this._bind();
      this._watchMedia();

      // 首帧自检：有些平台 CSS.supports 说支持，实际渲染不出模糊
      if (!this._selfChecked) {
        this._selfChecked = true;
        var self = this;
        requestAnimationFrame(function () {
          requestAnimationFrame(function () { self._selfCheck(); });
        });
      }
    }

    disconnectedCallback() {
      this._listeners.forEach(function (off) { off(); });
      this._listeners.length = 0;
      (this._mqs || []).forEach(function (item) {
        try {
          item.mq.removeEventListener('change', item.handler);
        } catch (e) {
          try { item.mq.removeListener(item.handler); } catch (e2) {}
        }
      });
      this._mqs = [];
    }

    attributeChangedCallback(name) {
      if (name === 'checked' || name === 'disabled') this._syncA11y();
      if (name === 'tone') this._applyTone();
      if (name === 'glass') this.refreshTier();
    }

    /* ---------- 公开 API ---------- */

    get checked() {
      return this.hasAttribute('checked');
    }

    set checked(v) {
      if (v) this.setAttribute('checked', '');
      else this.removeAttribute('checked');
    }

    get disabled() {
      return this.hasAttribute('disabled');
    }

    set disabled(v) {
      if (v) this.setAttribute('disabled', '');
      else this.removeAttribute('disabled');
    }

    /** 当前实际渲染档位：liquid / frosted / solid */
    get tier() {
      return this._tier;
    }

    /** 当前档位的判定原因 */
    get glassReason() {
      return this._reason;
    }

    toggle() {
      if (this.disabled) return;
      this.checked = !this.checked;
      this._emit();
    }

    refreshTier() {
      var r = resolveTier(this);
      this._setTier(r[0], r[1]);
    }

    /* ---------- 内部 ---------- */

    _setTier(tier, reason) {
      this._tier = tier;
      this._reason = reason;
      this.setAttribute('data-glass', tier);
      this.setAttribute('data-glass-reason', reason);
    }

    _emit() {
      this.dispatchEvent(new CustomEvent('change', {
        detail: { checked: this.checked },
        bubbles: true,
        composed: true
      }));
    }

    _syncA11y() {
      this.setAttribute('aria-checked', this.checked ? 'true' : 'false');
      if (this.disabled) {
        this.setAttribute('aria-disabled', 'true');
        this.removeAttribute('tabindex');
      } else {
        this.removeAttribute('aria-disabled');
        if (!this.hasAttribute('tabindex')) this.setAttribute('tabindex', '0');
      }
      var label = this.getAttribute('label');
      if (label && !this.hasAttribute('aria-label')) this.setAttribute('aria-label', label);
    }

    _applyTone() {
      var tone = (this.getAttribute('tone') || '').trim();
      if (!tone) return;
      var pair = TONES[tone.toLowerCase()];
      if (pair) {
        this.style.setProperty('--lgt-accent', pair[0]);
        this.style.setProperty('--lgt-accent-2', pair[1]);
      } else {
        // 任意 CSS 颜色：单色时第二站用同色，避免渐变断裂
        this.style.setProperty('--lgt-accent', tone);
        this.style.setProperty('--lgt-accent-2', tone);
      }
    }

    _selfCheck() {
      if (this._tier !== 'liquid') return;
      var cs = null;
      try {
        cs = getComputedStyle(this._glassEl);
      } catch (e) {
        return;
      }
      var v = cs.backdropFilter || cs.webkitBackdropFilter || '';
      if (!v || v === 'none') this._setTier('frosted', 'self-check-failed');
    }

    _bind() {
      var self = this;

      var onClick = function (e) {
        e.preventDefault();
        self.toggle();
      };
      this.addEventListener('click', onClick);
      this._listeners.push(function () { self.removeEventListener('click', onClick); });

      var onKey = function (e) {
        if (e.key === ' ' || e.key === 'Spacebar' || e.key === 'Enter') {
          e.preventDefault();
          self.toggle();
        }
      };
      this.addEventListener('keydown', onKey);
      this._listeners.push(function () { self.removeEventListener('keydown', onKey); });
    }

    _watchMedia() {
      var self = this;
      this._mqs = [];
      WATCHED_MQ.forEach(function (q) {
        var m = mq(q);
        if (!m) return;
        var handler = function () {
          self.refreshTier();
          self._selfChecked = false;
          requestAnimationFrame(function () { self._selfCheck(); });
        };
        try {
          m.addEventListener('change', handler);
        } catch (e) {
          try { m.addListener(handler); } catch (e2) { return; }
        }
        self._mqs.push({ mq: m, handler: handler });
      });

      // 设备内存/网络变化（Chrome 支持）
      if (navigator.connection && navigator.connection.addEventListener) {
        var ch = function () { self.refreshTier(); };
        try {
          navigator.connection.addEventListener('change', ch);
          this._listeners.push(function () {
            navigator.connection.removeEventListener('change', ch);
          });
        } catch (e) {}
      }
    }
  }

  customElements.define('liquid-glass-toggle', LiquidGlassToggle);
  window.LiquidGlassToggle = LiquidGlassToggle;
  window.LiquidGlassToggle.resolveTier = resolveTier;
  window.LiquidGlassToggle.supportsBackdrop = supportsBackdrop;
})();
