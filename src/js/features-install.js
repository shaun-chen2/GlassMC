/* =====================================================================
 * features-install.js —— 安装（原版 / 加载器 / Java）与模组（本地 / Modrinth）
 * ===================================================================== */
(function () {
  const HOOKS = window.__GM_HOOKS;
  const GM = (window.GM = window.GM || {});
  const ui = GM.ui;
  let invoke = null;
  let S = null;
  const toast = (m, k) => GM.ui.toast(m, k);

  // ---------------------------------------------------------------- 原版安装

  let installing = new Set();

  async function installVanilla(versionId, btn) {
    if (installing.has(versionId)) return;
    installing.add(versionId);
    const label = btn ? btn.textContent : '';
    ui.showProgress('安装 ' + versionId);
    if (btn) {
      btn.disabled = true;
      btn.innerHTML = '<span class="spinner"></span>';
    }
    try {
      const res = await invoke('install_version', { versionId });
      ui.updateProgress({ frac: 1, label: '安装完成' });
      setTimeout(() => ui.hideProgress(), 700);
      S.installed = await invoke('scan_versions');
      GM.app.renderAll();
      if (res.failed && res.failed.length) {
        toast(`装好了，但有 ${res.failed.length} 个文件没下成功（可再点一次补齐）`, '');
        console.warn('[install] 失败明细', res.failed);
      } else {
        toast(`已安装 ${versionId}`, 'ok');
      }
    } catch (e) {
      ui.hideProgress();
      toast(String(e).split('\n')[0], 'err');
      console.error(e);
    } finally {
      installing.delete(versionId);
      if (btn) {
        btn.disabled = false;
        btn.textContent = label || '安装';
      }
    }
  }

  window.__GM_INSTALL = installVanilla;

  // ---------------------------------------------------------------- 加载器

  const loaderState = { kind: 'fabric', game: '', versions: [], loading: false };
  const KIND_LABEL = { fabric: 'Fabric', quilt: 'Quilt', neoforge: 'NeoForge', forge: 'Forge' };

  function vanillaVersions() {
    return (S.installed || []).filter((v) => !v.loader).map((v) => v.id);
  }

  function renderLoaderGames() {
    const sel = document.getElementById('loader-game');
    if (!sel) return;
    const list = vanillaVersions();
    const prev = sel.value;
    sel.innerHTML = list.length
      ? list.map((id) => `<option value="${ui.escAttr(id)}">${ui.escAttr(id)}</option>`).join('')
      : '<option value="">（还没有原版版本，先去「游戏版本」装一个）</option>';
    if (prev && list.includes(prev)) sel.value = prev;
    loaderState.game = sel.value || '';
  }

  function renderLoaderKinds() {
    const box = document.getElementById('loader-kinds');
    if (!box) return;
    box.innerHTML = Object.keys(KIND_LABEL)
      .map(
        (k) =>
          `<button class="btn sm ${k === loaderState.kind ? 'primary' : ''}" data-kind="${k}">${KIND_LABEL[k]}</button>`,
      )
      .join('');
  }

  async function loadLoaderVersions() {
    const box = document.getElementById('loader-versions');
    const count = document.getElementById('loader-ver-count');
    if (!box) return;
    const game = document.getElementById('loader-game').value;
    loaderState.game = game;
    if (!game) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>先选一个游戏版本</span></div>';
      if (count) count.textContent = '—';
      return;
    }
    if (loaderState.loading) return;
    loaderState.loading = true;
    box.innerHTML = '<div class="empty"><span class="spinner"></span><span>正在查询可用版本…</span></div>';
    try {
      const list = await invoke('loader_versions', { kind: loaderState.kind, game });
      loaderState.versions = list;
      if (count) count.textContent = list.length + ' 个';
      box.innerHTML = list
        .map(
          (v, i) => `<div class="vrow">
            <div class="v-main">
              <div class="v-id" style="font-size:12.5px">${ui.escAttr(v)}</div>
              <div class="v-meta">${i === 0 ? '<span class="badge-pill ok"><span class="d"></span>最新</span>' : ''}
                <span class="mono-dim">${KIND_LABEL[loaderState.kind]} · MC ${ui.escAttr(game)}</span></div>
            </div>
            <div class="v-actions"><button class="btn sm" data-loader-install="${ui.escAttr(v)}">安装</button></div>
          </div>`,
        )
        .join('');
    } catch (e) {
      box.innerHTML = `<div class="empty"><span class="big-ico">!</span><span>${ui.escAttr(String(e))}</span></div>`;
      if (count) count.textContent = '—';
    } finally {
      loaderState.loading = false;
    }
    const hint = document.getElementById('loader-hint');
    if (hint) {
      hint.textContent =
        loaderState.kind === 'neoforge' || loaderState.kind === 'forge'
          ? '这类加载器需要 Java 21 来跑官方安装器，可能要几分钟。'
          : '直接写版本文件，装完会自动补齐依赖。';
    }
  }

  async function installLoader(version, btn) {
    const game = loaderState.game;
    if (!game) {
      toast('先选一个游戏版本', 'err');
      return;
    }
    ui.showProgress(`安装 ${KIND_LABEL[loaderState.kind]} ${version}`);
    if (btn) {
      btn.disabled = true;
      btn.innerHTML = '<span class="spinner"></span>';
    }
    try {
      const res = await invoke('install_loader', {
        kind: loaderState.kind,
        game,
        loaderVersion: version,
      });
      ui.updateProgress({ frac: 1, label: '安装完成' });
      setTimeout(() => ui.hideProgress(), 700);
      S.installed = await invoke('scan_versions');
      GM.app.renderAll();
      toast(`已安装 ${res.versionId}`, 'ok');
      GM.app.go('versions');
    } catch (e) {
      ui.hideProgress();
      const msg = String(e);
      toast(msg.split('\n')[0], 'err');
      ui.openSheet(
        ui.shell(
          '加载器安装失败',
          '通常是 Java 版本或网络问题',
          `<div class="mini-note" style="white-space:pre-wrap;max-height:40vh;overflow:auto;font-family:var(--font-mono)">${ui.escAttr(msg)}</div>`,
          `<button class="btn" data-sheet-close>知道了</button>`,
        ),
      );
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = '安装';
      }
    }
  }

  // ---------------------------------------------------------------- Java

  function renderJava() {
    const list = (S.boot.java || []).slice();
    const el = document.getElementById('java-count2');
    if (el) el.textContent = list.length + ' 个';
    const box = document.getElementById('java-list2');
    if (box) {
      box.innerHTML = list.length
        ? list
            .map(
              (j, i) => `<div class="vrow">
              <div class="v-main">
                <div class="v-id" style="font-size:12.5px">Java ${j.version} · ${ui.escAttr(j.vendor)}</div>
                <div class="v-meta"><span class="mono-dim">${ui.escAttr(j.path)}</span>
                  ${j.source === 'managed' ? '<span class="tag">本启动器安装</span>' : ''}</div>
              </div>
              <div class="v-actions">${i === 0 ? '<span class="badge-pill ok"><span class="d"></span>默认</span>' : ''}
                ${j.source === 'managed' ? `<button class="btn sm danger" data-del-java="${j.version}">删除</button>` : ''}</div>
            </div>`,
            )
            .join('')
        : '<div class="empty"><span class="big-ico">◌</span><span>没有检测到 Java</span></div>';
    }
    renderJavaCandidates();
  }

  let javaCands = [];

  async function renderJavaCandidates() {
    const box = document.getElementById('java-install-list');
    if (!box) return;
    if (!javaCands.length) {
      try {
        javaCands = await invoke('java_candidates');
      } catch (e) {
        javaCands = [];
      }
    }
    box.innerHTML = javaCands
      .map(
        (c) =>
          `<button class="btn sm ${c.installed ? '' : 'primary'}" data-java-install="${c.feature}" ${c.installed ? 'disabled' : ''}>
             ${c.label}${c.installed ? ' · 已装' : ''}
           </button>`,
      )
      .join('');
  }

  async function installJava(feature, btn) {
    ui.showProgress(`安装 Adoptium JDK ${feature}`);
    ui.busy(btn, true, '下载中');
    try {
      await invoke('install_java', { feature });
      ui.updateProgress({ frac: 1, label: '完成' });
      setTimeout(() => ui.hideProgress(), 600);
      S.boot.java = await invoke('list_java_all');
      javaCands = [];
      renderJava();
      GM.app.renderSettings();
      toast(`Java ${feature} 已就绪`, 'ok');
    } catch (e) {
      ui.hideProgress();
      toast(String(e).split('\n')[0], 'err');
    } finally {
      ui.busy(btn, false);
    }
  }

  // ---------------------------------------------------------------- 模组

  const modsState = { versionId: null, items: [], filter: '', loading: false };

  function guessGame(v) {
    if (!v) return '';
    // 优先用 inheritsFrom（整合包实例的 id 里往往只有 1.7 这种短号）
    const fromParent = String(v.inherits || '').match(/1\.\d+(?:\.\d+)?/);
    if (fromParent) return fromParent[0];
    const m = String(v.id || '').match(/1\.\d+(?:\.\d+)?/);
    return m ? m[0] : '';
  }

  async function loadMods(force) {
    const cur = GM.app.selectedVersion ? GM.app.selectedVersion() : null;
    if (!cur) {
      modsState.items = [];
      renderMods();
      return;
    }
    if (!force && modsState.versionId === cur.id) {
      renderMods();
      return;
    }
    modsState.versionId = cur.id;
    modsState.loading = true;
    try {
      modsState.items = await invoke('mods_scan', { versionId: cur.id });
    } catch (e) {
      modsState.items = [];
      toast(String(e), 'err');
    }
    modsState.loading = false;
    renderMods();
  }

  function fmtSize(n) {
    if (!n) return '0 B';
    const u = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    let x = n;
    while (x >= 1024 && i < u.length - 1) {
      x /= 1024;
      i++;
    }
    return (i === 0 ? x : x.toFixed(1)) + ' ' + u[i];
  }

  function renderMods() {
    const box = document.getElementById('mods-list');
    if (!box) return;
    const cur = GM.app.selectedVersion ? GM.app.selectedVersion() : null;
    const sub = document.getElementById('mods-sub');
    if (sub) {
      sub.textContent = cur
        ? `${cur.id} · ${modsState.items.length} 个文件`
        : '先选一个版本';
    }
    const kw = (document.getElementById('mods-filter') || {}).value || '';
    const list = modsState.items.filter(
      (m) => !kw || (m.name || '').toLowerCase().includes(kw.toLowerCase()) || (m.fileName || '').toLowerCase().includes(kw.toLowerCase()),
    );
    if (!list.length) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>这个版本没有装模组</span></div>';
      return;
    }
    box.innerHTML = list
      .map(
        (m) => `<div class="vrow">
        <div class="v-main">
          <div class="v-id" style="font-size:12.5px">${ui.escAttr(m.name)}</div>
          <div class="v-meta">
            <span class="mono-dim">${ui.escAttr(m.version || '—')}</span>
            <span class="dot-sep">·</span>
            <span class="mono-dim">${fmtSize(m.size)}</span>
            <span class="dot-sep">·</span>
            <span class="mono-dim">${ui.escAttr(m.modId)}</span>
            ${m.enabled ? '' : '<span class="badge-pill warn"><span class="d"></span>已禁用</span>'}
          </div>
        </div>
        <div class="v-actions">
          <button class="btn sm" data-mod-toggle="${ui.escAttr(m.path)}" data-enabled="${m.enabled ? '1' : '0'}">${m.enabled ? '禁用' : '启用'}</button>
          <button class="btn sm danger" data-mod-del="${ui.escAttr(m.path)}">删除</button>
        </div>
      </div>`,
      )
      .join('');
  }

  // Modrinth
  const mrState = { offset: 0, total: 0, items: [], game: '', loader: '', busy: false };

  function syncMrContext() {
    const cur = GM.app.selectedVersion ? GM.app.selectedVersion() : null;
    mrState.game = guessGame(cur);
    mrState.loader = cur && cur.loader ? cur.loader : '';
    const el = document.getElementById('mr-context');
    if (el) {
      el.textContent = cur
        ? `当前筛选：${mrState.game || '未知版本'} · ${mrState.loader || '原版'}（点安装会装到 ${cur.id}）`
        : '先选一个版本';
    }
  }

  async function searchModrinth(more) {
    if (mrState.busy) return;
    const q = (document.getElementById('mr-filter') || {}).value || '';
    if (!more) mrState.offset = 0;
    mrState.busy = true;
    const box = document.getElementById('mr-list');
    if (box && !more) box.innerHTML = '<div class="empty"><span class="spinner"></span><span>搜索中…</span></div>';
    const btn = document.getElementById('btn-mr-search');
    ui.busy(btn, true, '搜索');
    try {
      const [items, total] = await invoke('modrinth_search', {
        query: q,
        game: mrState.game,
        loader: mrState.loader,
        offset: mrState.offset,
      });
      mrState.total = total;
      if (more) mrState.items = mrState.items.concat(items);
      else mrState.items = items;
      renderMrList();
    } catch (e) {
      if (box) box.innerHTML = `<div class="empty"><span class="big-ico">!</span><span>${ui.escAttr(String(e))}</span></div>`;
    } finally {
      mrState.busy = false;
      ui.busy(btn, false);
    }
  }

  function fmtDownloads(n) {
    if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M';
    if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K';
    return String(n || 0);
  }

  function renderMrList() {
    const box = document.getElementById('mr-list');
    if (!box) return;
    if (!mrState.items.length) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>没有匹配的模组</span></div>';
      return;
    }
    box.innerHTML =
      mrState.items
        .map(
          (it) => `<div class="vrow">
          <div class="acc-avatar" style="border-radius:9px">${it.iconUrl ? `<img src="${ui.escAttr(it.iconUrl)}" alt="">` : '?'}</div>
          <div class="v-main">
            <div class="v-id" style="font-size:12.5px">${ui.escAttr(it.title)}</div>
            <div class="v-meta"><span class="mono-dim" style="max-width:420px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${ui.escAttr(it.description)}</span></div>
            <div class="v-meta"><span class="tag">${fmtDownloads(it.downloads)} 次下载</span>
              <span class="mono-dim">${ui.escAttr(it.author)}</span></div>
          </div>
          <div class="v-actions">
            <button class="btn sm" data-mr-install="${ui.escAttr(it.projectId)}" data-mr-title="${ui.escAttr(it.title)}">安装</button>
          </div>
        </div>`,
        )
        .join('') +
      (mrState.items.length < mrState.total
        ? '<div class="hstack" style="justify-content:center;margin-top:8px"><button class="btn sm" id="mr-more">加载更多</button></div>'
        : '');
    const more = document.getElementById('mr-more');
    if (more) {
      more.onclick = () => {
        mrState.offset += 20;
        searchModrinth(true);
      };
    }
  }

  async function installMod(projectId, title, btn) {
    const cur = GM.app.selectedVersion ? GM.app.selectedVersion() : null;
    if (!cur) {
      toast('先选一个版本', 'err');
      return;
    }
    ui.busy(btn, true, '安装');
    try {
      await invoke('modrinth_install', {
        versionId: cur.id,
        projectId,
        game: mrState.game,
        loader: mrState.loader,
        mrVersion: null,
      });
      toast(`已安装 ${title}`, 'ok');
      await loadMods(true);
    } catch (e) {
      toast(String(e).split('\n')[0], 'err');
    } finally {
      ui.busy(btn, false);
    }
  }

  // ---------------------------------------------------------------- 版本页增强

  /** app.js 渲染完版本行后，补上「目录 / 删除」两个操作。 */
  function enhanceVersions() {
    const box = document.getElementById('versions-list');
    if (!box) return;
    box.querySelectorAll('.vrow').forEach((row) => {
      const play = row.querySelector('[data-play]');
      if (!play) return;
      const id = play.dataset.play;
      const acts = row.querySelector('.v-actions');
      if (!acts || acts.querySelector('[data-open-dir]')) return;
      acts.insertAdjacentHTML(
        'beforeend',
        `<button class="btn sm" data-open-dir="${ui.escAttr(id)}">目录</button>
         <button class="btn sm danger" data-del-ver="${ui.escAttr(id)}">删除</button>`,
      );
    });
  }

  function bindVersionEvents() {
    const page = document.querySelector('[data-page="versions"]');
    if (!page || page.dataset.bound) return;
    page.dataset.bound = '1';
    page.addEventListener('click', async (e) => {
      const od = e.target.closest('[data-open-dir]');
      if (od) {
        try {
          const dir = await invoke('version_dir', { versionId: od.dataset.openDir });
          await invoke('open_dir', { path: dir });
        } catch (err) {
          toast(String(err), 'err');
        }
        return;
      }
      const dv = e.target.closest('[data-del-ver]');
      if (dv) {
        const id = dv.dataset.delVer;
        ui.confirm(`删除版本「${id}」？`, '版本目录会整个删掉，里面的存档与模组也随之消失。', async () => {
          try {
            await invoke('delete_version', { versionId: id });
            S.installed = await invoke('scan_versions');
            GM.app.renderAll();
            toast('已删除 ' + id, 'ok');
          } catch (err) {
            toast(String(err), 'err');
          }
        });
      }
    });
  }

  // ---------------------------------------------------------------- 事件绑定

  function bindInstallTabs() {
    document.querySelectorAll('[data-itab]').forEach((tab) => {
      tab.onclick = () => {
        document.querySelectorAll('[data-itab]').forEach((t) => t.classList.toggle('on', t === tab));
        document
          .querySelectorAll('[data-ipanel]')
          .forEach((p) => (p.style.display = p.dataset.ipanel === tab.dataset.itab ? '' : 'none'));
        if (tab.dataset.itab === 'loader') {
          renderLoaderGames();
          renderLoaderKinds();
          loadLoaderVersions();
        }
        if (tab.dataset.itab === 'java') renderJava();
      };
    });
    document.querySelectorAll('[data-mtab]').forEach((tab) => {
      tab.onclick = () => {
        document.querySelectorAll('[data-mtab]').forEach((t) => t.classList.toggle('on', t === tab));
        document
          .querySelectorAll('[data-mpanel]')
          .forEach((p) => (p.style.display = p.dataset.mpanel === tab.dataset.mtab ? '' : 'none'));
        if (tab.dataset.mtab === 'browse') syncMrContext();
      };
    });
  }

  function bindInstallEvents() {
    const page = document.querySelector('[data-page="install"]');
    if (!page || page.dataset.bound) return;
    page.dataset.bound = '1';

    page.addEventListener('click', (e) => {
      const kind = e.target.closest('[data-kind]');
      if (kind) {
        loaderState.kind = kind.dataset.kind;
        renderLoaderKinds();
        loadLoaderVersions();
        return;
      }
      const inst = e.target.closest('[data-loader-install]');
      if (inst) {
        installLoader(inst.dataset.loaderInstall, inst);
        return;
      }
      const jv = e.target.closest('[data-java-install]');
      if (jv && !jv.disabled) {
        installJava(Number(jv.dataset.javaInstall), jv);
        return;
      }
      const dj = e.target.closest('[data-del-java]');
      if (dj) {
        ui.confirm('删除这个 JDK？', '只会删本启动器安装的那一份。', async () => {
          try {
            await invoke('remove_java', { feature: Number(dj.dataset.delJava) });
            S.boot.java = await invoke('list_java_all');
            javaCands = [];
            renderJava();
            toast('已删除', 'ok');
          } catch (err) {
            toast(String(err), 'err');
          }
        });
      }
    });

    const sel = document.getElementById('loader-game');
    if (sel) {
      sel.onchange = () => {
        loaderState.game = sel.value;
        loadLoaderVersions();
      };
    }
  }

  function bindModEvents() {
    const page = document.querySelector('[data-page="mods"]');
    if (!page || page.dataset.bound) return;
    page.dataset.bound = '1';

    const f = document.getElementById('mods-filter');
    if (f) f.oninput = () => renderMods();

    document.getElementById('btn-mr-search').onclick = () => searchModrinth(false);
    const mrFilter = document.getElementById('mr-filter');
    if (mrFilter) {
      mrFilter.onkeydown = (e) => {
        if (e.key === 'Enter') searchModrinth(false);
      };
    }

    page.addEventListener('click', async (e) => {
      const tg = e.target.closest('[data-mod-toggle]');
      if (tg) {
        const enabled = tg.dataset.enabled === '1';
        try {
          await invoke('mods_toggle', { path: tg.dataset.modToggle, enabled: !enabled });
          await loadMods(true);
        } catch (err) {
          toast(String(err), 'err');
        }
        return;
      }
      const del = e.target.closest('[data-mod-del]');
      if (del) {
        ui.confirm('删除这个模组？', '文件会被移动到 mods/.trash，可以手动找回。', async () => {
          try {
            await invoke('mods_delete', { path: del.dataset.modDel });
            await loadMods(true);
            toast('已移到 .trash', 'ok');
          } catch (err) {
            toast(String(err), 'err');
          }
        });
        return;
      }
      const mi = e.target.closest('[data-mr-install]');
      if (mi) {
        installMod(mi.dataset.mrInstall, mi.dataset.mrTitle, mi);
        return;
      }
    });
  }

  // ---------------------------------------------------------------- 钩子

  HOOKS.ready.push(() => {
    invoke = window.__GM.invoke;
    S = window.__GM.S;
    bindInstallTabs();
    bindInstallEvents();
    bindModEvents();
    bindVersionEvents();
    renderJavaCandidates();
  });

  HOOKS.nav.push((page) => {
    if (page === 'mods') {
      loadMods(true);
    }
    if (page === 'install') {
      renderLoaderGames();
      renderLoaderKinds();
      renderJava();
    }
  });

  HOOKS.render.push(() => {
    if (!S) return;
    enhanceVersions();
    renderMods();
    renderJava();
    syncMrContext();
    renderLoaderGames();
  });

  GM.install = { vanilla: installVanilla, loader: installLoader, java: installJava };
  GM.mods = { reload: loadMods, render: renderMods };
})();
