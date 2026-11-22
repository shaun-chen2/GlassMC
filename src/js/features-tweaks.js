/* =====================================================================
 * features-tweaks.js —— 移动端等价 / 离线辅助包 / 启动优化
 *
 * 全部是「桌面启动器里能做到的等价实现」：
 * - Java 版无法真正触控注入，这里只做触控外观与虚拟操作条；
 * - 辅助包只离线生成 zip，绝不自动放进游戏；
 * - 进程清理只在用户勾选后执行，脚本只在点「试运行」时执行。
 * 不编辑 app.js / index.html 的既有逻辑，靠 GM.register/addNav/addPage 接入。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = window.GM;
  const HOOKS = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });
  const t = (k, v) => GM.t('tweaks.' + k, v);
  const esc = (s) => (GM.ui && GM.ui.escAttr ? GM.ui.escAttr(s) : GM.esc(s));
  const LS_TOUCH = 'gm-tw-touch';
  const LS_KEYS = 'gm-tw-keys';
  const LOW_ARGS = '-Xmx512M  -XX:+UseSerialGC  -XX:MaxMetaspaceSize=256m';

  const KINDS = [
    ['keep_inventory', 'kind.keep_inventory'],
    ['minimap', 'kind.minimap'],
    ['xray', 'kind.xray'],
    ['geometry', 'kind.geometry'],
    ['entity_health', 'kind.entity_health'],
    ['modpe', 'kind.modpe'],
  ];

  let prefs = {
    touchMode: false,
    virtualKeys: false,
    lowMemory: false,
    smartGc: false,
    prelaunchScript: '',
  };
  let mounted = false;
  let bar = null;

  const state = () => GM.state();
  const invoke = (cmd, args) => GM.invoke(cmd, args || {});
  const errText = (e) => String((e && e.message) || e || '').split('\n')[0];
  const fail = (e) => t('fail', { err: errText(e) });
  const toast = (msg, kind) => GM.toast(msg, kind);
  const busy = (on, label) => GM.busy(on, label);

  function installed() {
    const s = state();
    return (s && s.installed) || [];
  }

  function currentVersion() {
    const s = state();
    const id = s && s.config && s.config.lastVersion;
    const list = installed();
    return (id && list.find((v) => v.id === id)) || list[0] || null;
  }

  function gameDir() {
    const v = currentVersion();
    if (v && v.gameDir) return v.gameDir;
    const s = state();
    return (s && s.boot && s.boot.gameDir) || '';
  }

  function versionFromPage() {
    const sel = document.querySelector('[data-page="tweaks"] [data-tw-version]');
    return sel ? sel.value : '';
  }

  function setOut(id, text) {
    const box = document.getElementById(id);
    if (box) box.textContent = text == null ? '' : String(text);
  }

  // ---------------------------------------------------------------- 触控外观

  function injectTouchCss() {
    if (document.getElementById('gm-tw-touch-css')) return;
    const st = document.createElement('style');
    st.id = 'gm-tw-touch-css';
    st.textContent = [
      'body.gm-touch .btn, body.gm-touch .nav-item, body.gm-touch .chip {',
      '  min-height:44px; min-width:44px; padding:10px 16px; }',
      'body.gm-touch .btn.sm { min-height:38px; padding:8px 12px; }',
      'body.gm-touch .btn:hover, body.gm-touch .nav-item:hover, body.gm-touch .chip:hover {',
      '  transform:none; filter:none; box-shadow:none; }',
      'body.gm-touch input[type=checkbox], body.gm-touch input[type=radio] {',
      '  width:20px; height:20px; }',
      'body.gm-touch select, body.gm-touch input[type=text], body.gm-touch textarea {',
      '  font-size:16px; }',
    ].join('\n');
    document.head.appendChild(st);
  }

  function applyTouch(on) {
    document.body.classList.toggle('gm-touch', !!on);
    try {
      localStorage.setItem(LS_TOUCH, on ? '1' : '0');
    } catch (_) {
      /* 隐私模式下不可用，忽略 */
    }
  }

  // ---------------------------------------------------------------- 虚拟按键

  function keyState() {
    try {
      return JSON.parse(localStorage.getItem(LS_KEYS) || '{}') || {};
    } catch (_) {
      return {};
    }
  }

  function saveKeyState(s) {
    try {
      localStorage.setItem(LS_KEYS, JSON.stringify(s));
    } catch (_) {
      /* 忽略 */
    }
  }

  function makeBar() {
    if (bar) return;
    bar = document.createElement('div');
    bar.id = 'gm-tw-bar';
    Object.assign(bar.style, {
      position: 'fixed',
      right: '18px',
      bottom: '18px',
      zIndex: '9999',
      display: 'none',
      gap: '6px',
      alignItems: 'center',
      padding: '8px',
      borderRadius: '14px',
      background: 'rgba(20,24,34,.82)',
      backdropFilter: 'blur(12px)',
      border: '1px solid rgba(255,255,255,.18)',
      color: '#fff',
      fontSize: '12px',
      touchAction: 'none',
    });
    bar.innerHTML =
      '<span data-tw-grip style="cursor:move;padding:0 6px;opacity:.7">⋮⋮</span>' +
      '<span data-tw-vk-body style="display:flex;gap:6px">' +
      ['launch', 'shots', 'backup', 'home']
        .map(
          (a) =>
            `<button class="btn sm" data-tw-vk="${a}">${esc(t('vk.' + a))}</button>`,
        )
        .join('') +
      '</span>' +
      `<button class="btn sm" data-tw-vk="collapse">${esc(t('vk.collapse'))}</button>`;
    document.body.appendChild(bar);

    bar.addEventListener('click', (e) => {
      const b = e.target.closest('[data-tw-vk]');
      if (!b) return;
      const act = b.dataset.twVk;
      if (act === 'collapse') {
        const s = keyState();
        s.collapsed = !s.collapsed;
        saveKeyState(s);
        syncVirtualBar();
      } else {
        vkAction(act);
      }
    });
    bindDrag();
  }

  function bindDrag() {
    const grip = bar && bar.querySelector('[data-tw-grip]');
    if (!grip) return;
    let sx = 0;
    let sy = 0;
    let ox = 0;
    let oy = 0;
    let dragging = false;
    grip.addEventListener('pointerdown', (e) => {
      const r = bar.getBoundingClientRect();
      sx = r.left;
      sy = r.top;
      ox = e.clientX - r.left;
      oy = e.clientY - r.top;
      dragging = true;
      if (grip.setPointerCapture) grip.setPointerCapture(e.pointerId);
      e.preventDefault();
    });
    grip.addEventListener('pointermove', (e) => {
      if (!dragging) return;
      sx = Math.max(0, Math.min(window.innerWidth - bar.offsetWidth, e.clientX - ox));
      sy = Math.max(0, Math.min(window.innerHeight - bar.offsetHeight, e.clientY - oy));
      bar.style.left = sx + 'px';
      bar.style.top = sy + 'px';
      bar.style.right = 'auto';
      bar.style.bottom = 'auto';
    });
    const end = () => {
      if (!dragging) return;
      dragging = false;
      const s = keyState();
      s.x = sx;
      s.y = sy;
      saveKeyState(s);
    };
    grip.addEventListener('pointerup', end);
    grip.addEventListener('pointercancel', end);
  }

  function syncVirtualBar() {
    if (!bar) return;
    bar.style.display = prefs.virtualKeys ? 'flex' : 'none';
    const s = keyState();
    if (typeof s.x === 'number' && typeof s.y === 'number') {
      bar.style.left = s.x + 'px';
      bar.style.top = s.y + 'px';
      bar.style.right = 'auto';
      bar.style.bottom = 'auto';
    }
    const body = bar.querySelector('[data-tw-vk-body]');
    if (body) body.style.display = s.collapsed ? 'none' : 'flex';
    const cb = bar.querySelector('[data-tw-vk="collapse"]');
    if (cb) cb.textContent = t(s.collapsed ? 'vk.expand' : 'vk.collapse');
  }

  async function vkAction(act) {
    if (act === 'home') {
      GM.go('home');
      return;
    }
    if (act === 'launch') {
      const v = currentVersion();
      if (!v) {
        toast(t('chooseVersion'), 'err');
        return;
      }
      try {
        await invoke('launch_game', { versionId: v.id });
        toast(t('vk.launched'), 'ok');
      } catch (e) {
        toast(fail(e), 'err');
      }
      return;
    }
    const dir = gameDir().replace(/\/+$/, '');
    if (!dir) {
      toast(t('chooseVersion'), 'err');
      return;
    }
    if (act === 'shots') {
      try {
        await invoke('open_dir', { path: dir + '/screenshots' });
      } catch (e) {
        toast(fail(e), 'err');
      }
      return;
    }
    if (act === 'backup') {
      try {
        const v = currentVersion();
        if (!v) throw new Error(t('chooseVersion'));
        const worlds = await invoke('saves_list', { versionId: v.id });
        if (!worlds.length) throw new Error(t('vk.noWorld'));
        await invoke('saves_backup', {
          versionId: v.id,
          saveId: worlds[0].id,
          label: 'quick-' + new Date().toISOString().slice(0, 10),
        });
        toast(t('vk.backed'), 'ok');
      } catch (e) {
        // 没有世界或后端不可用时，退化为打开存档目录，按钮不至于失效
        invoke('open_dir', { path: dir + '/saves' }).catch((err) =>
          toast(fail(err), 'err'),
        );
      }
    }
  }

  // ---------------------------------------------------------------- 页面

  function pageHtml() {
    const kindHtml = KINDS.map(
      ([k, key]) =>
        `<button class="chip" data-tw-act="pack-make" data-kind="${k}" ` +
        `title="${esc(t('kind.' + k + '.desc'))}">${esc(t(key))}</button>`,
    ).join('');
    return `<div class="stack" style="gap:18px">
      <div class="field">
        <div class="lbl" style="font-size:16px">${esc(t('sec.mobile'))}</div>
        <div class="desc">${esc(t('sec.mobile.desc'))}</div>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('touch'))}</div>
        <label class="hstack" style="gap:8px">
          <input type="checkbox" id="tw-touch" data-tw-toggle="touchMode"> ${esc(t('touch.on'))}
        </label>
        <div class="desc">${esc(t('touch.desc'))}<br>${esc(t('touch.limit'))}</div>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('mem'))}</div>
        <div class="hstack" style="gap:8px;flex-wrap:wrap">
          <select data-tw-version></select>
          <button class="btn sm" data-tw-act="mem">${esc(t('mem.run'))}</button>
        </div>
        <div class="desc">${esc(t('mem.desc'))}</div>
        <pre id="tw-mem-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('dir'))}</div>
        <div class="desc">${esc(t('dir.desc'))}<br>${esc(t('dir.limit'))}</div>
        <div id="tw-dirs" class="stack" style="gap:6px"></div>
        <pre id="tw-dir-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('vkeys'))}</div>
        <label class="hstack" style="gap:8px">
          <input type="checkbox" id="tw-vkeys" data-tw-toggle="virtualKeys"> ${esc(t('vkeys.on'))}
        </label>
        <div class="desc">${esc(t('vkeys.desc'))}<br>${esc(t('vkeys.limit'))}</div>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('lowmem'))}</div>
        <label class="hstack" style="gap:8px">
          <input type="checkbox" id="tw-lowmem" data-tw-toggle="lowMemory"> ${esc(t('lowmem.on'))}
        </label>
        <div class="desc">${esc(t('lowmem.desc'))}<br>${esc(t('lowmem.limit'))}<br>
          <code>${esc(LOW_ARGS)}</code></div>
      </div>

      <div class="field">
        <div class="lbl" style="font-size:16px">${esc(t('sec.assist'))}</div>
        <div class="desc">${esc(t('pack.desc'))}<br>${esc(t('pack.limit'))}</div>
        <div class="chips" style="margin-top:8px">${kindHtml}</div>
        <div id="tw-packs" class="stack" style="gap:6px;margin-top:10px"></div>
        <pre id="tw-pack-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>

      <div class="field">
        <div class="lbl" style="font-size:16px">${esc(t('sec.boot'))}</div>
        <div class="desc">${esc(t('sec.boot.desc'))}</div>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('fast'))}</div>
        <div class="hstack" style="gap:8px">
          <button class="btn sm" data-tw-act="fast">${esc(t('fast.run'))}</button>
        </div>
        <div class="desc">${esc(t('fast.desc'))}<br>${esc(t('fast.limit'))}</div>
        <pre id="tw-fast-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('clean'))}</div>
        <div class="hstack" style="gap:8px">
          <button class="btn sm" data-tw-act="clean-scan">${esc(t('clean.scan'))}</button>
          <button class="btn sm" data-tw-act="clean-apply">${esc(t('clean.apply'))}</button>
        </div>
        <div class="desc">${esc(t('clean.desc'))}<br>${esc(t('clean.limit'))}</div>
        <div id="tw-procs" class="stack" style="gap:6px"></div>
        <pre id="tw-clean-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('gc'))}</div>
        <label class="hstack" style="gap:8px">
          <input type="checkbox" id="tw-gc" data-tw-toggle="smartGc"> ${esc(t('gc.on'))}
        </label>
        <div class="hstack" style="gap:8px;margin-top:6px">
          <select data-tw-version></select>
          <button class="btn sm" data-tw-act="gc-preview">${esc(t('gc.preview'))}</button>
        </div>
        <div class="desc">${esc(t('gc.desc'))}<br>${esc(t('gc.limit'))}</div>
        <pre id="tw-gc-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('script'))}</div>
        <textarea id="tw-script" rows="4" style="width:100%;resize:vertical" placeholder="${esc(t('script.ph'))}"></textarea>
        <div class="hstack" style="gap:8px;margin-top:6px">
          <button class="btn sm" data-tw-act="script-save">${esc(t('script.save'))}</button>
          <button class="btn sm" data-tw-act="script-run">${esc(t('script.run'))}</button>
        </div>
        <div class="desc">${esc(t('script.desc'))}<br>${esc(t('script.limit'))}</div>
        <pre id="tw-script-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>

      <div class="field">
        <div class="lbl">${esc(t('mcomp'))}</div>
        <div class="hstack" style="gap:8px">
          <button class="btn sm" data-tw-act="mcomp-preview">${esc(t('mcomp.preview'))}</button>
        </div>
        <div class="desc">${esc(t('mcomp.desc'))}</div>
        <pre id="tw-mcomp-out" class="mono-dim" style="white-space:pre-wrap"></pre>
      </div>
    </div>`;
  }

  function refreshVersions() {
    const page = document.querySelector('[data-page="tweaks"]');
    if (!page) return;
    const list = installed();
    page.querySelectorAll('[data-tw-version]').forEach((sel) => {
      const cur = sel.value;
      sel.innerHTML = list.length
        ? list.map((v) => `<option value="${esc(v.id)}">${esc(v.id)}</option>`).join('')
        : `<option value="">${esc(t('noVersion'))}</option>`;
      const s = state();
      const id = cur || (s && s.config && s.config.lastVersion) || '';
      if (id && list.some((v) => v.id === id)) sel.value = id;
    });
  }

  function syncToggles() {
    const map = {
      'tw-touch': prefs.touchMode,
      'tw-vkeys': prefs.virtualKeys,
      'tw-lowmem': prefs.lowMemory,
      'tw-gc': prefs.smartGc,
    };
    Object.keys(map).forEach((id) => {
      const el = document.getElementById(id);
      if (el) el.checked = !!map[id];
    });
    const ta = document.getElementById('tw-script');
    if (ta) ta.value = prefs.prelaunchScript || '';
  }

  async function loadPrefs() {
    try {
      prefs = await invoke('tweak_prefs_get');
    } catch (_) {
      // 预览模式：沿用默认值 + localStorage
      prefs.touchMode = localStorage.getItem(LS_TOUCH) === '1';
    }
    applyTouch(prefs.touchMode);
    syncToggles();
    syncVirtualBar();
  }

  async function loadDirs() {
    const box = document.getElementById('tw-dirs');
    if (!box) return;
    try {
      const list = await invoke('data_dir_targets');
      box.innerHTML = list.length
        ? list
            .map(
              (d) =>
                `<div class="vrow"><div class="v-main">` +
                `<div class="v-id">${esc(d.label)}</div>` +
                `<div class="v-meta"><span class="mono-dim">${esc(d.path)}</span> ` +
                `<span class="tag">${esc(d.kind)}</span></div></div>` +
                `<div class="v-actions"><button class="btn sm" data-tw-act="dir-move" ` +
                `data-target="${esc(d.path)}">${esc(t('dir.move'))}</button></div></div>`,
            )
            .join('')
        : `<div class="desc">${esc(t('dir.none'))}</div>`;
    } catch (e) {
      box.textContent = errText(e);
    }
  }

  async function loadPacks() {
    const box = document.getElementById('tw-packs');
    if (!box) return;
    try {
      const list = await invoke('assist_packs_list');
      box.innerHTML = list.length
        ? list
            .map(
              (p) =>
                `<div class="vrow"><div class="v-main">` +
                `<div class="v-id">${esc(p.kind)}</div>` +
                `<div class="v-meta"><span class="mono-dim">${esc(t('pack.size', { kb: Math.round(p.size / 1024) }))}</span></div></div>` +
                `<div class="v-actions"><button class="btn sm" data-tw-act="pack-open" ` +
                `data-kind="${esc(p.kind)}">${esc(t('pack.open'))}</button></div></div>`,
            )
            .join('')
        : `<div class="desc">${esc(t('pack.empty'))}</div>`;
    } catch (e) {
      box.textContent = errText(e);
    }
  }

  // ---------------------------------------------------------------- 交互

  async function onToggle(key, on) {
    try {
      if (key === 'touchMode') {
        prefs.touchMode = on;
        applyTouch(on);
        await invoke('tweak_prefs_set', { patch: { touchMode: on } });
      } else if (key === 'virtualKeys') {
        prefs.virtualKeys = on;
        syncVirtualBar();
        await invoke('tweak_prefs_set', { patch: { virtualKeys: on } });
      } else if (key === 'lowMemory') {
        prefs.lowMemory = on;
        await invoke('low_memory_set', { enabled: on });
      } else if (key === 'smartGc') {
        prefs.smartGc = on;
        await invoke('smart_gc_set', { enabled: on });
      }
    } catch (e) {
      toast(fail(e), 'err');
    }
  }

  async function onAct(btn) {
    const act = btn.dataset.twAct;
    if (act === 'mem') {
      const id = versionFromPage();
      if (!id) {
        toast(t('chooseVersion'), 'err');
        return;
      }
      busy(true, t('busy'));
      try {
        const r = await invoke('memory_recommend', { versionId: id });
        setOut(
          'tw-mem-out',
          t('mem.result', { gb: r.recommendedGb, mods: r.modCount, sys: r.systemGb }) +
            '\n' +
            r.reason,
        );
      } catch (e) {
        setOut('tw-mem-out', fail(e));
      } finally {
        busy(false);
      }
      return;
    }

    if (act === 'dir-move') {
      const target = btn.dataset.target;
      busy(true, t('busy'));
      try {
        const r = await invoke('data_dir_move', { target });
        setOut('tw-dir-out', r.note);
        toast(t('dir.done'), 'ok');
      } catch (e) {
        setOut('tw-dir-out', fail(e));
      } finally {
        busy(false);
      }
      return;
    }

    if (act === 'pack-make') {
      const kind = btn.dataset.kind;
      const id = versionFromPage();
      busy(true, t('busy'));
      try {
        const r = await invoke('assist_pack_create', {
          kind,
          versionId: id || (currentVersion() || {}).id || '',
        });
        setOut('tw-pack-out', r.note + '\n' + r.path);
        toast(t('pack.done'), 'ok');
        loadPacks();
      } catch (e) {
        setOut('tw-pack-out', fail(e));
      } finally {
        busy(false);
      }
      return;
    }

    if (act === 'pack-open') {
      try {
        await invoke('assist_pack_open', { kind: btn.dataset.kind });
      } catch (e) {
        toast(fail(e), 'err');
      }
      return;
    }

    if (act === 'fast') {
      busy(true, t('busy'));
      try {
        const r = await invoke('fast_core_bench');
        setOut(
          'tw-fast-out',
          t('fast.result', { scan: r.scanMs, manifest: r.manifestMs, n: r.versionCount }) +
            '\n' +
            r.note,
        );
      } catch (e) {
        setOut('tw-fast-out', fail(e));
      } finally {
        busy(false);
      }
      return;
    }

    if (act === 'clean-scan') {
      busy(true, t('busy'));
      try {
        const list = await invoke('cleanup_processes_scan');
        const box = document.getElementById('tw-procs');
        if (box) {
          box.innerHTML = list
            .map(
              (p) =>
                `<label class="hstack" style="gap:8px;font-size:12.5px">` +
                `<input type="checkbox" data-tw-proc="${esc(p.name)}"> ` +
                `${esc(p.label)} — ${esc(p.reason)}</label>`,
            )
            .join('');
          setOut('tw-clean-out', t('clean.hint'));
        }
      } catch (e) {
        setOut('tw-clean-out', fail(e));
      } finally {
        busy(false);
      }
      return;
    }

    if (act === 'clean-apply') {
      const names = Array.prototype.slice
        .call(document.querySelectorAll('[data-tw-proc]:checked'))
        .map((el) => el.dataset.twProc);
      if (!names.length) {
        toast(t('clean.none'), 'err');
        return;
      }
      busy(true, t('busy'));
      try {
        const r = await invoke('cleanup_processes_apply', { names });
        const lines = r.results.map(
          (x) => `${x.name}: ${x.ok ? t('clean.ok') : t('clean.bad') + ' ' + x.error}`,
        );
        setOut('tw-clean-out', lines.join('\n'));
      } catch (e) {
        setOut('tw-clean-out', fail(e));
      } finally {
        busy(false);
      }
      return;
    }

    if (act === 'gc-preview') {
      const id = versionFromPage();
      try {
        const r = await invoke('smart_gc_args', { versionId: id });
        setOut(
          'tw-gc-out',
          t('gc.result', { on: r.enabled ? t('common.on') : t('common.off') }) +
            '\n' +
            (r.args.length ? r.args.join('  ') : t('gc.empty')) +
            '\n' +
            r.note,
        );
      } catch (e) {
        setOut('tw-gc-out', fail(e));
      }
      return;
    }

    if (act === 'script-save') {
      const ta = document.getElementById('tw-script');
      const script = (ta && ta.value) || '';
      try {
        prefs = await invoke('prelaunch_script_set', { script });
        toast(t('script.saved'), 'ok');
      } catch (e) {
        toast(fail(e), 'err');
      }
      return;
    }

    if (act === 'script-run') {
      busy(true, t('busy'));
      try {
        const r = await invoke('prelaunch_script_run');
        setOut(
          'tw-script-out',
          t('script.result', {
            ok: r.ok ? t('common.yes') : t('common.no'),
            code: r.exitCode == null ? '—' : r.exitCode,
          }) +
            '\n--- stdout ---\n' +
            r.stdout +
            '\n--- stderr ---\n' +
            r.stderr +
            (r.timedOut ? '\n' + t('script.timeout') : ''),
        );
      } catch (e) {
        setOut('tw-script-out', fail(e));
      } finally {
        busy(false);
      }
      return;
    }

    if (act === 'mcomp-preview') {
      try {
        const args = await invoke('memory_compression_args');
        setOut(
          'tw-mcomp-out',
          args.length ? args.join('  ') + '\n' + t('mcomp.result') : t('mcomp.empty'),
        );
      } catch (e) {
        setOut('tw-mcomp-out', fail(e));
      }
    }
  }

  function bindPage() {
    const page = document.querySelector('[data-page="tweaks"]');
    if (!page || page.dataset.twBound) return;
    page.dataset.twBound = '1';
    page.addEventListener('change', (e) => {
      const tg = e.target.closest('[data-tw-toggle]');
      if (tg) onToggle(tg.dataset.twToggle, tg.checked);
    });
    page.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-tw-act]');
      if (btn) onAct(btn);
    });
  }

  // ---------------------------------------------------------------- 接入

  function mount() {
    if (mounted) return;
    mounted = true;
    injectTouchCss();
    GM.addNav({
      id: 'tweaks',
      label: 'nav.tweaks',
      title: GM.t('nav.tweaks'),
      icon:
        '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" ' +
        'stroke-width="1.8"><path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-3 3-3-3 3-3z"/></svg>',
    });
    GM.addPage('tweaks', pageHtml());
    makeBar();
    bindPage();
    refreshVersions();
    syncToggles();
    loadPrefs();
    loadDirs();
    loadPacks();
  }

  HOOKS.ready.push(mount);
  HOOKS.render.push(() => {
    if (!mounted) return;
    refreshVersions();
    syncVirtualBar();
  });
  HOOKS.nav.push((page) => {
    if (page === 'tweaks') refreshVersions();
  });
})();
