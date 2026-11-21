/* =====================================================================
 * features.js —— 账号 / 皮肤 / 通用弹层与进度
 *
 * 分工：app.js 负责骨架与首页、版本、设置的基础字段；
 * 这里接管账号页，并提供 GM.ui / GM.accounts 给后面的模块复用。
 * ===================================================================== */
(function () {
  const HOOKS = (window.__GM_HOOKS = window.__GM_HOOKS || { ready: [], nav: [], render: [] });
  const GM = (window.GM = window.GM || {});
  const t = (k, v) => window.GM.t(k, v);

  // app.js 在三个时机回调这些函数
  window.__GM_ON_READY = () => HOOKS.ready.forEach((f) => f());
  window.__GM_ON_NAV = (page) => HOOKS.nav.forEach((f) => f(page));
  window.__GM_ON_RENDER = () => HOOKS.render.forEach((f) => f());

  let invoke = null;
  let S = null;
  let listenFn = null;

  // ---------------------------------------------------------------- 通用 UI

  const ui = (GM.ui = {
    openSheet(html, opts = {}) {
      const sheet = document.getElementById('sheet');
      sheet.className = 'sheet' + (opts.wide ? ' wide' : '');
      sheet.innerHTML = html;
      document.getElementById('modal').classList.add('on');
      return sheet;
    },
    closeSheet() {
      document.getElementById('modal').classList.remove('on');
      document.getElementById('sheet').innerHTML = '';
    },
    /** 带标题的弹层外壳 */
    shell(title, sub, body, foot) {
      return `<div class="sheet-head">
        <div>
          <h3>${title}</h3>
          ${sub ? `<div class="sub">${sub}</div>` : ''}
        </div>
        <button class="btn sm icon x" data-sheet-close aria-label="${GM.t('html.titlebar.close')}">✕</button>
      </div>
      ${body}
      ${foot ? `<div class="sheet-foot">${foot}</div>` : ''}`;
    },
    showProgress(title) {
      const card = document.getElementById('progress-card');
      document.getElementById('pg-title').textContent = title || t('acc.progress.working');
      document.getElementById('pg-label').textContent = t('acc.progress.preparing');
      document.getElementById('pg-pct').textContent = '0%';
      document.getElementById('pg-bar').style.width = '0%';
      card.classList.add('on');
    },
    updateProgress(p) {
      const card = document.getElementById('progress-card');
      if (!card) return;
      if (p.title) document.getElementById('pg-title').textContent = p.title;
      if (p.label) document.getElementById('pg-label').textContent = p.label;
      const pct = Math.max(0, Math.min(1, p.frac || 0));
      document.getElementById('pg-pct').textContent = Math.round(pct * 100) + '%';
      document.getElementById('pg-bar').style.width = pct * 100 + '%';
      if (!card.classList.contains('on')) card.classList.add('on');
    },
    hideProgress() {
      document.getElementById('progress-card').classList.remove('on');
    },
    busy(btn, on, text) {
      if (!btn) return;
      if (on) {
        if (!btn.dataset._html) btn.dataset._html = btn.innerHTML;
        btn.disabled = true;
        btn.innerHTML = '<span class="spinner"></span> ' + (text || t('acc.busy.working'));
      } else {
        btn.disabled = false;
        if (btn.dataset._html) {
          btn.innerHTML = btn.dataset._html;
          delete btn.dataset._html;
        }
      }
    },
  });

  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') ui.closeSheet();
  });
  document.addEventListener('click', (e) => {
    if (e.target.id === 'modal' || e.target.closest('[data-sheet-close]')) ui.closeSheet();
  });

  // ---------------------------------------------------------------- 皮肤渲染

  const skinCache = new Map(); // accountId -> dataURL
  const imgCache = new Map(); // dataURL -> HTMLImageElement

  function loadImage(dataUrl) {
    return new Promise((resolve) => {
      if (imgCache.has(dataUrl)) return resolve(imgCache.get(dataUrl));
      const img = new Image();
      img.onload = () => {
        imgCache.set(dataUrl, img);
        resolve(img);
      };
      img.onerror = () => resolve(null);
      img.src = dataUrl;
    });
  }

  /** 在 canvas 上画一个等距投影的 Minecraft 头（顶面 + 两侧面）。 */
  function drawIsoHead(canvas, img) {
    const S = 128;
    canvas.width = S;
    canvas.height = S;
    const ctx = canvas.getContext('2d');
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, S, S);
    if (!img) return;

    const w2 = 30; // 顶面半宽
    const h2 = 15; // 顶面半高
    const height = 34; // 侧面高度
    const cx = S / 2;
    const cy = S / 2 - height / 2;

    const T = [cx, cy - h2];
    const R = [cx + w2, cy];
    const B = [cx, cy + h2];
    const L = [cx - w2, cy];
    const L2 = [L[0], L[1] + height];
    const B2 = [B[0], B[1] + height];
    const R2 = [R[0], R[1] + height];

    // 面顺序：顶 → 左 → 右，避免接缝
    drawFace(ctx, img, [8, 0, 8, 8], T, R, L); // 顶面
    drawFace(ctx, img, [0, 8, 8, 8], L, B, L2); // 右侧面（角色右侧）
    drawFace(ctx, img, [8, 8, 8, 8], B, R, B2); // 正面
    drawFace(ctx, img, [40, 8, 8, 8], B, R, B2); // 帽子层（若有内容，覆盖上面）
  }

  function drawFace(ctx, img, src, p0, p1, p2) {
    const [sx, sy, sw, sh] = src;
    ctx.save();
    // 单位正方形 → 平行四边形(p0 左上, p1 右上, p2 左下)
    ctx.setTransform(
      (p1[0] - p0[0]) / sw,
      (p1[1] - p0[1]) / sw,
      (p2[0] - p0[0]) / sh,
      (p2[1] - p0[1]) / sh,
      p0[0],
      p0[1],
    );
    ctx.drawImage(img, sx, sy, sw, sh, 0, 0, sw, sh);
    ctx.restore();
  }

  function paintAvatar(el, dataUrl) {
    if (!dataUrl) return;
    loadImage(dataUrl).then((img) => {
      if (!img) return;
      const canvas = document.createElement('canvas');
      drawIsoHead(canvas, img);
      el.innerHTML = '';
      canvas.style.width = '100%';
      canvas.style.height = '100%';
      el.appendChild(canvas);
    });
  }

  async function skinFor(acc) {
    if (skinCache.has(acc.id)) return skinCache.get(acc.id);
    try {
      const url = await invoke('account_skin', { id: acc.id });
      if (url) skinCache.set(acc.id, url);
      return url || null;
    } catch (e) {
      return null;
    }
  }

  // ---------------------------------------------------------------- 账号页

  // 账号类型的文案 key；Mojang 是产品名，不翻译
  const TYPE_LABEL = {
    offline: 'acc.type.offline',
    msa: 'acc.type.msa',
    microsoft: 'acc.type.msa',
    yggdrasil: 'acc.type.yggdrasil',
    mojang: 'Mojang',
  };

  function accountRow(a, activeId) {
    const kind = a.type || a.kind || 'offline';
    const isActive = a.id === activeId;
    const kindLabel = TYPE_LABEL[kind] ? t(TYPE_LABEL[kind]) : kind;
    return `<div class="vrow">
      <div class="acc-avatar" data-avatar="${GM.ui.escAttr(a.id)}">${GM.ui.escAttr((a.username || '?').slice(0, 1).toUpperCase())}</div>
      <div class="v-main">
        <div class="v-id" style="font-size:13px">${GM.ui.escAttr(a.username)}
          ${isActive ? `<span class="badge-pill ok" style="margin-left:6px"><span class="d"></span>${t('acc.badge.active')}</span>` : ''}
        </div>
        <div class="v-meta">
          <span class="badge-pill ${kind === 'offline' ? '' : 'accent'}">${kindLabel}</span>
          ${a.needsReauth ? `<span class="badge-pill warn"><span class="d"></span>${t('acc.badge.reauth')}</span>` : ''}
          ${a.skinLocalPath ? `<span class="tag">${t('acc.badge.local_skin')}</span>` : ''}
          ${a.uuid ? `<span class="mono-dim">${GM.ui.escAttr(String(a.uuid).replace(/-/g, '').slice(0, 12))}</span>` : ''}
        </div>
      </div>
      <div class="v-actions">
        ${isActive ? '' : `<button class="btn sm" data-use-acc="${GM.ui.escAttr(a.id)}">${t('acc.btn.use')}</button>`}
        ${kind === 'offline' ? '' : `<button class="btn sm" data-refresh-acc="${GM.ui.escAttr(a.id)}">${t('acc.btn.refresh')}</button>`}
        ${kind === 'offline' ? '' : `<button class="btn sm" data-sync-skin="${GM.ui.escAttr(a.id)}">${t('acc.btn.sync_skin')}</button>`}
        <button class="btn sm danger" data-del-acc="${GM.ui.escAttr(a.id)}">${t('common.delete')}</button>
      </div>
    </div>`;
  }

  function renderAccountsEx() {
    const box = document.getElementById('accounts-list');
    if (!box) return;
    const list = (S.boot.accounts || []).slice().sort((a, b) => {
      const active = S.config.activeAccount;
      if (a.id === active) return -1;
      if (b.id === active) return 1;
      return (a.username || '').localeCompare(b.username || '');
    });
    if (!list.length) {
      box.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${t('acc.empty.accounts')}</span></div>`;
      return;
    }
    const activeId = S.config.activeAccount;
    box.innerHTML = list.map((a) => accountRow(a, activeId)).join('');

    // 头像异步补齐
    list.forEach(async (a) => {
      const el = box.querySelector(`[data-avatar="${cssEsc(a.id)}"]`);
      if (!el) return;
      const url = await skinFor(a);
      if (url) paintAvatar(el, url);
    });

    const site = document.getElementById('skinlib-site');
    if (site) site.textContent = S.config.skinLibraryBase || '—';
  }

  function cssEsc(v) {
    return String(v).replace(/["\\]/g, '\\$&');
  }

  // ---------------------------------------------------------------- 登录弹层

  async function offlineSheet() {
    ui.openSheet(
      ui.shell(
        t('acc.offline.title'),
        t('acc.offline.sub'),
        `<div class="field">
           <div class="lbl">${t('acc.field.game_name')}</div>
           <input type="text" id="offline-name" placeholder="${t('acc.offline.name_ph')}" value="">
         </div>
         <div class="mini-note" id="offline-err"></div>`,
        `<button class="btn" data-sheet-close>${t('common.cancel')}</button>
         <button class="btn primary" id="offline-ok">${t('acc.offline.create')}</button>`,
      ),
    );
    const input = document.getElementById('offline-name');
    input.focus();
    const submit = async () => {
      const name = input.value.trim();
      if (!name) {
        document.getElementById('offline-err').textContent = t('acc.offline.err_empty');
        return;
      }
      const btn = document.getElementById('offline-ok');
      ui.busy(btn, true, t('acc.offline.creating'));
      try {
        const list = await invoke('create_offline_account', { name });
        S.boot.accounts = list;
        S.config = await invoke('get_config');
        ui.closeSheet();
        GM.app.renderAll();
        GM.ui.toast(t('acc.toast.created', { name }), 'ok');
      } catch (e) {
        document.getElementById('offline-err').textContent = String(e);
        ui.busy(btn, false);
      }
    };
    document.getElementById('offline-ok').onclick = submit;
    input.onkeydown = (e) => {
      if (e.key === 'Enter') submit();
    };
  }

  async function yggSheet() {
    const servers = (S.config.authServers || []).map(
      (s) => `<option value="${GM.ui.escAttr(s.id)}">${GM.ui.escAttr(s.name)} · ${GM.ui.escAttr(s.baseUrl)}</option>`,
    );
    ui.openSheet(
      ui.shell(
        t('acc.ygg.title'),
        t('acc.ygg.sub'),
        `<div class="field">
           <div class="lbl">${t('acc.field.auth_server')}</div>
           <select id="ygg-server">${servers.join('')}</select>
           <div class="desc"><a href="#" id="ygg-add" style="color:var(--accent-hi)">${t('acc.ygg.add_custom')}</a></div>
         </div>
         <div class="field">
           <div class="lbl">${t('acc.field.email_user')}</div>
           <input type="text" id="ygg-user" autocomplete="username">
         </div>
         <div class="field">
           <div class="lbl">${t('acc.field.password')}</div>
           <input type="password" id="ygg-pass" autocomplete="current-password">
         </div>
         <div class="mini-note" id="ygg-err"></div>`,
        `<button class="btn" data-sheet-close>${t('common.cancel')}</button>
         <button class="btn primary" id="ygg-ok">${t('acc.btn.login')}</button>`,
      ),
    );
    document.getElementById('ygg-add').onclick = (e) => {
      e.preventDefault();
      ui.closeSheet();
      addServerSheet();
    };
    const submit = async () => {
      const serverId = document.getElementById('ygg-server').value;
      const username = document.getElementById('ygg-user').value.trim();
      const password = document.getElementById('ygg-pass').value;
      const err = document.getElementById('ygg-err');
      if (!username || !password) {
        err.textContent = t('acc.ygg.err_empty');
        return;
      }
      const btn = document.getElementById('ygg-ok');
      ui.busy(btn, true, t('acc.ygg.logging'));
      err.textContent = '';
      try {
        const res = await invoke('ygg_login', { serverId, username, password });
        S.boot.accounts = res.accounts;
        S.boot.activeAccount = res.account;
        S.config = await invoke('get_config');
        ui.closeSheet();
        GM.app.renderAll();
        GM.ui.toast(t('acc.toast.logged_in', { name: res.account.username }), 'ok');
      } catch (e) {
        err.textContent = String(e);
        ui.busy(btn, false);
      }
    };
    document.getElementById('ygg-ok').onclick = submit;
    document.getElementById('ygg-pass').onkeydown = (e) => {
      if (e.key === 'Enter') submit();
    };
  }

  async function addServerSheet() {
    ui.openSheet(
      ui.shell(
        t('acc.srv.title'),
        t('acc.srv.sub'),
        `<div class="field"><div class="lbl">${t('acc.field.name')}</div><input type="text" id="srv-new-name" placeholder="${t('acc.srv.name_ph')}"></div>
         <div class="field"><div class="lbl">${t('acc.field.api_base')}</div><input type="text" id="srv-new-base" placeholder="https://example.com/api/yggdrasil"></div>
         <div class="mini-note" id="srv-new-err"></div>`,
        `<button class="btn" data-sheet-close>${t('common.cancel')}</button>
         <button class="btn primary" id="srv-new-ok">${t('acc.btn.add')}</button>`,
      ),
    );
    document.getElementById('srv-new-ok').onclick = async () => {
      const name = document.getElementById('srv-new-name').value.trim();
      const base = document.getElementById('srv-new-base').value.trim();
      const err = document.getElementById('srv-new-err');
      try {
        const list = await invoke('add_auth_server', { name, baseUrl: base });
        S.config.authServers = list;
        ui.closeSheet();
        GM.app.renderSettings();
        GM.ui.toast(t('acc.toast.server_added', { name }), 'ok');
        yggSheet();
      } catch (e) {
        err.textContent = String(e);
      }
    };
  }

  let msTimer = null;

  async function msSheet() {
    ui.openSheet(
      ui.shell(
        t('acc.ms.title'),
        t('acc.ms.sub'),
        `<div id="ms-body"><div class="empty"><span class="spinner"></span><span>${t('acc.ms.requesting')}</span></div></div>`,
        `<button class="btn" data-sheet-close id="ms-cancel">${t('common.close')}</button>`,
      ),
    );
    document.getElementById('ms-cancel').onclick = () => {
      if (msTimer) clearInterval(msTimer);
      msTimer = null;
      ui.closeSheet();
    };
    try {
      const code = await invoke('ms_start');
      const body = document.getElementById('ms-body');
      body.innerHTML = `
        <div class="code-box">
          <div>
            <div class="mini-note">${t('acc.ms.enter_code')}</div>
            <div class="code">${GM.ui.escAttr(code.userCode)}</div>
          </div>
          <button class="btn sm" id="ms-open" style="margin-left:auto">${t('acc.ms.open_page')}</button>
        </div>
        <div class="mini-note section-gap" id="ms-status">${t('acc.ms.waiting')}</div>`;
      document.getElementById('ms-open').onclick = () =>
        invoke('open_url', { url: code.verificationUri }).catch(() => {});
      invoke('open_url', { url: code.verificationUri }).catch(() => {});

      let interval = Math.max(code.interval || 5, 2) * 1000;
      let stopped = false;
      const tick = async () => {
        if (stopped) return;
        try {
          const r = await invoke('ms_poll', { deviceCode: code.deviceCode });
          const st = document.getElementById('ms-status');
          if (st) st.textContent = r.message;
          if (r.status === 'done') {
            stopped = true;
            clearInterval(msTimer);
            msTimer = null;
            S.boot.accounts = await invoke('accounts_all');
            S.config = await invoke('get_config');
            ui.closeSheet();
            GM.app.renderAll();
            GM.ui.toast(t('acc.toast.login_ok', { name: r.account ? r.account.username : '' }), 'ok');
          } else if (r.status === 'slow_down') {
            interval += 5000;
            clearInterval(msTimer);
            msTimer = setInterval(tick, interval);
          }
        } catch (e) {
          stopped = true;
          clearInterval(msTimer);
          msTimer = null;
          const st = document.getElementById('ms-status');
          if (st) {
            st.innerHTML = `<span style="color:var(--danger)">${GM.ui.escAttr(String(e))}</span>`;
          }
        }
      };
      msTimer = setInterval(tick, interval);
      setTimeout(tick, 1200);
    } catch (e) {
      document.getElementById('ms-body').innerHTML =
        `<div class="mini-note" style="color:var(--danger)">${GM.ui.escAttr(String(e))}</div>
         <div class="mini-note section-gap">${t('acc.ms.not_started')}</div>`;
    }
  }

  // ---------------------------------------------------------------- 皮肤库

  let skinPage = { page: 1, last: 1 };

  async function skinlibSheet(page = 1) {
    ui.openSheet(
      ui.shell(
        t('acc.skinlib.title'),
        GM.ui.escAttr(S.config.skinLibraryBase || '') + ' · ' + t('acc.skinlib.sub'),
        `<div id="skin-grid-wrap"><div class="empty"><span class="spinner"></span><span>${t('acc.skinlib.loading')}</span></div></div>
         <div class="hstack section-gap" style="justify-content:center">
           <button class="btn sm" id="skin-prev">${t('acc.skinlib.prev')}</button>
           <span class="mono-dim" id="skin-pageno">1 / 1</span>
           <button class="btn sm" id="skin-next">${t('acc.skinlib.next')}</button>
         </div>`,
        `<button class="btn" data-sheet-close>${t('common.close')}</button>`,
      ),
      { wide: true },
    );
    const render = async (p) => {
      const wrap = document.getElementById('skin-grid-wrap');
      if (!wrap) return;
      wrap.innerHTML = `<div class="empty"><span class="spinner"></span><span>${t('acc.skinlib.loading')}</span></div>`;
      try {
        const res = await invoke('skinlib_page', { page: p });
        skinPage = { page: res.page, last: res.lastPage };
        document.getElementById('skin-pageno').textContent = `${res.page} / ${res.lastPage}`;
        if (!res.items.length) {
          wrap.innerHTML = `<div class="empty"><span class="big-ico">◌</span><span>${t('acc.skinlib.empty')}</span></div>`;
          return;
        }
        wrap.innerHTML = `<div class="skin-grid">${res.items
          .map(
            (it) => `<div class="skin-cell" data-tid="${it.tid}">
              <canvas class="skin-face" data-face="${it.tid}" width="64" height="64"></canvas>
              <div class="nm" title="${GM.ui.escAttr(it.name)}">${GM.ui.escAttr(it.name)}</div>
              <div class="au">${GM.ui.escAttr(it.author || t('acc.skinlib.anon'))} · ${it.kind === 'alex' ? 'Alex' : 'Steve'}</div>
            </div>`,
          )
          .join('')}</div>`;
        // 逐个加载缩略图，避免同时打 20 个请求被站点限流
        for (const it of res.items) {
          try {
            const thumb = await invoke('skinlib_thumb', { tid: it.tid });
            const img = await loadImage(thumb);
            const cv = wrap.querySelector(`[data-face="${it.tid}"]`);
            if (img && cv) drawIsoHead(cv, img);
          } catch (e) {
            /* 单张失败不影响其它 */
          }
          await new Promise((r) => setTimeout(r, 40));
        }
      } catch (e) {
        wrap.innerHTML = `<div class="empty"><span class="big-ico">!</span><span>${GM.ui.escAttr(String(e))}</span></div>`;
      }
    };
    document.getElementById('skin-prev').onclick = () => {
      if (skinPage.page > 1) render(skinPage.page - 1);
    };
    document.getElementById('skin-next').onclick = () => {
      if (skinPage.page < skinPage.last) render(skinPage.page + 1);
    };
    document.getElementById('skin-grid-wrap').addEventListener('click', async (e) => {
      const cell = e.target.closest('[data-tid]');
      if (!cell) return;
      const acc = activeAccount();
      if (!acc) {
        GM.ui.toast(t('acc.toast.no_account'), 'err');
        return;
      }
      const tid = Number(cell.dataset.tid);
      cell.style.opacity = '.5';
      try {
        await invoke('skinlib_apply', { tid, accountId: acc.id });
        skinCache.delete(acc.id);
        S.boot.accounts = await invoke('accounts_all');
        GM.app.renderAll();
        GM.ui.toast(t('acc.toast.skin_applied'), 'ok');
      } catch (err) {
        GM.ui.toast(String(err), 'err');
      } finally {
        cell.style.opacity = '';
      }
    });
    render(page);
  }

  function activeAccount() {
    const list = S.boot.accounts || [];
    return list.find((a) => a.id === S.config.activeAccount) || list[0] || null;
  }

  // ---------------------------------------------------------------- 账号页事件

  function bindAccountEvents() {
    const box = document.getElementById('accounts-list');
    if (!box || box.dataset.bound) return;
    box.dataset.bound = '1';
    box.addEventListener('click', async (e) => {
      const use = e.target.closest('[data-use-acc]');
      if (use) {
        try {
          S.config = await invoke('set_active_account', { id: use.dataset.useAcc });
          S.boot.activeAccount = (S.boot.accounts || []).find((a) => a.id === S.config.activeAccount) || null;
          GM.app.renderChrome();
          GM.app.renderAll();
          GM.ui.toast(t('acc.toast.switched'), 'ok');
        } catch (err) {
          GM.ui.toast(String(err), 'err');
        }
        return;
      }
      const del = e.target.closest('[data-del-acc]');
      if (del) {
        const acc = (S.boot.accounts || []).find((a) => a.id === del.dataset.delAcc);
        confirmSheet(t('acc.confirm.del_title', { name: acc ? acc.username : '' }), t('acc.confirm.del_sub'), async () => {
          try {
            S.boot.accounts = await invoke('remove_account', { id: del.dataset.delAcc });
            S.config = await invoke('get_config');
            GM.app.renderAll();
            GM.ui.toast(t('acc.toast.deleted'), 'ok');
          } catch (err) {
            GM.ui.toast(String(err), 'err');
          }
        });
        return;
      }
      const rf = e.target.closest('[data-refresh-acc]');
      if (rf) {
        ui.busy(rf, true, t('acc.busy.refresh'));
        try {
          const acc = await invoke('refresh_account', { id: rf.dataset.refreshAcc });
          S.boot.accounts = await invoke('accounts_all');
          skinCache.delete(acc.id);
          GM.app.renderAll();
          GM.ui.toast(t('acc.toast.refreshed'), 'ok');
        } catch (err) {
          GM.ui.toast(String(err), 'err');
        } finally {
          ui.busy(rf, false);
        }
        return;
      }
      const sy = e.target.closest('[data-sync-skin]');
      if (sy) {
        ui.busy(sy, true, t('acc.busy.sync'));
        try {
          const acc = await invoke('sync_account_skin', { id: sy.dataset.syncSkin });
          S.boot.accounts = await invoke('accounts_all');
          skinCache.delete(acc.id);
          try {
            const url = await invoke('refresh_account_skin', { id: acc.id });
            if (url) skinCache.set(acc.id, url);
          } catch (e) {
            /* 忽略：站点可能仍返回旧贴图 */
          }
          GM.app.renderAll();
          GM.ui.toast(t('acc.toast.skin_synced'), 'ok');
        } catch (err) {
          GM.ui.toast(String(err), 'err');
        } finally {
          ui.busy(sy, false);
        }
        return;
      }
    });
  }

  function confirmSheet(title, desc, onOk) {
    ui.openSheet(
      ui.shell(title, desc, `<div class="mini-note">${t('acc.confirm.irreversible')}</div>`,
        `<button class="btn" data-sheet-close>${t('common.cancel')}</button><button class="btn danger" id="confirm-ok">${t('common.confirm')}</button>`),
    );
    document.getElementById('confirm-ok').onclick = () => {
      ui.closeSheet();
      onOk();
    };
  }

  // ---------------------------------------------------------------- 导出给其它模块

  GM.ui.escAttr = (s) =>
    String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  GM.ui.toast = (msg, kind) => (window.__GM ? window.__GM.toast(msg, kind) : console.log(msg));
  GM.ui.confirm = confirmSheet;
  GM.ui.cssEsc = cssEsc;
  GM.app = {};
  GM.accounts = { render: renderAccountsEx, active: activeAccount, skinCache, paintAvatar, drawIsoHead, skinFor };
  GM.skinlib = { open: skinlibSheet };
  GM.settings = { addServer: addServerSheet };

  // ---------------------------------------------------------------- 钩子

  HOOKS.ready.push(() => {
    invoke = window.__GM.invoke;
    listenFn = window.__GM.listen;
    S = window.__GM.S;
    Object.assign(GM.app, window.__GM);
    bindAccountEvents();

    document.getElementById('btn-add-offline').onclick = offlineSheet;
    document.getElementById('btn-add-ms').onclick = msSheet;
    document.getElementById('btn-add-ygg').onclick = yggSheet;
    document.getElementById('btn-skinlib').onclick = () => skinlibSheet(1);

    if (listenFn) {
      listenFn('progress', (ev) => {
        const p = ev.payload || {};
        ui.updateProgress({ label: p.label, frac: p.frac });
      });
    }
  });

  HOOKS.render.push(() => {
    if (!S) return;
    renderAccountsEx();
  });

  HOOKS.nav.push((page) => {
    if (page === 'accounts' && S) renderAccountsEx();
  });
})();
