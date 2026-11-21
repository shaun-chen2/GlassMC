/* =====================================================================
 * features-prefs.js —— 个性化：界面语言 / 缩放 / 自定义背景 / 启动音效 / 快捷键
 *
 * 与 app.js 的分工：
 * - app.js 只管骨架和基础设置项，个性化控件由这里接管；
 * - 所有写盘都走 GM.app.saveConfig（内部自带 200ms 防抖）；
 * - 图片 / 音频不进配置，只存绝对路径，展示时用 media_data_url 现读。
 * ===================================================================== */
(function () {
  const GM = window.GM;
  const HOOKS = window.__GM_HOOKS || (window.__GM_HOOKS = { ready: [], nav: [], render: [] });
  const $ = (sel) => document.querySelector(sel);
  const $$ = (sel) => Array.from(document.querySelectorAll(sel));

  const BG_MAX_BYTES = 16 * 1024 * 1024;
  const SOUND_MAX_BYTES = 8 * 1024 * 1024;

  let bound = false;
  // 背景图是异步读的，用户连续换图时只让最后一次生效
  let bgToken = 0;

  const cfg = () => (GM.app && GM.app.S && GM.app.S.config) || {};
  const t = (key, vars) => GM.t(key, vars);
  const toast = (msg, kind) => GM.app && GM.app.toast && GM.app.toast(msg, kind);
  const invoke = (cmd, args) => GM.app.invoke(cmd, args);

  // ---------------------------------------------------------------- 小工具

  function clamp(v, lo, hi, dflt) {
    const n = Number(v);
    if (!isFinite(n)) return dflt;
    return Math.min(hi, Math.max(lo, n));
  }

  function baseName(p) {
    return String(p || '').split(/[\\/]/).filter(Boolean).pop() || '';
  }

  function firstLine(e) {
    return String((e && e.message) || e || '').split('\n')[0];
  }

  // ---------------------------------------------------------------- 缩放 / 透明度

  function applyScale(scale) {
    const s = clamp(scale, 0.85, 1.25, 1);
    const shell = $('#shell');
    // WebKit / Chromium 支持 zoom；不支持的引擎会忽略，界面仍是 100%
    if (shell) shell.style.zoom = String(s);
    const out = $('#ui-scale-val');
    if (out) out.textContent = Math.round(s * 100) + '%';
    const range = $('#set-ui-scale');
    if (range && Math.abs(Number(range.value) - s) > 1e-6) range.value = String(s);
    return s;
  }

  function applyOpacity(v) {
    const op = clamp(v, 0.05, 0.8, 0.35);
    document.documentElement.style.setProperty('--bg-opacity', String(op));
    const out = $('#bg-opacity-val');
    if (out) out.textContent = Math.round(op * 100) + '%';
    const range = $('#set-bg-opacity');
    if (range && Math.abs(Number(range.value) - op) > 1e-6) range.value = String(op);
    return op;
  }

  // ---------------------------------------------------------------- 状态文案

  function paintLabels() {
    const c = cfg();

    const bgName = baseName(c.backgroundImage);
    const bgFile = $('#bg-file');
    if (bgFile) {
      bgFile.textContent = bgName || t('prefs.file.none');
      bgFile.title = c.backgroundImage || '';
    }
    const bgState = $('#bg-state');
    if (bgState) bgState.textContent = bgName ? t('prefs.bg.on') : t('prefs.bg.off');

    const soundName = baseName(c.launchSound);
    const soundFile = $('#sound-file');
    if (soundFile) {
      soundFile.textContent = soundName || t('prefs.file.none');
      soundFile.title = c.launchSound || '';
    }
  }

  // ---------------------------------------------------------------- 背景图

  /** 按路径把背景读成 data URL 铺到底层；读失败只清空，不打断界面。 */
  async function loadBackground(path) {
    const layer = $('#bg-layer');
    const token = ++bgToken;
    if (!path) {
      if (layer) layer.style.backgroundImage = '';
      document.documentElement.classList.remove('has-bg');
      return;
    }
    try {
      const url = await invoke('media_data_url', { path, maxBytes: BG_MAX_BYTES });
      if (token !== bgToken) return; // 期间又换了图，丢弃这次结果
      if (layer && url) layer.style.backgroundImage = 'url("' + url + '")';
      // 图片在 shell 底下，只有把外壳底色让开才看得见
      document.documentElement.classList.toggle('has-bg', !!(layer && url));
    } catch (e) {
      if (token !== bgToken) return;
      if (layer) layer.style.backgroundImage = '';
      document.documentElement.classList.remove('has-bg');
      console.warn('[prefs] 背景图读取失败', e);
    }
  }

  async function pickBackground(btn) {
    let path = null;
    try {
      path = await invoke('pick_file', { kind: 'image' });
    } catch (e) {
      toast(t('prefs.toast.bgFail', { msg: firstLine(e) }), 'err');
      return;
    }
    if (!path) return; // 用户取消

    if (btn) btn.disabled = true;
    const token = ++bgToken; // 读取期间用户可能点了「清除」
    try {
      const url = await invoke('media_data_url', { path, maxBytes: BG_MAX_BYTES });
      if (token !== bgToken) return;
      const layer = $('#bg-layer');
      if (layer) layer.style.backgroundImage = 'url("' + url + '")';
      document.documentElement.classList.toggle('has-bg', !!url);
      GM.app.saveConfig({
        backgroundImage: path,
        backgroundOpacity: clamp(cfg().backgroundOpacity, 0.05, 0.8, 0.35),
      });
      paintLabels();
      toast(t('prefs.toast.bg'), 'ok');
    } catch (e) {
      toast(t('prefs.toast.bgFail', { msg: firstLine(e) }), 'err');
    } finally {
      if (btn) btn.disabled = false;
    }
  }

  function clearBackground() {
    bgToken++;
    const layer = $('#bg-layer');
    if (layer) layer.style.backgroundImage = '';
    document.documentElement.classList.remove('has-bg');
    GM.app.saveConfig({ backgroundImage: '' });
    paintLabels();
    toast(t('prefs.toast.bgClear'), 'ok');
  }

  function onOpacityInput(e) {
    applyOpacity(e.target.value);
  }

  function onOpacityChange(e) {
    // 拖动时只改 CSS，松手才落盘，避免一秒钟写几十次配置
    const op = applyOpacity(e.target.value);
    GM.app.saveConfig({ backgroundOpacity: op });
  }

  // ---------------------------------------------------------------- 启动音效

  /** quiet = 启动时自动播放，失败不打扰用户（没有声卡/格式不支持都很常见）。 */
  async function playSound(path, quiet) {
    if (!path) return false;
    try {
      const url = await invoke('media_data_url', { path, maxBytes: SOUND_MAX_BYTES });
      const audio = new Audio(url);
      const p = audio.play();
      if (p && typeof p.catch === 'function') {
        p.catch((e) => {
          if (!quiet) toast(t('prefs.toast.soundFail', { msg: firstLine(e) }), 'err');
        });
      }
      return true;
    } catch (e) {
      if (!quiet) toast(t('prefs.toast.soundFail', { msg: firstLine(e) }), 'err');
      return false;
    }
  }

  async function pickSound() {
    let path = null;
    try {
      path = await invoke('pick_file', { kind: 'audio' });
    } catch (e) {
      toast(t('prefs.toast.soundFail', { msg: firstLine(e) }), 'err');
      return;
    }
    if (!path) return;
    GM.app.saveConfig({ launchSound: path });
    paintLabels();
    toast(t('prefs.toast.sound'), 'ok');
  }

  function clearSound() {
    GM.app.saveConfig({ launchSound: '' });
    paintLabels();
    toast(t('prefs.toast.soundClear'), 'ok');
  }

  // ---------------------------------------------------------------- 语言

  function switchLanguage(value) {
    const v = value || 'system';
    GM.app.saveConfig({ language: v });

    const loc = GM.i18n.resolve(v);
    GM.i18n.setLocale(loc); // 内部已经 apply(document) 并通知监听者
    GM.i18n.apply(document);
    GM.app.renderAll();
    GM.app.renderChrome();
    // 运行状态栏是动态文案，重绘一次让它跟着换语言
    if (GM.app.setRunning) GM.app.setRunning(GM.app.S.running);
    paintLabels();

    toast(t('prefs.toast.lang', { lang: t(loc === 'en-US' ? 'prefs.language.en' : 'prefs.language.zh') }), 'ok');
  }

  // ---------------------------------------------------------------- 快捷键

  function isTyping(el) {
    if (!el) return false;
    const tag = el.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable === true;
  }

  function onKeydown(e) {
    // Esc 需要在 features.js 的冒泡监听之前处理：
    // 弹层开着就关弹层，否则收日志；两边都关着时什么也不做。
    if (e.key === 'Escape') {
      const modal = $('#modal');
      if (modal && modal.classList.contains('on')) {
        e.preventDefault();
        e.stopPropagation();
        GM.ui.closeSheet();
        return;
      }
      if (GM.app.S.logOpen) {
        e.preventDefault();
        GM.app.openLog(false);
      }
      return;
    }

    const meta = e.metaKey || e.ctrlKey;
    if (!meta || e.repeat) return;
    // 在输入框里打字时不抢快捷键（Esc 除外，上面已处理）
    if (isTyping(e.target)) return;

    const key = e.key;

    if (/^[1-9]$/.test(key)) {
      const item = $$('.nav-item')[Number(key) - 1];
      if (!item) return;
      e.preventDefault();
      GM.app.go(item.dataset.nav);
      return;
    }

    if (key === 'Enter') {
      const btn = $('#btn-play');
      if (btn && !btn.disabled) {
        e.preventDefault();
        btn.click();
      }
      return;
    }

    if (key === 'f' || key === 'F') {
      const page = GM.app.S.page;
      const sel = page === 'install' ? '#remote-filter' : page === 'mods' ? '#mods-filter' : '';
      const input = sel ? $(sel) : null;
      if (!input) return;
      e.preventDefault();
      input.focus();
      if (input.select) input.select();
    }
  }

  // ---------------------------------------------------------------- 绑定 / 初始化

  function bind() {
    if (bound) return;
    bound = true;

    const lang = $('#set-language');
    if (lang) lang.addEventListener('change', (e) => switchLanguage(e.target.value));

    const scale = $('#set-ui-scale');
    if (scale) {
      scale.addEventListener('input', (e) => applyScale(e.target.value));
      scale.addEventListener('change', (e) => GM.app.saveConfig({ uiScale: applyScale(e.target.value) }));
    }

    const bgPick = $('#btn-bg-pick');
    if (bgPick) bgPick.addEventListener('click', () => pickBackground(bgPick));
    const bgClear = $('#btn-bg-clear');
    if (bgClear) bgClear.addEventListener('click', clearBackground);

    const bgOpacity = $('#set-bg-opacity');
    if (bgOpacity) {
      bgOpacity.addEventListener('input', onOpacityInput);
      bgOpacity.addEventListener('change', onOpacityChange);
    }

    const sPick = $('#btn-sound-pick');
    if (sPick) sPick.addEventListener('click', pickSound);
    const sTest = $('#btn-sound-test');
    if (sTest) sTest.addEventListener('click', () => playSound(cfg().launchSound, false));
    const sClear = $('#btn-sound-clear');
    if (sClear) sClear.addEventListener('click', clearSound);

    // 捕获阶段：见 onKeydown 里对 Esc 的说明
    document.addEventListener('keydown', onKeydown, true);
  }

  /** 开机回填：控件值、CSS 变量、背景图、缩放。 */
  function applyFromConfig() {
    const c = cfg();

    const lang = $('#set-language');
    if (lang) lang.value = c.language || 'zh-CN';

    applyScale(c.uiScale);
    applyOpacity(c.backgroundOpacity);
    paintLabels();
    loadBackground(c.backgroundImage || '');
  }

  // ---------------------------------------------------------------- 钩子

  HOOKS.ready.push(() => {
    bind();
    applyFromConfig();

    // 每次成功启动游戏后端都会 emit 一次，正好用来放启动音效
    if (GM.app.listen) {
      GM.app.listen('game-launch', () => {
        const path = cfg().launchSound;
        if (path) playSound(path, true);
      });
    }
  });

  // 语言变了只重画动态文案；静态节点由 i18n.apply 负责
  if (GM.i18n && GM.i18n.onChange) GM.i18n.onChange(paintLabels);
})();
