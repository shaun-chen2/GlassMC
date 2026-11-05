/* =============================================================================
   GlassMC · Tauri 前端
   -----------------------------------------------------------------------------
   直接跑在 WebView 里，没有构建步骤：一个文件搞定路由、渲染与事件。
   没有 __TAURI__ 时自动切到 mock（用于浏览器里单独看界面）。
   ========================================================================== */
(function () {
  'use strict';

  // ---------------------------------------------------------------- 运行时适配
  const TAURI = window.__TAURI__;
  const IS_TAURI = !!TAURI;

  // ---------------------------------------------------------------- mock
  const MOCK_STATE = {
    version: '1.3.2',
    dataRoot: '/Users/demo/Library/Application Support/GlassMC',
    gameDir: '/Users/demo/Library/Application Support/GlassMC/minecraft',
    os: 'osx',
    arch: 'arm64',
    platform: 'macOS',
    glassHint: 'liquid',
    config: {
      glassOpacity: 1,
      theme: 'dark',
      downloadBase: 'https://bmclapi2.bangbang93.com',
      javaMirror: 'https://mirrors.tuna.tsinghua.edu.cn/Adoptium',
      memoryGb: 12,
      windowWidth: 0,
      windowHeight: 0,
      extraJvmArgs: '',
      lastVersion: '新蒸程1.7',
      javaPath: '',
      activeAccount: 'a1',
      offlineName: 'Steve',
      quitOnClose: true,
      authServers: [
        { id: 'srv-littleskin', name: 'LittleSkin', baseUrl: 'https://littleskin.cn/api/yggdrasil', isPreset: true },
        { id: 'srv-miniskins', name: 'MiniSkins', baseUrl: 'https://www.mcpeau.com/api/yggdrasil', isPreset: true },
        { id: 'srv-ely-by', name: 'Ely.by', baseUrl: 'https://authserver.ely.by', isPreset: true }
      ],
      skinLibraryBase: 'https://www.mcpeau.com',
      msClientId: '',
      terracottaVersion: ''
    },
    accounts: [
      { id: 'a1', type: 'offline', username: 'Steve', uuid: 'b50ad385-829d-3141-a216-7e7d7539ba7f', source: 'glassmc', needsReauth: false }
    ],
    activeAccount: { id: 'a1', type: 'offline', username: 'Steve', source: 'glassmc', needsReauth: false },
    installed: [
      { id: '新蒸程1.7', kind: 'release', loader: 'neoforge', inherits: '1.21.1-NeoForge', mods: 283, isolated: true, gameDir: '/Users/demo/Library/Application Support/GlassMC/minecraft/versions/新蒸程1.7', jarOk: true, missingLibs: 0, hasModsDir: true },
      { id: '1.21.1-NeoForge', kind: 'release', loader: 'neoforge', inherits: '1.21.1', mods: 43, isolated: false, gameDir: '/Users/demo/Library/Application Support/GlassMC/minecraft', jarOk: true, missingLibs: 0, hasModsDir: true },
      { id: '1.20.1', kind: 'release', loader: null, inherits: null, mods: 0, isolated: false, gameDir: '/Users/demo/Library/Application Support/GlassMC/minecraft', jarOk: true, missingLibs: 0, hasModsDir: true }
    ],
    java: [
      { path: '/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home/bin/java', home: '/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home', version: 21, vendor: 'Eclipse Adoptium', arch: 'aarch64', source: 'system', label: 'Eclipse Adoptium 21 · aarch64' },
      { path: '/Library/Java/JavaVirtualMachines/zulu-8.jdk/Contents/Home/bin/java', home: '/Library/Java/JavaVirtualMachines/zulu-8.jdk/Contents/Home', version: 8, vendor: 'Azul Systems', arch: 'aarch64', source: 'system', label: 'Azul Systems 8 · aarch64' }
    ],
    running: false
  };

  const mockBus = new Map();
  const mockEmit = (name, payload) => (mockBus.get(name) || []).forEach((cb) => cb({ payload }));
  const mockListen = async (name, cb) => {
    if (!mockBus.has(name)) mockBus.set(name, []);
    mockBus.get(name).push(cb);
    return () => {};
  };

  const mockInvoke = async (cmd, args = {}) => {
    switch (cmd) {
      case 'bootstrap':
        return JSON.parse(JSON.stringify(MOCK_STATE));
      case 'save_config': {
        Object.assign(MOCK_STATE.config, args.patch || {});
        return { ...MOCK_STATE.config };
      }
      case 'scan_versions':
        return MOCK_STATE.installed;
      case 'list_java':
        return MOCK_STATE.java;
      case 'list_mods':
        return Array.from({ length: 46 }, (_, i) => ({
          name: `mod_${i}.jar`,
          label: ['create_aeronautics', 'create_deep_sea', 'jei', 'jade', 'patchouli'][i % 5] + (i > 4 ? '_' + i : ''),
          size: 120000 + i * 3700,
          enabled: i % 17 !== 0,
          mtime: 1762000000 - i * 999
        }));
      case 'list_remote':
        return [
          '1.21.4', '1.21.3', '1.21.1', '1.21', '1.20.6', '1.20.4', '1.20.1', '1.20', '1.19.4', '1.19.2',
          '1.18.2', '1.17.1', '1.16.5', '1.15.2', '1.14.4', '1.12.2', '1.8.9', '1.7.10'
        ].map((id) => ({ id, kind: 'release', url: '' }));
      case 'preview_command':
        return {
          program: '/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home/bin/java',
          args: ['-Xmx12288M', '-XstartOnFirstThread', '-Djava.library.path=/natives', '-cp', '…（共 214 项）', 'net.minecraft.client.main.Main', '--username', 'Steve'],
          cwd: MOCK_STATE.gameDir,
          versionId: args.versionId,
          javaVersion: 21,
          maxMemoryMb: 12288,
          classpathCount: 214,
          missingFiles: 0
        };
      case 'launch_game': {
        MOCK_STATE.running = true;
        const lines = [
          '[main/INFO] 正在启动 Minecraft 1.21.1',
          '[main/INFO] Backend library: LWJGL',
          '[main/INFO] OpenAL initialized',
          '[Render thread/INFO] OpenGL: Apple M3 Pro',
          '[Render thread/INFO] Game took 12.4 seconds to start'
        ];
        lines.forEach((l, i) => setTimeout(() => mockEmit('game-log', { stream: 'out', line: l }), 240 * (i + 1)));
        return 4242;
      }
      case 'kill_game':
        MOCK_STATE.running = false;
        return null;
      case 'game_running':
        return MOCK_STATE.running;
      case 'open_path':
      case 'open_url':
      case 'window_action':
      case 'set_opacity':
        return null;
      case 'account_list':
        return MOCK_STATE.accounts;
      case 'set_active_account':
        MOCK_STATE.config.activeAccount = args.id;
        return { ...MOCK_STATE.config };
      case 'create_offline_account': {
        const acc = { id: 'a' + (MOCK_STATE.accounts.length + 1), type: 'offline', username: args.name, source: 'glassmc', needsReauth: false };
        MOCK_STATE.accounts.push(acc);
        MOCK_STATE.config.activeAccount = acc.id;
        return MOCK_STATE.accounts;
      }
      // ---- 第二阶段命令的浏览器模拟 ----
      case 'accounts_all':
        return MOCK_STATE.accounts;
      case 'get_config':
        return { ...MOCK_STATE.config };
      case 'list_java_all':
        return MOCK_STATE.java;
      case 'java_candidates':
        return [8, 11, 17, 21, 25].map((f) => ({ feature: f, label: 'Java ' + f, installed: f === 21 || f === 17 }));
      case 'mods_scan':
        return Array.from({ length: 28 }, (_, i) => {
          const names = ['Create', 'JEI', 'Jade', 'Patchouli', 'Flywheel', 'Farmer\'s Delight', 'Sophisticated Backpacks'];
          const nm = names[i % names.length];
          return {
            fileName: nm.toLowerCase().replace(/[^a-z]+/g, '-') + '-' + (i + 1) + '.jar',
            path: MOCK_STATE.gameDir + '/mods/' + i + '.jar',
            modId: nm.toLowerCase().replace(/[^a-z]/g, ''),
            name: nm,
            version: '1.' + (i % 9) + '.' + i,
            size: 180000 + i * 42000,
            enabled: i % 13 !== 0
          };
        });
      case 'mods_dir':
        return MOCK_STATE.gameDir + '/mods';
      case 'modrinth_search': {
        const all = [
          ['sodium', '现代渲染优化，大幅提升帧率', 'jellysquid3', 41200000],
          ['iris', '光影加载器，支持 OptiFine 光影包', 'coderbot', 18700000],
          ['jei', '查看物品配方与用途', 'mezz', 33100000],
          ['create', '机械与自动化', 'simibubi', 15200000],
          ['jade', '方块信息 HUD', 'snownee', 9800000]
        ];
        const start = args.offset || 0;
        const hits = all.slice(start, start + 3).map((x) => ({
          projectId: x[0], slug: x[0], title: x[0].toUpperCase(), description: x[1], author: x[2], iconUrl: '', downloads: x[3]
        }));
        return [hits, all.length];
      }
      case 'modrinth_versions':
        return [];
      case 'skinlib_page':
        return {
          items: Array.from({ length: 12 }, (_, i) => ({ tid: i + 1, name: '皮肤 ' + (i + 1), kind: i % 3 === 0 ? 'alex' : 'steve', likes: 100 - i, author: 'user' + i })),
          page: args.page || 1, lastPage: 3, total: 36
        };
      case 'skinlib_thumb':
      case 'skinlib_apply':
      case 'account_skin':
      case 'refresh_account_skin':
        return null;
      case 'injector_status':
        return { installed: true, version: '1.2.2', path: MOCK_STATE.gameDir + '/../authlib-injector.jar' };
      case 'injector_install':
        return { installed: true, version: '1.2.2', path: MOCK_STATE.gameDir + '/../authlib-injector.jar' };
      case 'version_detail':
        return { id: args.versionId, mainClass: 'net.minecraft.client.main.Main', jarId: args.versionId, libraries: 214, missingLibraries: 0, jarOk: true, assets: '17', javaMajor: 21, inheritedFrom: null };
      case 'loader_versions':
        return args.kind === 'neoforge'
          ? ['21.1.236', '21.1.219', '21.1.209', '21.1.190', '21.1.186']
          : ['0.17.2', '0.16.14', '0.16.10', '0.16.9', '0.15.11'];
      case 'import_detect':
        return [
          { id: 'hmcl|/Applications/HMCL.app/Contents/Resources/.minecraft', launcher: 'hmcl', name: 'HMCL', rootPath: '/Applications/HMCL.app/Contents/Resources/.minecraft', gameDir: '/Applications/HMCL.app/Contents/Resources/.minecraft', accountCount: 1, versionCount: 4 },
          { id: 'official|~/Library/Application Support/minecraft', launcher: 'official', name: '官方启动器', rootPath: '~/Library/Application Support/minecraft', gameDir: '~/Library/Application Support/minecraft', accountCount: 0, versionCount: 2 }
        ];
      case 'modpack_list':
        return [];
      case 'tc_snapshot':
        return { ready: false, preparing: false, prepareText: '', error: null, port: null, version: '0.4.2', easytier: '', state: null, binaryInstalled: false };
      case 'tc_bootstrap':
        return { ready: true, preparing: false, prepareText: '', error: null, port: 49221, version: '0.4.2', easytier: 'v2.5.0', state: { state: 'waiting', index: 0 }, binaryInstalled: true };
      default:
        return null;
    }
  };

  const invoke = IS_TAURI ? TAURI.core.invoke : mockInvoke;
  const listen = IS_TAURI ? TAURI.event.listen : mockListen;

  // ---------------------------------------------------------------- 状态
  const S = {
    boot: null,
    config: null,
    installed: [],
    remote: [],
    mods: [],
    page: 'home',
    selected: localStorage.getItem('glassmc.selected') || '',
    running: false,
    remoteLoading: false,
    logOpen: false
  };

  // ---------------------------------------------------------------- 工具
  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

  const esc = (s) =>
    String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  function fmtSize(bytes) {
    if (!bytes) return '0 B';
    const u = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    let v = bytes;
    while (v >= 1024 && i < u.length - 1) {
      v /= 1024;
      i++;
    }
    return (v >= 100 ? v.toFixed(0) : v.toFixed(1)) + ' ' + u[i];
  }

  function toast(msg, kind = '') {
    const box = $('#toasts');
    const t = document.createElement('div');
    t.className = 'toast ' + kind;
    t.textContent = msg;
    box.appendChild(t);
    setTimeout(() => {
      t.style.transition = 'opacity .25s ease, transform .25s ease';
      t.style.opacity = '0';
      t.style.transform = 'translateY(6px)';
      setTimeout(() => t.remove(), 260);
    }, kind === 'err' ? 5200 : 2800);
  }

  const loaderName = { forge: 'Forge', neoforge: 'NeoForge', fabric: 'Fabric', quilt: 'Quilt' };

  function accountInitial(name) {
    return (name || '?').trim().charAt(0).toUpperCase() || '?';
  }

  // ---------------------------------------------------------------- 主题
  function prefersDark() {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }

  function applyTheme() {
    const t = (S.config && S.config.theme) || 'system';
    const dark = t === 'dark' || (t === 'system' && prefersDark());
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    syncToggleScheme();
    const sel = $('#set-theme');
    if (sel) sel.value = t;
  }

  function syncToggleScheme() {
    const scheme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
    $$('liquid-glass-toggle').forEach((el) => el.setAttribute('scheme', scheme));
    const tierEl = $('#toggle-tier');
    const demo = $('#demo-toggle');
    if (tierEl) tierEl.textContent = (demo && demo.dataset.glass) || '检测中…';
  }

  function applyAlpha() {
    const a = S.config ? S.config.glassOpacity : 1;
    document.documentElement.style.setProperty('--glass-alpha', String(a));
    const out = $('#alpha-val');
    if (out) out.textContent = Math.round(a * 100) + '%';
  }

  // ---------------------------------------------------------------- 渲染：通用
  function renderChrome() {
    const b = S.boot;
    $('#tb-ver').textContent = 'v' + b.version;
    $('#about-ver').textContent = 'v' + b.version;
    $('#env-platform').textContent = b.platform + ' · ' + b.arch;
    $('#env-datadir').textContent = b.dataRoot;
    $('#env-datadir').title = b.dataRoot;
    $('#env-source').textContent = S.config.downloadBase;
    $('#env-source').title = S.config.downloadBase;
    $('#env-glass').textContent = b.glassHint === 'liquid' ? '液态玻璃' : '磨砂';
    const j = b.java && b.java[0];
    $('#env-java').textContent = j ? 'Java ' + j.version + ' · ' + j.vendor : '未检测到';
    $('#nav-badge-versions').textContent = String(b.installed.length);
    $('#stat-versions').textContent = String(b.installed.length);
    $('#stat-java').textContent = String((b.java || []).length);
    $('#stat-mem').textContent = (S.config.memoryGb || 0).toFixed(0) + 'G';
    renderAccountChip();
  }

  function renderAccountChip() {
    const a = S.boot.activeAccount || {};
    const av = $('#account-avatar');
    if (a.skinLocalPath) {
      av.innerHTML = '<img src="' + esc(a.skinLocalPath) + '" alt="">';
    } else {
      av.textContent = accountInitial(a.username);
    }
    $('#account-name').textContent = a.username || '未登录';
    const map = { offline: '离线账号', msa: '微软正版', microsoft: '微软正版', yggdrasil: '外置登录', mojang: 'Mojang' };
    $('#account-type').textContent = map[a.type] || a.type || '—';
  }

  // ---------------------------------------------------------------- 渲染：首页
  function selectedVersion() {
    const list = S.installed;
    if (!list.length) return null;
    return (
      list.find((v) => v.id === S.selected) ||
      list.find((v) => v.id === S.config.lastVersion) ||
      list[0]
    );
  }

  function versionBadges(v) {
    const out = [];
    if (v.loader) out.push('<span class="badge-pill accent">' + esc(loaderName[v.loader] || v.loader) + '</span>');
    if (v.isolated) out.push('<span class="badge-pill">版本隔离</span>');
    if (v.mods > 0) out.push('<span class="badge-pill">' + v.mods + ' 模组</span>');
    if (!v.jarOk) out.push('<span class="badge-pill danger"><span class="d"></span>缺客户端 jar</span>');
    if (v.missingLibs > 0) out.push('<span class="badge-pill warn"><span class="d"></span>缺 ' + v.missingLibs + ' 个库</span>');
    if (v.jarOk && v.missingLibs === 0) out.push('<span class="badge-pill ok"><span class="d"></span>就绪</span>');
    return out.join('');
  }

  function renderHome() {
    const v = selectedVersion();
    const heroTitle = $('#hero-title');
    const heroVer = $('#hero-version');
    const heroMeta = $('#hero-meta');

    if (!v) {
      heroTitle.textContent = '还没有装好的版本';
      heroVer.textContent = '先去「安装」页装一个，或者把已有启动器的数据导入';
      heroMeta.innerHTML = '';
      $('#btn-play').disabled = true;
    } else {
      S.selected = v.id;
      heroTitle.textContent = v.loader ? (loaderName[v.loader] || v.loader) + ' 整合实例' : '原版游戏';
      heroVer.textContent = v.id + (v.inherits ? '  ·  继承 ' + v.inherits : '');
      heroMeta.innerHTML = versionBadges(v);
      $('#btn-play').disabled = false;
    }

    const totalMods = S.installed.reduce((a, x) => a + (x.mods || 0), 0);
    $('#stat-mods').textContent = String(totalMods);

    const recent = S.installed.slice(0, 4);
    const box = $('#home-recent');
    box.innerHTML = recent.length
      ? recent
          .map(
            (x) => `
        <div class="vrow" data-pick="${esc(x.id)}" style="cursor:default">
          <div class="v-main">
            <div class="v-id">${esc(x.id)}</div>
            <div class="v-meta">${versionBadges(x)}</div>
          </div>
          <div class="v-actions">
            <button class="btn sm ${x.id === S.selected ? 'primary' : ''}" data-play="${esc(x.id)}">启动</button>
          </div>
        </div>`
          )
          .join('')
      : '<div class="empty"><span class="big-ico">◌</span><span>暂无已安装版本</span></div>';
    $('#home-count').textContent = S.installed.length + ' 个';
  }

  // ---------------------------------------------------------------- 渲染：版本
  function renderVersions() {
    const box = $('#versions-list');
    $('#versions-sub').textContent = S.installed.length + ' 个版本 · ' + (S.boot.dataRoot || '');
    if (!S.installed.length) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>versions 目录里没有可用版本</span></div>';
      return;
    }
    box.innerHTML = S.installed
      .map(
        (v) => `
      <div class="vrow" data-pick="${esc(v.id)}">
        <div class="v-main">
          <div class="v-id">${esc(v.id)}</div>
          <div class="v-meta">${versionBadges(v)}</div>
        </div>
        <div class="v-actions">
          <button class="btn sm" data-cmd="${esc(v.id)}">启动参数</button>
          <button class="btn sm primary" data-play="${esc(v.id)}">启动</button>
        </div>
      </div>`
      )
      .join('');
  }

  // ---------------------------------------------------------------- 渲染：安装
  function renderRemote() {
    const box = $('#remote-list');
    const kw = ($('#remote-filter').value || '').trim().toLowerCase();
    const installed = new Set(S.installed.map((v) => v.id));
    const list = S.remote.filter((v) => !kw || v.id.toLowerCase().includes(kw));

    if (!list.length) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>没有匹配的版本</span></div>';
      return;
    }
    box.innerHTML = list
      .slice(0, 200)
      .map((v) => {
        const has = installed.has(v.id);
        // 后端新版序列化成 kind，旧版误 rename 成了 type，两种都认；
        // 都取不到就整块不渲染，免得留一个空胶囊光剩圆点。
        const kind = v.kind || v.type || '';
        return `
      <div class="vrow">
        <div class="v-main">
          <div class="v-id">${esc(v.id)}</div>
          <div class="v-meta">
            ${kind ? `<span class="badge-pill">${esc(kind)}</span>` : ''}
            ${has ? '<span class="badge-pill ok"><span class="d"></span>已安装</span>' : ''}
          </div>
        </div>
        <div class="v-actions">
          <button class="btn sm" ${has ? 'disabled' : ''} data-download="${esc(v.id)}">${has ? '已装' : '下载'}</button>
        </div>
      </div>`;
      })
      .join('');
  }

  // ---------------------------------------------------------------- 渲染：模组
  function renderMods() {
    const v = selectedVersion();
    const box = $('#mods-list');
    if (!v) {
      $('#mods-sub').textContent = '—';
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>先选一个版本</span></div>';
      return;
    }
    $('#mods-sub').textContent = v.id + ' · ' + S.mods.length + ' 个文件' + (v.isolated ? ' · 实例目录' : ' · 全局 mods');
    $('#nav-badge-mods').textContent = String(S.mods.length);

    const kw = ($('#mods-filter').value || '').trim().toLowerCase();
    const list = S.mods.filter((m) => !kw || m.name.toLowerCase().includes(kw));
    if (!list.length) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>mods 目录是空的</span></div>';
      return;
    }
    box.innerHTML = list
      .map(
        (m) => `
      <div class="mrow">
        <span class="nm" title="${esc(m.name)}">${esc(m.label)}</span>
        <span class="sz">${fmtSize(m.size)}</span>
        <span class="st">${m.enabled ? '<span class="badge-pill ok"><span class="d"></span></span>' : '<span class="badge-pill warn"><span class="d"></span></span>'}</span>
      </div>`
      )
      .join('');
  }

  // ---------------------------------------------------------------- 渲染：账号
  function renderAccounts() {
    const box = $('#accounts-list');
    const list = S.boot.accounts || [];
    if (!list.length) {
      box.innerHTML = '<div class="empty"><span class="big-ico">◌</span><span>还没有账号，新建一个离线账号就能进游戏</span></div>';
      return;
    }
    const activeId = S.config.activeAccount;
    const map = { offline: '离线账号', msa: '微软正版', microsoft: '微软正版', yggdrasil: '外置登录', mojang: 'Mojang' };
    box.innerHTML = list
      .map(
        (a) => `
      <div class="vrow">
        <div class="avatar">${esc(accountInitial(a.username))}</div>
        <div class="v-main">
          <div class="v-id" style="font-size:13px">${esc(a.username)}</div>
          <div class="v-meta">
            <span class="badge-pill ${a.type === 'offline' ? '' : 'accent'}">${esc(map[a.type] || a.type)}</span>
            ${a.needsReauth ? '<span class="badge-pill warn"><span class="d"></span>需要重新登录</span>' : ''}
            ${a.id === activeId ? '<span class="badge-pill ok"><span class="d"></span>使用中</span>' : ''}
          </div>
        </div>
        <div class="v-actions">
          ${a.id === activeId ? '' : '<button class="btn sm" data-use-account="' + esc(a.id) + '">使用</button>'}
        </div>
      </div>`
      )
      .join('');
  }

  // ---------------------------------------------------------------- 渲染：设置
  function renderSettings() {
    const c = S.config;
    $('#set-memory').value = c.memoryGb;
    $('#mem-val').textContent = Number(c.memoryGb).toFixed(1) + ' GB';
    $('#mem-hint').textContent = 'Xmx ' + Math.round(c.memoryGb * 1024) + 'M';
    $('#set-win-w').value = c.windowWidth || '';
    $('#set-win-h').value = c.windowHeight || '';
    $('#set-jvm').value = c.extraJvmArgs || '';
    $('#set-alpha').value = c.glassOpacity;
    $('#alpha-val').textContent = Math.round(c.glassOpacity * 100) + '%';
    $('#set-source').value = c.downloadBase;
    $('#set-java-mirror').value = c.javaMirror;
    $('#set-theme').value = c.theme;

    const quit = $('#set-quit');
    if (quit) quit.checked = !!c.quitOnClose;

    const list = S.boot.java || [];
    $('#java-count').textContent = list.length + ' 个';
    $('#java-list').innerHTML = list.length
      ? list
          .map(
            (j, i) => `
        <div class="vrow">
          <div class="v-main">
            <div class="v-id" style="font-size:12.5px">Java ${j.version} · ${esc(j.vendor)}</div>
            <div class="v-meta">
              <span class="mono-dim">${esc(j.path)}</span>
            </div>
          </div>
          <div class="v-actions">
            ${i === 0 ? '<span class="badge-pill ok"><span class="d"></span>默认</span>' : ''}
          </div>
        </div>`
          )
          .join('')
      : '<div class="empty"><span class="big-ico">◌</span><span>没有检测到 Java，可指定路径或安装一个 JDK</span></div>';
  }

  // ---------------------------------------------------------------- 渲染：全部
  function renderAll() {
    renderChrome();
    renderHome();
    renderVersions();
    renderRemote();
    renderSettings();
    syncToggleScheme();
    // 账号页与模组页由 features.js 接管（带皮肤头像、Modrinth 等）
    if (window.__GM_ON_RENDER) window.__GM_ON_RENDER();
  }

  // ---------------------------------------------------------------- 页面切换
  function go(page) {
    S.page = page;
    $$('.nav-item').forEach((b) => b.classList.toggle('active', b.dataset.nav === page));
    $$('.page').forEach((p) => p.classList.toggle('on', p.dataset.page === page));
    $('#content').scrollTop = 0;
    if (page === 'install' && !S.remote.length && !S.remoteLoading) loadRemote();
    if (window.__GM_ON_NAV) window.__GM_ON_NAV(page);
  }

  // ---------------------------------------------------------------- 数据操作
  let saveTimer = null;
  function saveConfig(patch) {
    Object.assign(S.config, patch);
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        S.config = await invoke('save_config', { patch });
      } catch (e) {
        toast('保存失败：' + e, 'err');
      }
    }, 200);
  }

  async function loadMods() {
    const v = selectedVersion();
    if (!v) return;
    try {
      S.mods = await invoke('list_mods', { versionId: v.id });
    } catch (e) {
      S.mods = [];
    }
    renderMods();
  }

  async function refreshInstalled() {
    try {
      S.installed = await invoke('scan_versions');
      renderAll();
      toast('已重新扫描', 'ok');
    } catch (e) {
      toast('扫描失败：' + e, 'err');
    }
  }

  async function loadRemote() {
    if (S.remoteLoading) return;
    S.remoteLoading = true;
    const btn = $('#btn-load-remote');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner"></span> 拉取中';
    try {
      S.remote = await invoke('list_remote', { includeSnapshots: $('#chk-snapshots').checked });
      renderRemote();
      toast('已获取 ' + S.remote.length + ' 个版本', 'ok');
    } catch (e) {
      toast('拉取失败：' + e, 'err');
    } finally {
      S.remoteLoading = false;
      btn.disabled = false;
      btn.textContent = '刷新列表';
    }
  }

  async function play(id) {
    const v = S.installed.find((x) => x.id === id);
    if (!v) return;
    S.selected = id;
    localStorage.setItem('glassmc.selected', id);
    openLog(true);
    setRunning(true, '启动中…');
    try {
      const pid = await invoke('launch_game', { versionId: id });
      setRunning(true, 'PID ' + pid);
      toast('已启动 ' + id, 'ok');
    } catch (e) {
      setRunning(false);
      appendLog('err', '启动失败：' + String(e));
      toast(String(e).split('\n')[0], 'err');
    }
  }

  async function showCommand(id) {
    try {
      const c = await invoke('preview_command', { versionId: id });
      openLog(true);
      $('#log-body').innerHTML = '';
      appendLog('out', '# ' + c.cwd);
      appendLog('out', c.program + ' \\');
      c.args.forEach((a, i) => {
        appendLog('out', '  ' + a + (i === c.args.length - 1 ? '' : ' \\'));
      });
      $('#log-state').textContent = 'classpath ' + c.classpathCount + ' 项 · 缺失 ' + c.missingFiles + ' · Java ' + c.javaVersion;
    } catch (e) {
      toast(String(e).split('\n')[0], 'err');
    }
  }

  // ---------------------------------------------------------------- 日志
  function appendLog(stream, line) {
    const body = $('#log-body');
    const d = document.createElement('div');
    if (stream === 'err') d.className = 'e';
    d.textContent = line;
    body.appendChild(d);
    while (body.childElementCount > 2000) body.removeChild(body.firstChild);
    body.scrollTop = body.scrollHeight;
  }

  function openLog(on) {
    S.logOpen = on;
    $('#logbar').classList.toggle('on', on);
  }

  function setRunning(on, text) {
    S.running = on;
    const chip = $('#tb-status');
    chip.classList.toggle('live', on);
    $('#tb-status-text').textContent = on ? text || '运行中' : '空闲';
    $('#log-state').textContent = on ? '运行中' : '未启动';
    const btn = $('#btn-play');
    if (btn) {
      btn.disabled = on;
      btn.classList.toggle('primary', !on);
      btn.innerHTML = on
        ? '<span class="spinner"></span> 游戏运行中'
        : '<svg viewBox="0 0 16 16" fill="currentColor"><path d="M4.4 2.6l8.2 5.4-8.2 5.4z"/></svg> 启动游戏';
    }
  }

  // ---------------------------------------------------------------- 事件
  function bind() {
    // 窗口按钮
    $('#btn-min').onclick = () => invoke('window_action', { action: 'minimize' });
    $('#btn-max').onclick = () => invoke('window_action', { action: 'maximize' });
    $('#btn-close').onclick = () => {
      if (S.config.quitOnClose) invoke('window_action', { action: 'close' });
      else invoke('window_action', { action: 'minimize' });
    };

    // 侧栏
    $$('.nav-item').forEach((b) => (b.onclick = () => go(b.dataset.nav)));
    $('#account-chip').onclick = () => go('accounts');

    // 内容区委托
    $('#content').addEventListener('click', (e) => {
      const playBtn = e.target.closest('[data-play]');
      if (playBtn) {
        play(playBtn.dataset.play);
        return;
      }
      const cmdBtn = e.target.closest('[data-cmd]');
      if (cmdBtn) {
        showCommand(cmdBtn.dataset.cmd);
        return;
      }
      const pick = e.target.closest('[data-pick]');
      if (pick) {
        S.selected = pick.dataset.pick;
        localStorage.setItem('glassmc.selected', S.selected);
        renderAll();
        loadMods();
        return;
      }
      const dl = e.target.closest('[data-download]');
      if (dl && !dl.disabled) {
        if (window.__GM_INSTALL) window.__GM_INSTALL(dl.dataset.download, dl);
        else toast('安装模块未加载', 'err');
        return;
      }
      const useAcc = e.target.closest('[data-use-account]');
      if (useAcc) {
        invoke('set_active_account', { id: useAcc.dataset.useAccount })
          .then((cfg) => {
            S.config = cfg;
            S.boot.activeAccount = (S.boot.accounts || []).find((a) => a.id === cfg.activeAccount) || S.boot.activeAccount;
            renderChrome();
            renderAccounts();
            toast('已切换账号', 'ok');
          })
          .catch((err) => toast(String(err), 'err'));
      }
    });

    // 首页
    $('#btn-play').onclick = () => {
      const v = selectedVersion();
      if (v) play(v.id);
    };
    $('#btn-open-dir').onclick = () =>
      invoke('open_path', { path: (selectedVersion() || {}).gameDir || S.boot.gameDir }).catch((e) => toast(String(e), 'err'));
    $('#btn-toggle-log').onclick = () => openLog(!S.logOpen);
    $('#btn-collapse-log').onclick = () => openLog(false);
    $('#btn-clear-log').onclick = () => ($('#log-body').innerHTML = '');
    $('#btn-kill').onclick = () =>
      invoke('kill_game')
        .then(() => {
          setRunning(false);
          toast('已结束进程', 'ok');
        })
        .catch((e) => toast(String(e), 'err'));

    // 版本页
    $('#btn-refresh-versions').onclick = refreshInstalled;

    // 安装页
    $('#btn-load-remote').onclick = loadRemote;
    $('#chk-snapshots').onchange = () => {
      if (S.remote.length) loadRemote();
    };
    $('#remote-filter').oninput = renderRemote;

    // 模组页
    $('#mods-filter').oninput = renderMods;
    $('#btn-open-mods').onclick = () => {
      const v = selectedVersion();
      if (!v) return;
      invoke('open_path', { path: v.gameDir + '/mods' }).catch((e) => toast(String(e), 'err'));
    };

    // 账号页
    $('#btn-add-offline').onclick = () => {
      const name = prompt('离线账号名字（3-16 位）');
      if (!name) return;
      invoke('create_offline_account', { name })
        .then((list) => {
          S.boot.accounts = list;
          renderAccounts();
          renderChrome();
          toast('已创建离线账号 ' + name, 'ok');
        })
        .catch((e) => toast(String(e), 'err'));
    };

    // 设置页
    const mem = $('#set-memory');
    mem.oninput = () => {
      const v = Number(mem.value);
      $('#mem-val').textContent = v.toFixed(1) + ' GB';
      $('#mem-hint').textContent = 'Xmx ' + Math.round(v * 1024) + 'M';
      $('#stat-mem').textContent = v.toFixed(0) + 'G';
      saveConfig({ memoryGb: v });
    };

    const alpha = $('#set-alpha');
    alpha.oninput = () => {
      const v = Number(alpha.value);
      S.config.glassOpacity = v;
      applyAlpha();
      saveConfig({ glassOpacity: v });
    };

    $('#set-win-w').onchange = (e) => saveConfig({ windowWidth: Number(e.target.value) || 0 });
    $('#set-win-h').onchange = (e) => saveConfig({ windowHeight: Number(e.target.value) || 0 });
    $('#set-jvm').onchange = (e) => saveConfig({ extraJvmArgs: e.target.value });
    $('#set-source').onchange = (e) => saveConfig({ downloadBase: e.target.value.trim() });
    $('#set-java-mirror').onchange = (e) => saveConfig({ javaMirror: e.target.value.trim() });
    $('#set-theme').onchange = (e) => {
      saveConfig({ theme: e.target.value });
      applyTheme();
    };

    const quit = $('#set-quit');
    if (quit) {
      quit.addEventListener('change', (e) => saveConfig({ quitOnClose: !!e.detail.checked }));
    }

    // 演示开关
    $$('#demo-toggle, #demo-toggle2, #demo-toggle3').forEach((t) => {
      t.addEventListener('change', () => syncToggleScheme());
    });

    $('#link-repo').onclick = (e) => {
      e.preventDefault();
      invoke('open_url', { url: 'https://github.com/shaun-chen2/GlassMC' }).catch(() => {});
    };

    // 内容滚动 → 标题栏加分隔线
    $('#content').addEventListener('scroll', () => {
      $('#shell').classList.toggle('scrolled', $('#content').scrollTop > 4);
    });

    // 系统主题变化
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if ((S.config.theme || 'system') === 'system') applyTheme();
    });
  }

  // ---------------------------------------------------------------- 启动
  async function boot() {
    try {
      S.boot = await invoke('bootstrap');
    } catch (e) {
      toast('初始化失败：' + e, 'err');
      console.error(e);
      return;
    }
    S.config = S.boot.config;
    S.installed = S.boot.installed || [];
    S.running = !!S.boot.running;

    // 平台的观感差异（圆角半径等）让 CSS 自己判断，别在样式里猜 userAgent
    document.documentElement.dataset.os = String(S.boot.os || '').toLowerCase();

    applyTheme();
    applyAlpha();
    renderAll();
    bind();

    if (S.running) {
      setRunning(true, '运行中');
      openLog(true);
    }

    await listen('game-log', (ev) => appendLog(ev.payload.stream, ev.payload.line));
    await listen('game-exit', (ev) => {
      setRunning(false);
      appendLog('out', '—— 游戏已退出（code ' + ev.payload + '）——');
      toast('游戏已退出', '');
    });

    // 液态玻璃开关的档位是异步算出来的
    setTimeout(syncToggleScheme, 320);

    if (!IS_TAURI) {
      document.documentElement.classList.add('preview');
      document.title = 'GlassMC（浏览器预览）';
      console.info('[GlassMC] 当前是浏览器 mock 模式');
    }

    // 交给 features.js（安装 / 账号 / 模组 / 导入 / 联机）
    window.__GM = {
      invoke,
      listen,
      S,
      IS_TAURI,
      toast,
      esc,
      fmtSize,
      renderAll,
      renderChrome,
      renderSettings,
      renderVersions,
      renderRemote,
      renderHome,
      renderAccounts,
      selectedVersion,
      go,
      saveConfig,
      setRunning,
      accountInitial,
      loaderName,
      loadRemote,
      openLog,
      appendLog,
    };
    if (window.__GM_ON_READY) window.__GM_ON_READY();

    // features 模块的 ready 钩子里可能还有异步加载（皮肤库、联机状态等），
    // 它们注册得比首帧晚，这里再渲染一次，保证补丁式的按钮（目录/删除）一定在。
    setTimeout(renderAll, 60);
  }

  // 必须等其它脚本（features*.js）也解析完再启动：
  // 否则 boot 会在当前脚本结束后立刻走完微任务，那时 features 的钩子还没注册。
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else setTimeout(boot, 0);
})();
