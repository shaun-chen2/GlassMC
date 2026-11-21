/* =====================================================================
 * features-more.js —— 导入（启动器 / 整合包）、陶瓦联机、设置补充项
 * ===================================================================== */
(function () {
  const HOOKS = window.__GM_HOOKS;
  const GM = (window.GM = window.GM || {});
  const ui = GM.ui;
  let invoke = null;
  let S = null;
  const t = (k, v) => window.GM.t(k, v);
  const toast = (m, k) => GM.ui.toast(m, k);

  // ================================================================ 导入

  const impState = { list: [], loading: false };

  async function scanLaunchers(btn) {
    if (impState.loading) return;
    impState.loading = true;
    ui.busy(btn, true, t('more.busy.scanning'));
    try {
      impState.list = await invoke('import_detect');
      renderImportList();
      toast(
        impState.list.length
          ? t('more.import.found', { n: impState.list.length })
          : t('more.import.not_found'),
        impState.list.length ? 'ok' : '',
      );
    } catch (e) {
      toast(String(e), 'err');
    } finally {
      impState.loading = false;
      ui.busy(btn, false);
    }
  }

  function renderImportList() {
    const box = document.getElementById('import-list');
    if (!box) return;
    const count = document.getElementById('import-count');
    if (count) count.textContent = impState.list.length ? t('more.count', { n: impState.list.length }) : '—';
    if (!impState.list.length) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${t('more.import.empty')}</span></div>`;
      return;
    }
    // 启动器名称：HMCL 与 Prism / MultiMC 是产品名，不翻译
    const LAUNCHER = { official: 'more.launcher.official', hmcl: 'HMCL', prism: 'Prism / MultiMC' };
    box.innerHTML = impState.list
      .map(
        (c) => `<div class="vrow">
        <div class="v-main">
          <div class="v-id" style="font-size:12.5px">${ui.escAttr(t(LAUNCHER[c.launcher] || c.launcher))}</div>
          <div class="v-meta"><span class="mono-dim" style="max-width:520px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${ui.escAttr(c.gameDir)}</span></div>
          <div class="v-meta"><span class="tag">${t('more.import.versions', { n: c.versionCount })}</span>
            ${c.accountCount ? `<span class="tag">${t('more.import.accounts', { n: c.accountCount })}</span>` : ''}</div>
        </div>
        <div class="v-actions"><button class="btn sm" data-import="${ui.escAttr(c.launcher)}" data-dir="${ui.escAttr(c.gameDir)}">${t('more.btn.import')}</button></div>
      </div>`,
      )
      .join('');
  }

  async function runImport(launcher, gameDir, btn) {
    ui.showProgress(t('more.import.progress_title'));
    ui.busy(btn, true, t('more.busy.importing'));
    try {
      ui.updateProgress({ frac: 0.3, label: t('more.import.progress_copy') });
      const n = await invoke('import_run', { launcher, gameDir });
      ui.updateProgress({ frac: 1, label: t('common.done') });
      setTimeout(() => ui.hideProgress(), 600);
      S.installed = await invoke('scan_versions');
      S.boot.accounts = await invoke('accounts_all');
      GM.app.renderAll();
      toast(t('more.toast.import_done', { n }), 'ok');
      GM.app.go('versions');
    } catch (e) {
      ui.hideProgress();
      toast(String(e).split('\n')[0], 'err');
    } finally {
      ui.busy(btn, false);
    }
  }

  // ---------------------------------------------------------------- 整合包

  async function pickModpack() {
    const path = await invoke('pick_file', { kind: 'pack' });
    if (!path) return;
    try {
      const probe = await invoke('modpack_probe', { path });
      const idx = probe.index || {};
      const isCf = probe.kind === 'curseforge';
      const mods = (idx.files || []).filter(
        (f) => f.path.toLowerCase().startsWith('mods/') && f.path.toLowerCase().endsWith('.jar'),
      ).length;
      const deps = idx.dependencies || {};
      const loaderKey = ['neoforge', 'forge', 'fabric-loader', 'quilt-loader'].find((k) => deps[k]);
      ui.openSheet(
        ui.shell(
          ui.escAttr(idx.name || t('more.pack.fallback_name')),
          ui.escAttr(idx.summary || ''),
          `<div class="stack" style="gap:8px">
             <div class="kv"><span class="k">${t('more.pack.field.source')}</span><span class="v">${isCf ? 'CurseForge .zip' : 'Modrinth .mrpack'}</span></div>
             <div class="kv"><span class="k">Minecraft</span><span class="v">${ui.escAttr(deps.minecraft || t('more.pack.undeclared'))}</span></div>
             <div class="kv"><span class="k">${t('more.pack.field.loader')}</span><span class="v">${ui.escAttr(loaderKey ? `${loaderKey} ${deps[loaderKey]}` : t('more.pack.vanilla'))}</span></div>
             <div class="kv"><span class="k">${t('more.pack.field.mods')}</span><span class="v">${t('more.pack.count', { n: mods })}</span></div>
             <div class="kv"><span class="k">${t('more.pack.field.total')}</span><span class="v">${t('more.pack.items', { n: (idx.files || []).length })}</span></div>
             <div class="mini-note">${t('more.pack.note')}${
               isCf ? t('more.pack.note_cf') : ''
             }</div>
           </div>`,
          `<button class="btn" data-sheet-close>${t('common.cancel')}</button>
           <button class="btn primary" id="mrpack-ok">${t('more.btn.import')}</button>`,
        ),
      );
      document.getElementById('mrpack-ok').onclick = async () => {
        const btn = document.getElementById('mrpack-ok');
        ui.busy(btn, true, t('more.busy.importing'));
        ui.showProgress(t('more.pack.progress_title'));
        try {
          if (isCf) {
            const out = await invoke('curseforge_import', { path, name: null });
            ui.updateProgress({ frac: 1, label: t('more.pack.progress_files') });
            ui.closeSheet();
            if (out && out.failed) {
              toast(t('more.toast.pack_partial', { n: out.failed }), 'err');
            }
            await afterModpackImport(out.record);
          } else {
            const rec = await invoke('modpack_import', { path, name: null });
            ui.updateProgress({ frac: 1, label: t('more.pack.progress_files') });
            ui.closeSheet();
            await afterModpackImport(rec);
          }
        } catch (e) {
          ui.hideProgress();
          ui.busy(btn, false);
          toast(String(e).split('\n')[0], 'err');
        }
      };
    } catch (e) {
      toast(String(e).split('\n')[0], 'err');
    }
  }

  /** 判断整合包依赖是否齐，缺就引导继续安装。 */
  async function afterModpackImport(rec) {
    S.installed = await invoke('scan_versions');
    GM.app.renderAll();
    setTimeout(() => ui.hideProgress(), 500);

    let detail = null;
    try {
      detail = await invoke('version_detail', { versionId: rec.instanceId });
    } catch (e) {
      /* 缺原版也是这个结果 */
    }
    const missingGame = !detail;
    const missingLibs = detail && detail.missingLibraries > 0;
    const missingJar = detail && !detail.jarOk;

    if (!missingGame && !missingLibs && !missingJar) {
      toast(t('more.toast.pack_ready', { name: rec.name }), 'ok');
      return;
    }

    ui.openSheet(
      ui.shell(
        t('more.pack.ready_title'),
        t('more.pack.ready_sub'),
        `<div class="mini-note">${t('more.pack.need_note', {
          name: ui.escAttr(rec.name),
          game: ui.escAttr(rec.gameVersion),
          loader: rec.loader ? ` + ${ui.escAttr(rec.loader)} ${ui.escAttr(rec.loaderVersion)}` : '',
        })}</div>
         <div class="section-gap stack" style="gap:8px">
           ${
             missingGame
               ? `<button class="btn primary" id="mp-install-game">${t('more.pack.btn_install_game', { game: ui.escAttr(rec.gameVersion) })}</button>`
               : ''
           }
           ${
             rec.loader && (missingGame || missingJar || missingLibs)
               ? `<button class="btn" id="mp-install-loader">${t('more.pack.btn_install_loader', {
                   loader: ui.escAttr(rec.loader),
                   version: ui.escAttr(rec.loaderVersion),
                 })}</button>`
               : ''
           }
           <button class="btn" id="mp-install-deps">${t('more.pack.btn_deps')}</button>
         </div>`,
        `<button class="btn" data-sheet-close>${t('more.btn.later')}</button>`,
      ),
    );

    const g = document.getElementById('mp-install-game');
    if (g) {
      g.onclick = async () => {
        ui.closeSheet();
        await installVersionFlow(rec.gameVersion);
      };
    }
    const l = document.getElementById('mp-install-loader');
    if (l) {
      l.onclick = async () => {
        ui.closeSheet();
        ui.showProgress(t('more.pack.progress_install_loader', { loader: rec.loader, version: rec.loaderVersion }));
        try {
          await invoke('install_loader', {
            kind: rec.loader,
            game: rec.gameVersion,
            loaderVersion: rec.loaderVersion,
          });
          ui.updateProgress({ frac: 1, label: t('common.done') });
          setTimeout(() => ui.hideProgress(), 600);
          S.installed = await invoke('scan_versions');
          GM.app.renderAll();
          toast(t('more.toast.loader_ready'), 'ok');
        } catch (e) {
          ui.hideProgress();
          toast(String(e).split('\n')[0], 'err');
        }
      };
    }
    const d = document.getElementById('mp-install-deps');
    if (d) {
      d.onclick = async () => {
        ui.closeSheet();
        await installVersionFlow(rec.instanceId);
      };
    }
  }

  async function installVersionFlow(versionId) {
    ui.showProgress(t('more.pack.progress_install_ver', { id: versionId }));
    try {
      const res = await invoke('install_version', { versionId });
      ui.updateProgress({ frac: 1, label: t('common.done') });
      setTimeout(() => ui.hideProgress(), 600);
      S.installed = await invoke('scan_versions');
      GM.app.renderAll();
      if (res.failed && res.failed.length) toast(t('more.toast.files_failed', { n: res.failed.length }), '');
      else toast(t('more.toast.installed'), 'ok');
    } catch (e) {
      ui.hideProgress();
      toast(String(e).split('\n')[0], 'err');
    }
  }

  async function exportModpack() {
    const cur = GM.app.selectedVersion();
    if (!cur) {
      toast(t('more.toast.pick_version'), 'err');
      return;
    }
    const dest = await invoke('pick_save_path', { defaultName: `${cur.id}.mrpack` });
    if (!dest) return;
    ui.showProgress(t('more.pack.export_title'));
    try {
      ui.updateProgress({ frac: 0.4, label: t('more.pack.export_collect') });
      const msg = await invoke('modpack_export', {
        versionId: cur.id,
        destination: dest,
        name: cur.id,
        summary: t('more.pack.export_summary'),
      });
      ui.updateProgress({ frac: 1, label: t('common.done') });
      setTimeout(() => ui.hideProgress(), 600);
      toast(t('more.toast.exported', { msg }), 'ok');
      loadPacks();
    } catch (e) {
      ui.hideProgress();
      toast(String(e).split('\n')[0], 'err');
    }
  }

  async function loadPacks() {
    const box = document.getElementById('import-packs');
    if (!box) return;
    try {
      const list = await invoke('modpack_list');
      if (!list.length) {
        box.innerHTML = '';
        return;
      }
      box.innerHTML = list
        .map(
          (p) => `<div class="vrow">
          <div class="v-main">
            <div class="v-id" style="font-size:12.5px">${ui.escAttr(p.name)}</div>
            <div class="v-meta"><span class="tag">MC ${ui.escAttr(p.gameVersion)}</span>
              <span class="tag">${ui.escAttr(p.loader || t('more.pack.vanilla'))} ${ui.escAttr(p.loaderVersion || '')}</span>
              <span class="tag">${t('more.pack.mods', { n: p.modCount })}</span>
              <span class="mono-dim">${ui.escAttr(p.instanceId)}</span></div>
          </div>
          <div class="v-actions"><button class="btn sm" data-play="${ui.escAttr(p.instanceId)}">${t('more.btn.launch')}</button></div>
        </div>`,
        )
        .join('');
    } catch (e) {
      box.innerHTML = '';
    }
  }

  // ================================================================ 联机

  let tcSnap = null;
  let tcTimer = null;

  // 下面三张表存的是文案 key，渲染时再翻译；未知状态回退到原始值
  const STATE_TEXT = {
    waiting: 'more.tc.state.waiting',
    'host-scanning': 'more.tc.state.host_scanning',
    'host-starting': 'more.tc.state.host_starting',
    'host-ok': 'more.tc.state.host_ok',
    'guest-connecting': 'more.tc.state.guest_connecting',
    'guest-ok': 'more.tc.state.guest_ok',
    exception: 'more.tc.state.exception',
  };

  const DIFF_TEXT = {
    EASIEST: 'more.tc.diff.easiest',
    SIMPLE: 'more.tc.diff.simple',
    MEDIUM: 'more.tc.diff.medium',
    TOUGH: 'more.tc.diff.tough',
    UNKNOWN: 'more.tc.diff.unknown',
  };

  const EXC_TEXT = {
    0: 'more.tc.exc.0',
    1: 'more.tc.exc.1',
    2: 'more.tc.exc.2',
    3: 'more.tc.exc.3',
    4: 'more.tc.exc.4',
    5: 'more.tc.exc.5',
  };

  function renderTc() {
    const box = document.getElementById('tc-status');
    if (!box) return;
    const s = tcSnap;
    const ver = document.getElementById('tc-ver');
    if (ver) ver.textContent = s && s.ready ? `${s.version}${s.easytier ? ' · ' + s.easytier : ''}` : '—';

    if (!s) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${t('more.tc.empty')}</span></div>`;
      renderMembers([]);
      return;
    }
    if (s.preparing) {
      box.innerHTML = `<div class="empty"><span class="spinner"></span><span>${ui.escAttr(s.prepareText || t('more.tc.preparing'))}</span></div>`;
      renderMembers([]);
      return;
    }
    if (!s.ready) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${
        s.error ? ui.escAttr(s.error) : t('more.tc.hint_boot')
      }</span></div>`;
      renderMembers([]);
      return;
    }
    const st = s.state || {};
    const state = st.state || 'waiting';
    const label = STATE_TEXT[state] ? t(STATE_TEXT[state]) : state;
    const isOk = state === 'host-ok' || state === 'guest-ok';
    const isExc = state === 'exception';
    const parts = [];
    parts.push(
      `<div class="hstack">
        <span class="badge-pill ${isExc ? 'danger' : isOk ? 'ok' : 'accent'}"><span class="d"></span>${ui.escAttr(label)}</span>
        ${st.difficulty ? `<span class="tag">${ui.escAttr(t(DIFF_TEXT[st.difficulty] || st.difficulty))}</span>` : ''}
      </div>`,
    );
    if (st.room) {
      parts.push(
        `<div class="code-box" style="padding:11px 13px">
           <div>
             <div class="mini-note">${t('more.tc.room_code')}</div>
             <div class="code" style="font-size:15px">${ui.escAttr(st.room)}</div>
           </div>
           <button class="btn sm" id="tc-copy" style="margin-left:auto">${t('common.copy')}</button>
         </div>`,
      );
    }
    if (isExc && st.type != null) {
      parts.push(`<div class="mini-note" style="color:var(--danger)">${ui.escAttr(t(EXC_TEXT[st.type] || 'more.tc.unknown_error'))}</div>`);
    }
    if (st.url) {
      parts.push(`<div class="kv"><span class="k">${t('more.tc.tunnel_url')}</span><span class="v mono-dim">${ui.escAttr(st.url)}</span></div>`);
    }
    parts.push(`<div class="mini-note">${t('more.tc.note')}</div>`);
    box.innerHTML = parts.join('');
    const copy = document.getElementById('tc-copy');
    if (copy) {
      copy.onclick = () => {
        navigator.clipboard.writeText(st.room || '').then(
          () => toast(t('more.tc.copied'), 'ok'),
          () => toast(t('more.tc.copy_failed'), 'err'),
        );
      };
    }
    renderMembers(st.profiles || []);
  }

  function renderMembers(list) {
    const box = document.getElementById('tc-members');
    const count = document.getElementById('tc-count');
    if (count) count.textContent = list.length ? t('more.tc.members', { n: list.length }) : '—';
    if (!box) return;
    box.innerHTML = list.length
      ? list
          .map((p) => {
            const role = p.kind === 'HOST' ? t('more.tc.role.host') : p.kind === 'GUEST' ? t('more.tc.role.guest') : t('more.tc.role.local');
            return `<div class="vrow">
              <div class="acc-avatar">${ui.escAttr((p.name || '?').slice(0, 1).toUpperCase())}</div>
              <div class="v-main">
                <div class="v-id" style="font-size:12.5px">${ui.escAttr(p.name)}</div>
                <div class="v-meta"><span class="tag">${role}</span>
                  <span class="mono-dim">${ui.escAttr(p.vendor || '')}</span></div>
              </div>
            </div>`;
          })
          .join('')
      : `<div class="empty"><span class="big-ico">◌</span><span>${t('more.tc.empty_members')}</span></div>`;
  }

  async function tcBootstrap(btn) {
    ui.busy(btn, true, t('more.busy.starting'));
    tcSnap = Object.assign({}, tcSnap, { preparing: true, prepareText: t('more.tc.preparing') });
    renderTc();
    try {
      tcSnap = await invoke('tc_bootstrap');
      renderTc();
      startTcPolling();
      toast(t('more.toast.tc_ready'), 'ok');
    } catch (e) {
      tcSnap = await invoke('tc_snapshot').catch(() => tcSnap);
      renderTc();
      toast(String(e).split('\n')[0], 'err');
    } finally {
      ui.busy(btn, false);
    }
  }

  function startTcPolling() {
    if (tcTimer) clearInterval(tcTimer);
    tcTimer = setInterval(async () => {
      if (!tcSnap || !tcSnap.ready) return;
      try {
        const st = await invoke('tc_refresh');
        tcSnap.state = st;
        renderTc();
      } catch (e) {
        tcSnap = await invoke('tc_snapshot').catch(() => tcSnap);
        renderTc();
      }
    }, 1800);
  }

  async function tcAction(fn, btn, label) {
    ui.busy(btn, true, label);
    try {
      const st = await fn();
      if (tcSnap) tcSnap.state = st;
      renderTc();
    } catch (e) {
      toast(String(e).split('\n')[0], 'err');
    } finally {
      ui.busy(btn, false);
    }
  }

  function tcPlayer() {
    const v = (document.getElementById('tc-player') || {}).value || '';
    if (v.trim()) return v.trim();
    const acc = GM.accounts.active();
    return acc ? acc.username : 'GlassMC';
  }

  // ================================================================ 设置补充

  function renderServers() {
    const box = document.getElementById('srv-list');
    if (!box) return;
    const list = S.config.authServers || [];
    const count = document.getElementById('srv-count');
    if (count) count.textContent = t('more.count', { n: list.length });
    box.innerHTML = list
      .map(
        (s) => `<div class="vrow">
        <div class="v-main">
          <div class="v-id" style="font-size:12px">${ui.escAttr(s.name)}</div>
          <div class="v-meta"><span class="mono-dim">${ui.escAttr(s.baseUrl)}</span>
            ${s.isPreset ? `<span class="tag">${t('more.srv.builtin')}</span>` : ''}</div>
        </div>
        <div class="v-actions">${
          s.isPreset ? '' : `<button class="btn sm danger" data-del-srv="${ui.escAttr(s.id)}">${t('common.delete')}</button>`
        }</div>
      </div>`,
      )
      .join('');
  }

  async function renderInjector() {
    const el = document.getElementById('injector-ver');
    const state = document.getElementById('injector-state');
    try {
      const s = await invoke('injector_status');
      if (el) el.textContent = s.installed ? t('more.injector.ready', { ver: s.version ? ` · ${s.version}` : '' }) : t('more.injector.missing');
      if (state) state.textContent = s.installed ? t('more.injector.ok') : t('more.injector.state_missing');
    } catch (e) {
      if (el) el.textContent = '—';
    }
  }

  // ================================================================ 事件

  function bindMoreEvents() {
    const impPage = document.querySelector('[data-page="import"]');
    if (impPage && !impPage.dataset.bound) {
      impPage.dataset.bound = '1';
      impPage.addEventListener('click', (e) => {
        const b = e.target.closest('[data-import]');
        if (b) {
          runImport(b.dataset.import, b.dataset.dir, b);
          return;
        }
        const p = e.target.closest('[data-play]');
        if (p && window.GM.app) {
          const btn = document.getElementById('btn-play');
          window.GM.app.go('home');
          invoke('launch_game', { versionId: p.dataset.play })
            .then(() => toast(t('more.toast.launched'), 'ok'))
            .catch((err) => toast(String(err).split('\n')[0], 'err'));
          return;
        }
      });
      document.getElementById('btn-import-scan').onclick = (e) => scanLaunchers(e.currentTarget);
      document.getElementById('btn-import-mrpack').onclick = pickModpack;
      document.getElementById('btn-export-mrpack').onclick = exportModpack;
    }

    const mpPage = document.querySelector('[data-page="multiplayer"]');
    if (mpPage && !mpPage.dataset.bound) {
      mpPage.dataset.bound = '1';
      document.getElementById('btn-tc-boot').onclick = (e) => tcBootstrap(e.currentTarget);
      document.getElementById('btn-tc-shutdown').onclick = async () => {
        try {
          await invoke('tc_shutdown');
        } catch (e) {
          /* 忽略 */
        }
        if (tcTimer) clearInterval(tcTimer);
        tcTimer = null;
        tcSnap = await invoke('tc_snapshot').catch(() => null);
        renderTc();
        toast(t('more.toast.tc_closed'), 'ok');
      };
      document.getElementById('btn-tc-create').onclick = (e) =>
        tcAction(() => invoke('tc_create', { player: tcPlayer() }), e.currentTarget, t('more.tc.create'));
      document.getElementById('btn-tc-join').onclick = (e) => {
        const room = (document.getElementById('tc-room') || {}).value || '';
        if (!room.trim()) {
          toast(t('more.tc.need_room'), 'err');
          return;
        }
        tcAction(() => invoke('tc_join', { room: room.trim(), player: tcPlayer() }), e.currentTarget, t('more.tc.join'));
      };
      document.getElementById('btn-tc-leave').onclick = (e) =>
        tcAction(() => invoke('tc_leave'), e.currentTarget, t('more.tc.leave'));
    }

    const srvBox = document.getElementById('srv-list');
    if (srvBox && !srvBox.dataset.bound) {
      srvBox.dataset.bound = '1';
      srvBox.addEventListener('click', async (e) => {
        const del = e.target.closest('[data-del-srv]');
        if (!del) return;
        try {
          const list = await invoke('remove_auth_server', { id: del.dataset.delSrv });
          S.config.authServers = list;
          renderServers();
          toast(t('more.toast.deleted'), 'ok');
        } catch (err) {
          toast(String(err), 'err');
        }
      });
    }

    const addBtn = document.getElementById('btn-add-srv');
    if (addBtn && !addBtn.dataset.bound) {
      addBtn.dataset.bound = '1';
      addBtn.onclick = async () => {
        const name = (document.getElementById('srv-name') || {}).value || '';
        const base = (document.getElementById('srv-base') || {}).value || '';
        try {
          S.config.authServers = await invoke('add_auth_server', { name, baseUrl: base });
          document.getElementById('srv-name').value = '';
          document.getElementById('srv-base').value = '';
          renderServers();
          toast(t('more.toast.added'), 'ok');
        } catch (e) {
          toast(String(e), 'err');
        }
      };
    }

    const inj = document.getElementById('btn-injector');
    if (inj && !inj.dataset.bound) {
      inj.dataset.bound = '1';
      inj.onclick = async (e) => {
        ui.busy(e.currentTarget, true, t('more.busy.downloading'));
        try {
          await invoke('injector_install', { force: true });
          await renderInjector();
          toast(t('more.injector.ok'), 'ok');
        } catch (err) {
          toast(String(err), 'err');
        } finally {
          ui.busy(e.currentTarget, false);
        }
      };
    }

    // Java 手动指定
    const pickJava = document.getElementById('btn-pick-java');
    if (pickJava && !pickJava.dataset.bound) {
      pickJava.dataset.bound = '1';
      pickJava.onclick = async () => {
        const path = await invoke('pick_file', { kind: 'java' });
        if (!path) return;
        GM.app.saveConfig({ javaPath: path });
        S.config.javaPath = path;
        S.boot.java = await invoke('list_java_all');
        renderSettingsExtra();
        toast(t('more.toast.java_set'), 'ok');
      };
    }
    const clearJava = document.getElementById('btn-clear-java');
    if (clearJava && !clearJava.dataset.bound) {
      clearJava.dataset.bound = '1';
      clearJava.onclick = async () => {
        GM.app.saveConfig({ javaPath: '' });
        S.config.javaPath = '';
        S.boot.java = await invoke('list_java_all');
        renderSettingsExtra();
        toast(t('more.toast.java_auto'), 'ok');
      };
    }

    const skinBase = document.getElementById('set-skinlib');
    if (skinBase && !skinBase.dataset.bound) {
      skinBase.dataset.bound = '1';
      skinBase.onchange = (e) => {
        GM.app.saveConfig({ skinLibraryBase: e.target.value.trim() });
        setTimeout(() => {
          if (S.config.skinLibraryBase) S.config.skinLibraryBase = e.target.value.trim();
          const site = document.getElementById('skinlib-site');
          if (site) site.textContent = e.target.value.trim();
        }, 60);
      };
    }
    const msId = document.getElementById('set-msclient');
    if (msId && !msId.dataset.bound) {
      msId.dataset.bound = '1';
      msId.onchange = (e) => GM.app.saveConfig({ msClientId: e.target.value.trim() });
    }
  }

  // ================================================================ 钩子

  HOOKS.ready.push(async () => {
    invoke = window.__GM.invoke;
    S = window.__GM.S;
    bindMoreEvents();
    renderServers();
    renderInjector();
    loadPacks();
    tcSnap = await invoke('tc_snapshot').catch(() => null);
    renderTc();
    if (tcSnap && tcSnap.ready) startTcPolling();
  });

  HOOKS.nav.push((page) => {
    if (page === 'multiplayer') {
      invoke('tc_snapshot')
        .then((s) => {
          tcSnap = s;
          renderTc();
          if (s && s.ready) startTcPolling();
        })
        .catch(() => {});
    }
    if (page === 'import') loadPacks();
  });

  function renderSettingsExtra() {
    renderServers();
    const sb = document.getElementById('set-skinlib');
    if (sb && document.activeElement !== sb) sb.value = S.config.skinLibraryBase || '';
    const mc = document.getElementById('set-msclient');
    if (mc && document.activeElement !== mc) mc.value = S.config.msClientId || '';
    const jm = document.getElementById('java-manual');
    if (jm) {
      jm.textContent = S.config.javaPath
        ? t('more.java.manual', { path: S.config.javaPath })
        : t('more.java.auto');
    }
  }

  HOOKS.render.push(() => {
    if (!S) return;
    renderSettingsExtra();
    // 注入器状态文案是动态的，切语言后也要跟着重画
    renderInjector();
  });

  GM.importer = { scan: scanLaunchers, pick: pickModpack, export: exportModpack };
  GM.multiplayer = { refresh: renderTc };
})();
