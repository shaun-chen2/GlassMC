/* =====================================================================
 * features-ops.js —— 实例运维：分类胶囊、每行「实例…」菜单
 *
 * 复制 / 分类 / 归档 / 桌面快捷方式四个命令都在后端，这里只做界面：
 * - 胶囊与筛选状态只存模块内，不动 app.js 的渲染逻辑；
 * - 每行按钮由渲染钩子事后补上，重复 renderAll 不会叠加（靠 data-* 判重）。
 * ===================================================================== */
(function () {
  const GM = window.GM;
  const HOOKS = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });
  const t = (key, vars) => GM.t('ops.' + key, vars);
  const esc = (s) => GM.ui.escAttr(s);

  const FILTER_ALL = 'all';
  const FILTER_NONE = '__none__'; // 「未分类」的占位 key，和真实分类名不会撞

  let invoke = null;
  let S = null;
  let filter = FILTER_ALL;

  /** ready 之前也可能有一次 renderAll，这里按需补引用。 */
  function refs() {
    if (window.__GM) {
      if (!invoke) invoke = window.__GM.invoke;
      if (!S) S = window.__GM.S;
    }
    return S;
  }

  function brief(e) {
    const msg = e && e.message ? e.message : e;
    return String(msg == null ? '' : msg).split('\n')[0];
  }

  // ---------------------------------------------------------------- 配置读取

  function cfg() {
    return (S && S.config) || {};
  }

  function cats() {
    const map = cfg().instanceCategories;
    return map && typeof map === 'object' ? map : {};
  }

  function catOf(id) {
    return cats()[id] || '';
  }

  function isArchived(id) {
    const list = cfg().archivedInstances;
    return Array.isArray(list) && list.indexOf(id) >= 0;
  }

  function showArchived() {
    const box = document.getElementById('chk-archived');
    return !!(box && box.checked);
  }

  /** 出现过的分类名，去重后按字典序排。 */
  function categoryNames() {
    const map = cats();
    const seen = Object.create(null);
    const out = [];
    Object.keys(map).forEach((id) => {
      const name = map[id];
      if (!name || seen[name]) return;
      seen[name] = 1;
      out.push(name);
    });
    return out.sort((a, b) => a.localeCompare(b));
  }

  // ---------------------------------------------------------------- 分类胶囊

  /** 某个筛选项在当前「显示归档」状态下的实例数。 */
  function countFor(key) {
    const list = (S && S.installed) || [];
    const arch = showArchived();
    return list.filter((v) => {
      const id = v.id || '';
      if (isArchived(id) && !arch) return false;
      if (key === FILTER_ALL) return true;
      if (key === FILTER_NONE) return !catOf(id);
      return catOf(id) === key;
    }).length;
  }

  function syncChips() {
    if (!refs()) return;
    const box = document.getElementById('inst-chips');
    if (!box) return;

    const names = categoryNames();
    // 分类被清空（或最后几个实例换了分类）后，选中项要退回「全部」
    if (filter !== FILTER_ALL && filter !== FILTER_NONE && names.indexOf(filter) < 0) filter = FILTER_ALL;

    const items = [
      { key: FILTER_ALL, label: t('filter.all') },
      { key: FILTER_NONE, label: t('filter.none') },
    ].concat(names.map((n) => ({ key: n, label: n })));

    box.innerHTML = items
      .map((it) => {
        const on = it.key === filter ? ' on' : '';
        return `<button class="chip${on}" data-ops-cat="${esc(it.key)}">${esc(it.label)}` +
          `<span class="mono-dim" style="margin-left:5px">${countFor(it.key)}</span></button>`;
      })
      .join('');
  }

  // ---------------------------------------------------------------- 行筛选

  function applyFilter() {
    const box = document.getElementById('versions-list');
    if (!box || !refs()) return;

    const arch = showArchived();
    const rows = box.querySelectorAll('.vrow');
    let visible = 0;

    rows.forEach((row) => {
      const id = row.dataset.pick || '';
      let ok = true;
      // 归档的默认不显示；勾了「显示归档」才参与分类筛选
      if (isArchived(id) && !arch) ok = false;
      else if (filter === FILTER_NONE) ok = !catOf(id);
      else if (filter !== FILTER_ALL) ok = catOf(id) === filter;
      row.style.display = ok ? '' : 'none';
      if (ok) visible++;
    });

    // 筛选到空时不改列表内容，只补一条提示；下次渲染会被 innerHTML 冲掉
    const old = box.querySelector('[data-ops-empty]');
    if (old) old.remove();
    if (!visible && rows.length) {
      const hint = document.createElement('div');
      hint.className = 'empty';
      hint.dataset.opsEmpty = '1';
      hint.innerHTML = `<span class="big-ico">◌</span><span>${esc(t('empty'))}</span>`;
      box.appendChild(hint);
    }
  }

  // ---------------------------------------------------------------- 行增强

  /** 给每行补「实例…」按钮与分类 / 归档徽章；已有则跳过，保证幂等。 */
  function enhanceRows() {
    const box = document.getElementById('versions-list');
    if (!box || !refs()) return;

    box.querySelectorAll('.vrow').forEach((row) => {
      const id = row.dataset.pick;
      if (!id) return;

      const acts = row.querySelector('.v-actions');
      if (acts && !acts.querySelector('[data-inst-menu]')) {
        acts.insertAdjacentHTML(
          'beforeend',
          `<button class="btn sm" data-inst-menu="${esc(id)}">${esc(t('menu'))}</button>`,
        );
      }

      const meta = row.querySelector('.v-meta');
      if (!meta) return;
      const name = catOf(id);
      if (name && !meta.querySelector('[data-ops-cat-badge]')) {
        meta.insertAdjacentHTML(
          'beforeend',
          `<span class="badge-pill" data-ops-cat-badge="${esc(name)}">${esc(name)}</span>`,
        );
      }
      if (isArchived(id) && !meta.querySelector('[data-ops-arch-badge]')) {
        meta.insertAdjacentHTML(
          'beforeend',
          `<span class="badge-pill warn" data-ops-arch-badge="1">${esc(t('badge.archived'))}</span>`,
        );
      }
    });
  }

  // ---------------------------------------------------------------- 实例菜单

  function openMenu(id) {
    const archived = isArchived(id);
    const items = [
      ['copy', t('copy')],
      ['cat', t('cat')],
      ['arch', archived ? t('unarch') : t('arch')],
      ['shortcut', t('shortcut')],
    ];
    const body = `<div class="stack" style="gap:8px">${items
      .map(
        ([act, label]) =>
          `<button class="btn" data-ops-act="${act}" style="justify-content:flex-start">${esc(label)}</button>`,
      )
      .join('')}</div>`;

    GM.ui.openSheet(
      GM.ui.shell(
        esc(t('menu.title', { id })),
        esc(t('menu.sub')),
        body,
        `<button class="btn" data-sheet-close>${esc(t('close'))}</button>`,
      ),
    );

    const sheet = document.getElementById('sheet');
    if (!sheet) return;
    sheet.querySelectorAll('[data-ops-act]').forEach((btn) => {
      btn.onclick = () => {
        const act = btn.dataset.opsAct;
        if (act === 'copy') openCopy(id);
        else if (act === 'cat') openCategory(id);
        else if (act === 'arch') {
          GM.ui.closeSheet();
          // 状态可能被别处改过，点下去时再取一次
          toggleArchive(id, !isArchived(id));
        } else if (act === 'shortcut') {
          GM.ui.closeSheet();
          makeShortcut(id);
        }
      };
    });
  }

  // ---------------------------------------------------------------- 复制

  function openCopy(id) {
    const body = `<div class="field">
        <div class="lbl">${esc(t('copy.name'))}</div>
        <input type="text" id="ops-copy-name" maxlength="64" value="${esc(id + t('copy.suffix'))}">
      </div>
      <div class="stack" style="gap:8px">
        <label class="hstack" style="gap:8px;font-size:12.5px">
          <input type="checkbox" id="ops-copy-mods"> ${esc(t('copy.withMods'))}
        </label>
        <label class="hstack" style="gap:8px;font-size:12.5px">
          <input type="checkbox" id="ops-copy-saves" checked> ${esc(t('copy.withSaves'))}
        </label>
      </div>`;

    GM.ui.openSheet(
      GM.ui.shell(
        esc(t('copy.title', { id })),
        esc(t('copy.sub')),
        body,
        `<button class="btn primary" id="ops-copy-go">${esc(t('confirm'))}</button>
         <button class="btn" data-sheet-close>${esc(t('close'))}</button>`,
      ),
    );

    const go = document.getElementById('ops-copy-go');
    if (go) go.onclick = () => doCopy(id);
  }

  async function doCopy(id) {
    const nameEl = document.getElementById('ops-copy-name');
    const name = ((nameEl && nameEl.value) || '').trim();
    if (!name) {
      GM.app.toast(t('copy.empty'), 'err');
      if (nameEl) nameEl.focus();
      return;
    }
    const modsEl = document.getElementById('ops-copy-mods');
    const savesEl = document.getElementById('ops-copy-saves');

    GM.ui.closeSheet();
    GM.ui.showProgress(t('copy.running'));
    try {
      const newId = await invoke('copy_instance', {
        versionId: id,
        name,
        withMods: !!(modsEl && modsEl.checked),
        withSaves: !!(savesEl && savesEl.checked),
      });
      if (!newId) throw new Error(t('mock'));
      S.installed = await invoke('scan_versions');
      GM.ui.hideProgress();
      GM.app.renderAll();
      GM.app.toast(t('copy.done', { name: newId }), 'ok');
    } catch (e) {
      GM.ui.hideProgress();
      GM.app.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  // ---------------------------------------------------------------- 分类

  function openCategory(id) {
    const cur = catOf(id);
    const names = categoryNames();
    const chips = names.length
      ? `<div class="desc">${esc(t('cat.existing'))}</div>
         <div class="chips" style="margin-top:6px">${names
           .map((n) => `<button class="chip" data-ops-fill="${esc(n)}">${esc(n)}</button>`)
           .join('')}</div>`
      : '';

    GM.ui.openSheet(
      GM.ui.shell(
        esc(t('cat.title', { id })),
        esc(t('cat.sub')),
        `<div class="field">
           <div class="lbl">${esc(t('cat.name'))}</div>
           <input type="text" id="ops-cat-input" maxlength="40" value="${esc(cur)}" placeholder="${esc(t('cat.ph'))}">
           ${chips}
         </div>`,
        `<button class="btn primary" id="ops-cat-save">${esc(t('confirm'))}</button>
         <button class="btn" data-sheet-close>${esc(t('close'))}</button>`,
      ),
    );

    const input = document.getElementById('ops-cat-input');
    const sheet = document.getElementById('sheet');
    if (sheet) {
      sheet.querySelectorAll('[data-ops-fill]').forEach((btn) => {
        btn.onclick = () => {
          if (input) input.value = btn.dataset.opsFill;
        };
      });
    }
    const save = document.getElementById('ops-cat-save');
    if (save) save.onclick = () => saveCategory(id);
  }

  async function saveCategory(id) {
    const input = document.getElementById('ops-cat-input');
    const value = ((input && input.value) || '').trim();
    try {
      const next = await invoke('set_instance_category', { versionId: id, category: value });
      if (!next) throw new Error(t('mock'));
      S.config = next;
      GM.ui.closeSheet();
      GM.app.renderAll();
      GM.app.toast(value ? t('cat.done') : t('cat.cleared'), 'ok');
    } catch (e) {
      GM.app.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  // ---------------------------------------------------------------- 归档 / 快捷方式

  async function toggleArchive(id, archived) {
    try {
      const next = await invoke('set_instance_archived', { versionId: id, archived });
      if (!next) throw new Error(t('mock'));
      S.config = next;
      GM.app.renderAll();
      GM.app.toast(t(archived ? 'arch.done' : 'unarch.done', { id }), 'ok');
    } catch (e) {
      GM.app.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  async function makeShortcut(id) {
    try {
      const path = await invoke('create_instance_shortcut', { versionId: id });
      if (!path) throw new Error(t('mock'));
      GM.app.toast(t('shortcut.ok', { path }), 'ok');
    } catch (e) {
      GM.app.toast(t('fail', { err: brief(e) }), 'err');
    }
  }

  // ---------------------------------------------------------------- 事件绑定

  function bindVersionsPage() {
    const page = document.querySelector('[data-page="versions"]');
    if (!page || page.dataset.opsBound) return;
    page.dataset.opsBound = '1';

    // 委托：胶囊与行内按钮都是每次渲染重建的，不能逐个挂 onclick
    page.addEventListener('click', (e) => {
      const chip = e.target.closest('[data-ops-cat]');
      if (chip) {
        filter = chip.dataset.opsCat || FILTER_ALL;
        syncChips();
        applyFilter();
        return;
      }
      const menu = e.target.closest('[data-inst-menu]');
      if (menu) {
        openMenu(menu.dataset.instMenu);
      }
    });
  }

  // ---------------------------------------------------------------- 钩子

  HOOKS.ready.push(() => {
    refs();
    bindVersionsPage();

    const chk = document.getElementById('chk-archived');
    if (chk && !chk.dataset.opsBound) {
      chk.dataset.opsBound = '1';
      chk.addEventListener('change', () => {
        syncChips();
        applyFilter();
      });
    }

    // 语言切换由 features-prefs 统一触发 renderAll，这里不再重复注册
  });

  HOOKS.render.push(() => {
    enhanceRows();
    syncChips();
    applyFilter();
  });

  HOOKS.nav.push((page) => {
    if (page !== 'versions') return;
    syncChips();
    applyFilter();
  });
})();
