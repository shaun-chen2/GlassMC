/* =====================================================================
 * features-install.js —— 安装（原版 / 加载器 / Java）与模组（本地 / Modrinth）
 * ===================================================================== */
(function () {
  const HOOKS = window.__GM_HOOKS;
  const GM = (window.GM = window.GM || {});
  const ui = GM.ui;
  let invoke = null;
  let S = null;
  const t = (k, v) => window.GM.t(k, v);
  const toast = (m, k) => GM.ui.toast(m, k);

  // ---------------------------------------------------------------- 原版安装

  let installing = new Set();

  async function installVanilla(versionId, btn) {
    if (installing.has(versionId)) return;
    installing.add(versionId);
    const label = btn ? btn.textContent : '';
    ui.showProgress(t('install.progress.installing', { id: versionId }));
    if (btn) {
      btn.disabled = true;
      btn.innerHTML = '<span class="spinner"></span>';
    }
    try {
      const res = await invoke('install_version', { versionId });
      ui.updateProgress({ frac: 1, label: t('install.progress.done') });
      setTimeout(() => ui.hideProgress(), 700);
      S.installed = await invoke('scan_versions');
      GM.app.renderAll();
      if (res.failed && res.failed.length) {
        toast(t('install.toast.partial', { n: res.failed.length }), '');
        console.warn('[install] 失败明细', res.failed);
      } else {
        toast(t('install.toast.installed', { id: versionId }), 'ok');
      }
    } catch (e) {
      ui.hideProgress();
      toast(String(e).split('\n')[0], 'err');
      console.error(e);
    } finally {
      installing.delete(versionId);
      if (btn) {
        btn.disabled = false;
        btn.textContent = label || t('install.btn.install');
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
      : `<option value="">${t('install.loader.no_game_opt')}</option>`;
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
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${t('install.loader.pick_game')}</span></div>`;
      if (count) count.textContent = '—';
      return;
    }
    if (loaderState.loading) return;
    loaderState.loading = true;
    box.innerHTML = `<div class="empty"><span class="spinner"></span><span>${t('install.loader.loading')}</span></div>`;
    try {
      const list = await invoke('loader_versions', { kind: loaderState.kind, game });
      loaderState.versions = list;
      if (count) count.textContent = t('install.count', { n: list.length });
      box.innerHTML = list
        .map(
          (v, i) => `<div class="vrow">
            <div class="v-main">
              <div class="v-id" style="font-size:12.5px">${ui.escAttr(v)}</div>
              <div class="v-meta">${i === 0 ? `<span class="badge-pill ok"><span class="d"></span>${t('install.loader.latest')}</span>` : ''}
                <span class="mono-dim">${KIND_LABEL[loaderState.kind]} · MC ${ui.escAttr(game)}</span></div>
            </div>
            <div class="v-actions"><button class="btn sm" data-loader-install="${ui.escAttr(v)}">${t('install.btn.install')}</button></div>
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
          ? t('install.loader.hint_java21')
          : t('install.loader.hint_direct');
    }
  }

  async function installLoader(version, btn) {
    const game = loaderState.game;
    if (!game) {
      toast(t('install.loader.pick_game'), 'err');
      return;
    }
    ui.showProgress(t('install.progress.installing_loader', { kind: KIND_LABEL[loaderState.kind], version }));
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
      ui.updateProgress({ frac: 1, label: t('install.progress.done') });
      setTimeout(() => ui.hideProgress(), 700);
      S.installed = await invoke('scan_versions');
      GM.app.renderAll();
      toast(t('install.toast.installed', { id: res.versionId }), 'ok');
      GM.app.go('versions');
    } catch (e) {
      ui.hideProgress();
      const msg = String(e);
      toast(msg.split('\n')[0], 'err');
      ui.openSheet(
        ui.shell(
          t('install.loader.fail_title'),
          t('install.loader.fail_sub'),
          `<div class="mini-note" style="white-space:pre-wrap;max-height:40vh;overflow:auto;font-family:var(--font-mono)">${ui.escAttr(msg)}</div>`,
          `<button class="btn" data-sheet-close>${t('install.btn.got_it')}</button>`,
        ),
      );
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = t('install.btn.install');
      }
    }
  }

  // ---------------------------------------------------------------- Java

  function renderJava() {
    const list = (S.boot.java || []).slice();
    const el = document.getElementById('java-count2');
    if (el) el.textContent = t('install.count', { n: list.length });
    const box = document.getElementById('java-list2');
    if (box) {
      box.innerHTML = list.length
        ? list
            .map(
              (j, i) => `<div class="vrow">
              <div class="v-main">
                <div class="v-id" style="font-size:12.5px">Java ${j.version} · ${ui.escAttr(j.vendor)}</div>
                <div class="v-meta"><span class="mono-dim">${ui.escAttr(j.path)}</span>
                  ${j.source === 'managed' ? `<span class="tag">${t('install.java.managed')}</span>` : ''}</div>
              </div>
              <div class="v-actions">${i === 0 ? `<span class="badge-pill ok"><span class="d"></span>${t('install.java.default')}</span>` : ''}
                ${j.source === 'managed' ? `<button class="btn sm danger" data-del-java="${j.version}">${t('common.delete')}</button>` : ''}</div>
            </div>`,
            )
            .join('')
        : `<div class="empty"><span class="big-ico">◌</span><span>${t('install.java.empty')}</span></div>`;
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
             ${c.label}${c.installed ? t('install.java.installed_suffix') : ''}
           </button>`,
      )
      .join('');
  }

  async function installJava(feature, btn) {
    ui.showProgress(t('install.progress.installing_java', { feature }));
    ui.busy(btn, true, t('install.busy.downloading'));
    try {
      await invoke('install_java', { feature });
      ui.updateProgress({ frac: 1, label: t('common.done') });
      setTimeout(() => ui.hideProgress(), 600);
      S.boot.java = await invoke('list_java_all');
      javaCands = [];
      renderJava();
      GM.app.renderSettings();
      toast(t('install.toast.java_ready', { feature }), 'ok');
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
        ? `${cur.id} · ${t('install.mods.file_count', { n: modsState.items.length })}`
        : t('install.mods.pick_version');
    }
    const kw = (document.getElementById('mods-filter') || {}).value || '';
    const list = modsState.items.filter(
      (m) => !kw || (m.name || '').toLowerCase().includes(kw.toLowerCase()) || (m.fileName || '').toLowerCase().includes(kw.toLowerCase()),
    );
    if (!list.length) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${t('install.mods.empty')}</span></div>`;
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
            ${m.enabled ? '' : `<span class="badge-pill warn"><span class="d"></span>${t('install.mods.disabled')}</span>`}
          </div>
        </div>
        <div class="v-actions">
          <button class="btn sm" data-mod-toggle="${ui.escAttr(m.path)}" data-enabled="${m.enabled ? '1' : '0'}">${m.enabled ? t('install.mods.disable') : t('install.mods.enable')}</button>
          <button class="btn sm danger" data-mod-del="${ui.escAttr(m.path)}">${t('common.delete')}</button>
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
        ? t('install.mr.context', {
            game: mrState.game || t('install.mr.unknown_ver'),
            loader: mrState.loader || t('install.mr.vanilla'),
            id: cur.id,
          })
        : t('install.mods.pick_version');
    }
  }

  async function searchModrinth(more) {
    if (mrState.busy) return;
    const q = (document.getElementById('mr-filter') || {}).value || '';
    if (!more) mrState.offset = 0;
    mrState.busy = true;
    const box = document.getElementById('mr-list');
    if (box && !more) box.innerHTML = `<div class="empty"><span class="spinner"></span><span>${t('install.mr.searching')}</span></div>`;
    const btn = document.getElementById('btn-mr-search');
    ui.busy(btn, true, t('install.mr.search'));
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
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${t('install.mr.empty')}</span></div>`;
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
            <div class="v-meta"><span class="tag">${t('install.mr.downloads', { n: fmtDownloads(it.downloads) })}</span>
              <span class="mono-dim">${ui.escAttr(it.author)}</span></div>
          </div>
          <div class="v-actions">
            <button class="btn sm" data-mr-install="${ui.escAttr(it.projectId)}" data-mr-title="${ui.escAttr(it.title)}">${t('install.btn.install')}</button>
          </div>
        </div>`,
        )
        .join('') +
      (mrState.items.length < mrState.total
        ? `<div class="hstack" style="justify-content:center;margin-top:8px"><button class="btn sm" id="mr-more">${t('install.mr.more')}</button></div>`
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
      toast(t('install.mods.pick_version'), 'err');
      return;
    }
    const optEl = document.getElementById('mr-optional');
    ui.busy(btn, true, t('install.btn.install'));
    ui.showProgress(t('install.progress.installing', { id: title }));
    try {
      const rep = await invoke('modrinth_install', {
        versionId: cur.id,
        projectId,
        game: mrState.game,
        loader: mrState.loader,
        mrVersion: null,
        includeOptional: !!(optEl && optEl.checked),
      });
      const inst = (rep && rep.installed) || [];
      const skip = (rep && rep.skipped) || [];
      const fail = (rep && rep.failed) || [];
      if (inst.length) {
        toast(t('install.toast.installed_many', { n: inst.length, list: inst.join('、') }), 'ok');
      } else if (skip.length) {
        toast(t('install.toast.already', { list: skip.join('、') }), 'ok');
      } else {
        toast(t('install.toast.nothing'), 'err');
      }
      if (fail.length) {
        // 前置装不上不算致命错误（比如作者只发了 Forge 版），列出来让用户自己判断
        ui.openSheet(
          ui.shell(
            t('install.deps.title'),
            t('install.deps.sub', { n: fail.length }),
            `<div class="stack" style="gap:8px">${fail
              .map(
                (f) =>
                  `<div class="kv"><span class="k">${ui.escAttr(f.title || f.projectId || t('install.deps.fallback'))}</span><span class="v">${ui.escAttr(f.reason)}</span></div>`,
              )
              .join('')}</div>`,
            `<button class="btn" data-sheet-close>${t('install.btn.got_it')}</button>`,
          ),
        );
      }
      await loadMods(true);
    } catch (e) {
      toast(String(e).split('\n')[0], 'err');
    } finally {
      ui.hideProgress();
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
        `<button class="btn sm" data-open-dir="${ui.escAttr(id)}">${t('install.ver.dir')}</button>
         <button class="btn sm danger" data-del-ver="${ui.escAttr(id)}">${t('common.delete')}</button>`,
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
        ui.confirm(t('install.confirm.del_title', { id }), t('install.confirm.del_sub'), async () => {
          try {
            await invoke('delete_version', { versionId: id });
            S.installed = await invoke('scan_versions');
            GM.app.renderAll();
            toast(t('install.toast.version_deleted', { id }), 'ok');
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
        ui.confirm(t('install.confirm.del_java_title'), t('install.confirm.del_java_sub'), async () => {
          try {
            await invoke('remove_java', { feature: Number(dj.dataset.delJava) });
            S.boot.java = await invoke('list_java_all');
            javaCands = [];
            renderJava();
            toast(t('install.toast.deleted'), 'ok');
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
        ui.confirm(t('install.confirm.del_mod_title'), t('install.confirm.del_mod_sub'), async () => {
          try {
            await invoke('mods_delete', { path: del.dataset.modDel });
            await loadMods(true);
            toast(t('install.toast.trashed'), 'ok');
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
