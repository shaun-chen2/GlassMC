/* =====================================================================
 * features-more.js —— 导入（启动器 / 整合包）、陶瓦联机、设置补充项
 * ===================================================================== */
(function () {
  const HOOKS = window.__GM_HOOKS;
  const GM = (window.GM = window.GM || {});
  const ui = GM.ui;
  let invoke = null;
  let S = null;
  const toast = (m, k) => GM.ui.toast(m, k);

  // ================================================================ 导入

  const impState = { list: [], loading: false };

  async function scanLaunchers(btn) {
    if (impState.loading) return;
    impState.loading = true;
    ui.busy(btn, true, '扫描中');
    try {
      impState.list = await invoke('import_detect');
      renderImportList();
      toast(impState.list.length ? `找到 ${impState.list.length} 个可导入来源` : '没有找到其它启动器的游戏目录', impState.list.length ? 'ok' : '');
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
    if (count) count.textContent = impState.list.length ? impState.list.length + ' 个' : '—';
    if (!impState.list.length) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>点右上角「扫描本机启动器」</span></div>';
      return;
    }
    const LAUNCHER = { official: '官方启动器', hmcl: 'HMCL', prism: 'Prism / MultiMC' };
    box.innerHTML = impState.list
      .map(
        (c) => `<div class="vrow">
        <div class="v-main">
          <div class="v-id" style="font-size:12.5px">${ui.escAttr(LAUNCHER[c.launcher] || c.launcher)}</div>
          <div class="v-meta"><span class="mono-dim" style="max-width:520px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${ui.escAttr(c.gameDir)}</span></div>
          <div class="v-meta"><span class="tag">${c.versionCount} 个版本</span>
            ${c.accountCount ? `<span class="tag">${c.accountCount} 个账号</span>` : ''}</div>
        </div>
        <div class="v-actions"><button class="btn sm" data-import="${ui.escAttr(c.launcher)}" data-dir="${ui.escAttr(c.gameDir)}">导入</button></div>
      </div>`,
      )
      .join('');
  }

  async function runImport(launcher, gameDir, btn) {
    ui.showProgress('正在导入游戏数据');
    ui.busy(btn, true, '导入中');
    try {
      ui.updateProgress({ frac: 0.3, label: '复制版本 / 模组 / 存档…' });
      const n = await invoke('import_run', { launcher, gameDir });
      ui.updateProgress({ frac: 1, label: '完成' });
      setTimeout(() => ui.hideProgress(), 600);
      S.installed = await invoke('scan_versions');
      S.boot.accounts = await invoke('accounts_all');
      GM.app.renderAll();
      toast(`导入完成，本地共 ${n} 个版本`, 'ok');
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
    const path = await invoke('pick_file', { kind: 'mrpack' });
    if (!path) return;
    try {
      const idx = await invoke('modpack_inspect', { path });
      const mods = (idx.files || []).filter(
        (f) => f.path.toLowerCase().startsWith('mods/') && f.path.toLowerCase().endsWith('.jar'),
      ).length;
      const deps = idx.dependencies || {};
      const loaderKey = ['neoforge', 'forge', 'fabric-loader', 'quilt-loader'].find((k) => deps[k]);
      ui.openSheet(
        ui.shell(
          ui.escAttr(idx.name || '整合包'),
          ui.escAttr(idx.summary || ''),
          `<div class="stack" style="gap:8px">
             <div class="kv"><span class="k">Minecraft</span><span class="v">${ui.escAttr(deps.minecraft || '未声明')}</span></div>
             <div class="kv"><span class="k">加载器</span><span class="v">${ui.escAttr(loaderKey ? `${loaderKey} ${deps[loaderKey]}` : '原版')}</span></div>
             <div class="kv"><span class="k">模组文件</span><span class="v">${mods} 个</span></div>
             <div class="kv"><span class="k">总文件</span><span class="v">${(idx.files || []).length} 项</span></div>
             <div class="mini-note">导入会铺到 versions/ 下的独立实例，不影响其它版本。</div>
           </div>`,
          `<button class="btn" data-sheet-close>取消</button>
           <button class="btn primary" id="mrpack-ok">导入</button>`,
        ),
      );
      document.getElementById('mrpack-ok').onclick = async () => {
        const btn = document.getElementById('mrpack-ok');
        ui.busy(btn, true, '导入中');
        ui.showProgress('导入整合包');
        try {
          const rec = await invoke('modpack_import', { path, name: null });
          ui.updateProgress({ frac: 1, label: '文件就绪' });
          ui.closeSheet();
          await afterModpackImport(rec);
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
      toast(`整合包「${rec.name}」已就绪`, 'ok');
      return;
    }

    ui.openSheet(
      ui.shell(
        '整合包文件已铺好',
        '还差游戏本体或加载器',
        `<div class="mini-note">「${ui.escAttr(rec.name)}」需要 Minecraft ${ui.escAttr(rec.gameVersion)}${
          rec.loader ? ` + ${ui.escAttr(rec.loader)} ${ui.escAttr(rec.loaderVersion)}` : ''
        }。可以现在补齐，也可以稍后自己装。</div>
         <div class="section-gap stack" style="gap:8px">
           ${
             missingGame
               ? `<button class="btn primary" id="mp-install-game">安装 Minecraft ${ui.escAttr(rec.gameVersion)}</button>`
               : ''
           }
           ${
             rec.loader && (missingGame || missingJar || missingLibs)
               ? `<button class="btn" id="mp-install-loader">安装 ${ui.escAttr(rec.loader)} ${ui.escAttr(rec.loaderVersion)}</button>`
               : ''
           }
           <button class="btn" id="mp-install-deps">只补齐依赖库与资源</button>
         </div>`,
        `<button class="btn" data-sheet-close>稍后</button>`,
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
        ui.showProgress(`安装 ${rec.loader} ${rec.loaderVersion}`);
        try {
          await invoke('install_loader', {
            kind: rec.loader,
            game: rec.gameVersion,
            loaderVersion: rec.loaderVersion,
          });
          ui.updateProgress({ frac: 1, label: '完成' });
          setTimeout(() => ui.hideProgress(), 600);
          S.installed = await invoke('scan_versions');
          GM.app.renderAll();
          toast('加载器已装好', 'ok');
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
    ui.showProgress('安装 ' + versionId);
    try {
      const res = await invoke('install_version', { versionId });
      ui.updateProgress({ frac: 1, label: '完成' });
      setTimeout(() => ui.hideProgress(), 600);
      S.installed = await invoke('scan_versions');
      GM.app.renderAll();
      if (res.failed && res.failed.length) toast(`有 ${res.failed.length} 个文件失败`, '');
      else toast('安装完成', 'ok');
    } catch (e) {
      ui.hideProgress();
      toast(String(e).split('\n')[0], 'err');
    }
  }

  async function exportModpack() {
    const cur = GM.app.selectedVersion();
    if (!cur) {
      toast('先选一个版本', 'err');
      return;
    }
    const dest = await invoke('pick_save_path', { defaultName: `${cur.id}.mrpack` });
    if (!dest) return;
    ui.showProgress('导出整合包');
    try {
      ui.updateProgress({ frac: 0.4, label: '收集模组与配置…' });
      const msg = await invoke('modpack_export', {
        versionId: cur.id,
        destination: dest,
        name: cur.id,
        summary: '由 GlassMC 导出',
      });
      ui.updateProgress({ frac: 1, label: '完成' });
      setTimeout(() => ui.hideProgress(), 600);
      toast('已导出：' + msg, 'ok');
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
              <span class="tag">${ui.escAttr(p.loader || '原版')} ${ui.escAttr(p.loaderVersion || '')}</span>
              <span class="tag">${p.modCount} 个模组</span>
              <span class="mono-dim">${ui.escAttr(p.instanceId)}</span></div>
          </div>
          <div class="v-actions"><button class="btn sm" data-play="${ui.escAttr(p.instanceId)}">启动</button></div>
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

  const STATE_TEXT = {
    waiting: '待机中',
    'host-scanning': '正在扫描本机局域网房间…',
    'host-starting': '正在创建隧道…',
    'host-ok': '房间已开启',
    'guest-connecting': '正在连接房主…',
    'guest-ok': '已加入房间',
    exception: '出现异常',
  };

  const DIFF_TEXT = {
    EASIEST: '网络很好，几乎秒连',
    SIMPLE: '网络良好，通常几秒内连上',
    MEDIUM: '网络一般，可能要等十几秒',
    TOUGH: '网络较严格，可能需要多试几次',
    UNKNOWN: '正在评估网络环境',
  };

  const EXC_TEXT = {
    0: '无法连接到房主：房间可能已关闭，或对方网络不稳定。',
    1: '与房主的连接中断了。',
    2: '本机的隧道组件崩溃了，请重试一次。',
    3: '房主那边的隧道组件崩溃了，请让房主重新创建房间。',
    4: '房主的游戏服务器已关闭（可能退出了存档），房间自动解散。',
    5: '房间协调协议出错，请稍后重试。',
  };

  function renderTc() {
    const box = document.getElementById('tc-status');
    if (!box) return;
    const s = tcSnap;
    const ver = document.getElementById('tc-ver');
    if (ver) ver.textContent = s && s.ready ? `${s.version}${s.easytier ? ' · ' + s.easytier : ''}` : '—';

    if (!s) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>还没有连接组件</span></div>';
      renderMembers([]);
      return;
    }
    if (s.preparing) {
      box.innerHTML = `<div class="empty"><span class="spinner"></span><span>${ui.escAttr(s.prepareText || '正在准备…')}</span></div>`;
      renderMembers([]);
      return;
    }
    if (!s.ready) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${
        s.error ? ui.escAttr(s.error) : '点右上角「启动联机组件」'
      }</span></div>`;
      renderMembers([]);
      return;
    }
    const st = s.state || {};
    const state = st.state || 'waiting';
    const label = STATE_TEXT[state] || state;
    const isOk = state === 'host-ok' || state === 'guest-ok';
    const isExc = state === 'exception';
    const parts = [];
    parts.push(
      `<div class="hstack">
        <span class="badge-pill ${isExc ? 'danger' : isOk ? 'ok' : 'accent'}"><span class="d"></span>${ui.escAttr(label)}</span>
        ${st.difficulty ? `<span class="tag">${ui.escAttr(DIFF_TEXT[st.difficulty] || st.difficulty)}</span>` : ''}
      </div>`,
    );
    if (st.room) {
      parts.push(
        `<div class="code-box" style="padding:11px 13px">
           <div>
             <div class="mini-note">邀请码</div>
             <div class="code" style="font-size:15px">${ui.escAttr(st.room)}</div>
           </div>
           <button class="btn sm" id="tc-copy" style="margin-left:auto">复制</button>
         </div>`,
      );
    }
    if (isExc && st.type != null) {
      parts.push(`<div class="mini-note" style="color:var(--danger)">${ui.escAttr(EXC_TEXT[st.type] || '出现了未知错误。')}</div>`);
    }
    if (st.url) {
      parts.push(`<div class="kv"><span class="k">隧道地址</span><span class="v mono-dim">${ui.escAttr(st.url)}</span></div>`);
    }
    parts.push('<div class="mini-note">房主需要先在游戏里「对局域网开放」，再点创建房间。</div>');
    box.innerHTML = parts.join('');
    const copy = document.getElementById('tc-copy');
    if (copy) {
      copy.onclick = () => {
        navigator.clipboard.writeText(st.room || '').then(
          () => toast('邀请码已复制', 'ok'),
          () => toast('复制失败，请手动选中', 'err'),
        );
      };
    }
    renderMembers(st.profiles || []);
  }

  function renderMembers(list) {
    const box = document.getElementById('tc-members');
    const count = document.getElementById('tc-count');
    if (count) count.textContent = list.length ? list.length + ' 人' : '—';
    if (!box) return;
    box.innerHTML = list.length
      ? list
          .map((p) => {
            const role = p.kind === 'HOST' ? '房主' : p.kind === 'GUEST' ? '成员' : '本机';
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
      : '<div class="empty"><span class="big-ico">◌</span><span>房间里还没有人</span></div>';
  }

  async function tcBootstrap(btn) {
    ui.busy(btn, true, '启动中');
    tcSnap = Object.assign({}, tcSnap, { preparing: true, prepareText: '正在准备联机组件…' });
    renderTc();
    try {
      tcSnap = await invoke('tc_bootstrap');
      renderTc();
      startTcPolling();
      toast('联机组件已就绪', 'ok');
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
    if (count) count.textContent = list.length + ' 个';
    box.innerHTML = list
      .map(
        (s) => `<div class="vrow">
        <div class="v-main">
          <div class="v-id" style="font-size:12px">${ui.escAttr(s.name)}</div>
          <div class="v-meta"><span class="mono-dim">${ui.escAttr(s.baseUrl)}</span>
            ${s.isPreset ? '<span class="tag">内置</span>' : ''}</div>
        </div>
        <div class="v-actions">${
          s.isPreset ? '' : `<button class="btn sm danger" data-del-srv="${ui.escAttr(s.id)}">删除</button>`
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
      if (el) el.textContent = s.installed ? `已就绪${s.version ? ' · ' + s.version : ''}` : '未安装';
      if (state) state.textContent = s.installed ? '注入器已就绪' : '注入器未安装';
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
            .then(() => toast('已启动', 'ok'))
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
        toast('组件已关闭', 'ok');
      };
      document.getElementById('btn-tc-create').onclick = (e) =>
        tcAction(() => invoke('tc_create', { player: tcPlayer() }), e.currentTarget, '创建');
      document.getElementById('btn-tc-join').onclick = (e) => {
        const room = (document.getElementById('tc-room') || {}).value || '';
        if (!room.trim()) {
          toast('先填邀请码', 'err');
          return;
        }
        tcAction(() => invoke('tc_join', { room: room.trim(), player: tcPlayer() }), e.currentTarget, '加入');
      };
      document.getElementById('btn-tc-leave').onclick = (e) =>
        tcAction(() => invoke('tc_leave'), e.currentTarget, '返回');
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
          toast('已删除', 'ok');
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
          toast('已添加', 'ok');
        } catch (e) {
          toast(String(e), 'err');
        }
      };
    }

    const inj = document.getElementById('btn-injector');
    if (inj && !inj.dataset.bound) {
      inj.dataset.bound = '1';
      inj.onclick = async (e) => {
        ui.busy(e.currentTarget, true, '下载中');
        try {
          await invoke('injector_install', { force: true });
          await renderInjector();
          toast('注入器已就绪', 'ok');
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
        toast('已指定 java', 'ok');
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
        toast('已恢复自动挑选', 'ok');
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
      jm.textContent = S.config.javaPath ? '已手动指定：' + S.config.javaPath : '当前自动挑选版本最接近的 Java';
    }
  }

  HOOKS.render.push(() => {
    if (!S) return;
    renderSettingsExtra();
  });

  GM.importer = { scan: scanLaunchers, pick: pickModpack, export: exportModpack };
  GM.multiplayer = { refresh: renderTc };
})();
