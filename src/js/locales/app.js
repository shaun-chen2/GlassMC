//! app.js（外壳：标题栏、首页、版本、安装、模组、账号、设置、日志、toast）的动态文案。
//! 只放运行时拼接的字符串；静态节点上的文案在 locales/html.js 里用 data-i18n 管。
window.GM.i18n.add('zh-CN', {
  // ---- 顶栏 / 环境信息
  'app.chrome.detecting': '检测中…',
  'app.chrome.glass_liquid': '液态玻璃',
  'app.chrome.glass_frosted': '磨砂',
  'app.chrome.java_none': '未检测到',
  'app.chrome.preview_title': 'GlassMC（浏览器预览）',

  // ---- 账号
  'app.account.not_logged_in': '未登录',
  'app.account.type_offline': '离线账号',
  'app.account.type_msa': '微软正版',
  'app.account.type_yggdrasil': '外置登录',
  'app.account.empty': '还没有账号，新建一个离线账号就能进游戏',
  'app.account.reauth': '需要重新登录',
  'app.account.in_use': '使用中',
  'app.account.use': '使用',
  'app.account.prompt_offline_name': '离线账号名字（3-16 位）',

  // ---- 版本徽章
  'app.badge.isolated': '版本隔离',
  'app.badge.mods': '{n} 模组',
  'app.badge.no_jar': '缺客户端 jar',
  'app.badge.missing_libs': '缺 {n} 个库',
  'app.badge.ready': '就绪',

  // ---- 首页
  'app.home.empty_title': '还没有装好的版本',
  'app.home.empty_hint': '先去「安装」页装一个，或者把已有启动器的数据导入',
  'app.home.instance_title': '{loader} 整合实例',
  'app.home.vanilla': '原版游戏',
  'app.home.with_inherits': '{id}  ·  继承 {parent}',
  'app.home.play': '启动',
  'app.home.play_btn': '启动游戏',
  'app.home.empty_recent': '暂无已安装版本',
  'app.home.count': '{n} 个',

  // ---- 版本页
  'app.versions.sub': '{n} 个版本 · {path}',
  'app.versions.empty': 'versions 目录里没有可用版本',
  'app.versions.command': '启动参数',
  'app.versions.play': '启动',

  // ---- 安装页
  'app.install.installed': '已安装',
  'app.install.installed_short': '已装',
  'app.install.download': '下载',
  'app.install.empty': '点右上角刷新，拉取可安装的版本',
  'app.install.sub_idle': '镜像优先，失败自动回退官方源',
  'app.install.no_match': '没有匹配的版本',
  'app.install.sub_no_match': '匹配 0 个 · 共 {total} 个',
  'app.install.sub_filtered': '匹配 {matched} 个 · 已显示 {shown} · 共 {total} 个',
  'app.install.sub_all': '共 {total} 个版本 · 已显示 {shown}',
  'app.install.more': '显示更多（剩余 {n}）',
  'app.install.fetching': '拉取中',
  'app.install.refresh': '刷新列表',

  // ---- 模组页
  'app.mods.empty_no_version': '先选一个版本',
  'app.mods.empty': 'mods 目录是空的',
  'app.mods.sub': '{id} · {n} 个文件 · {scope}',
  'app.mods.scope_instance': '实例目录',
  'app.mods.scope_global': '全局 mods',

  // ---- 设置页
  'app.settings.java_count': '{n} 个',
  'app.settings.java_default': '默认',
  'app.settings.java_empty': '没有检测到 Java，可指定路径或安装一个 JDK',

  // ---- 更新
  'app.update.hint_found': '发现 v{version}',
  'app.update.hint_latest': '已是最新（v{version}）',
  'app.update.downloading': '下载 GlassMC v{version}',
  'app.update.download_done': '下载完成',

  // ---- 日志抽屉
  'app.log.starting': '启动中…',
  'app.log.running': '运行中',
  'app.log.idle': '空闲',
  'app.log.stopped': '未启动',
  'app.log.running_btn': '游戏运行中',
  'app.log.launch_failed': '启动失败：{msg}',
  'app.log.command_state': 'classpath {count} 项 · 缺失 {missing} · Java {java}',
  'app.log.game_exit': '—— 游戏已退出（code {code}）——',

  // ---- toast
  'app.toast.save_failed': '保存失败：{msg}',
  'app.toast.scan_failed': '扫描失败：{msg}',
  'app.toast.rescanned': '已重新扫描',
  'app.toast.remote_failed': '拉取失败：{msg}',
  'app.toast.remote_loaded': '已获取 {total} 个版本（正式 {release} · 其它 {other}）',
  'app.toast.launched': '已启动 {id}',
  'app.toast.install_not_loaded': '安装模块未加载',
  'app.toast.account_switched': '已切换账号',
  'app.toast.account_created': '已创建离线账号 {name}',
  'app.toast.killed': '已结束进程',
  'app.toast.game_exit': '游戏已退出',
  'app.toast.update_found': '发现新版本 v{version}',
  'app.toast.update_latest': '已经是最新版本',
  'app.toast.update_failed': '检查更新失败：{msg}',
  'app.toast.update_skipped': '已忽略 v{version}',
  'app.toast.saved_to': '已保存到 {path}',
  'app.toast.boot_failed': '初始化失败：{msg}',

  // ---- mock 数据里的说明性文字（仅浏览器预览模式可见）
  'app.mock.mod_sodium': '现代渲染优化，大幅提升帧率',
  'app.mock.mod_iris': '光影加载器，支持 OptiFine 光影包',
  'app.mock.mod_jei': '查看物品配方与用途',
  'app.mock.mod_create': '机械与自动化',
  'app.mock.mod_jade': '方块信息 HUD',
  'app.mock.skin_name': '皮肤 {n}',
  'app.mock.official_launcher': '官方启动器',
  'app.mock.cp_omitted': '…（共 {n} 项）',
});

window.GM.i18n.add('en-US', {
  // ---- 顶栏 / 环境信息
  'app.chrome.detecting': 'Detecting…',
  'app.chrome.glass_liquid': 'Liquid glass',
  'app.chrome.glass_frosted': 'Frosted',
  'app.chrome.java_none': 'Not detected',
  'app.chrome.preview_title': 'GlassMC (browser preview)',

  // ---- 账号
  'app.account.not_logged_in': 'Not signed in',
  'app.account.type_offline': 'Offline',
  'app.account.type_msa': 'Microsoft',
  'app.account.type_yggdrasil': 'External auth',
  'app.account.empty': 'No accounts yet — create an offline account to start playing',
  'app.account.reauth': 'Sign-in required',
  'app.account.in_use': 'In use',
  'app.account.use': 'Use',
  'app.account.prompt_offline_name': 'Offline account name (3-16 characters)',

  // ---- 版本徽章
  'app.badge.isolated': 'Isolated',
  'app.badge.mods': '{n} mods',
  'app.badge.no_jar': 'Client jar missing',
  'app.badge.missing_libs': '{n} libs missing',
  'app.badge.ready': 'Ready',

  // ---- 首页
  'app.home.empty_title': 'No installed version yet',
  'app.home.empty_hint': 'Install one from the Install page, or import an existing launcher',
  'app.home.instance_title': '{loader} modpack instance',
  'app.home.vanilla': 'Vanilla',
  'app.home.with_inherits': '{id}  ·  inherits {parent}',
  'app.home.play': 'Launch',
  'app.home.play_btn': 'Launch game',
  'app.home.empty_recent': 'No installed versions',
  'app.home.count': '{n}',

  // ---- 版本页
  'app.versions.sub': '{n} versions · {path}',
  'app.versions.empty': 'No usable versions in the versions folder',
  'app.versions.command': 'Command',
  'app.versions.play': 'Launch',

  // ---- 安装页
  'app.install.installed': 'Installed',
  'app.install.installed_short': 'Installed',
  'app.install.download': 'Download',
  'app.install.empty': 'Refresh in the top right to fetch installable versions',
  'app.install.sub_idle': 'Mirror first, falls back to the official source',
  'app.install.no_match': 'No matching versions',
  'app.install.sub_no_match': '0 matches · {total} total',
  'app.install.sub_filtered': '{matched} matches · {shown} shown · {total} total',
  'app.install.sub_all': '{total} versions · {shown} shown',
  'app.install.more': 'Show more ({n} left)',
  'app.install.fetching': 'Fetching',
  'app.install.refresh': 'Refresh list',

  // ---- 模组页
  'app.mods.empty_no_version': 'Pick a version first',
  'app.mods.empty': 'The mods folder is empty',
  'app.mods.sub': '{id} · {n} files · {scope}',
  'app.mods.scope_instance': 'instance dir',
  'app.mods.scope_global': 'global mods',

  // ---- 设置页
  'app.settings.java_count': '{n}',
  'app.settings.java_default': 'Default',
  'app.settings.java_empty': 'No Java found — set a path or install a JDK',

  // ---- 更新
  'app.update.hint_found': 'v{version} available',
  'app.update.hint_latest': 'Up to date (v{version})',
  'app.update.downloading': 'Downloading GlassMC v{version}',
  'app.update.download_done': 'Download complete',

  // ---- 日志抽屉
  'app.log.starting': 'Starting…',
  'app.log.running': 'Running',
  'app.log.idle': 'Idle',
  'app.log.stopped': 'Not running',
  'app.log.running_btn': 'Game running',
  'app.log.launch_failed': 'Launch failed: {msg}',
  'app.log.command_state': 'classpath {count} entries · {missing} missing · Java {java}',
  'app.log.game_exit': '—— Game exited (code {code}) ——',

  // ---- toast
  'app.toast.save_failed': 'Save failed: {msg}',
  'app.toast.scan_failed': 'Scan failed: {msg}',
  'app.toast.rescanned': 'Rescanned',
  'app.toast.remote_failed': 'Fetch failed: {msg}',
  'app.toast.remote_loaded': 'Fetched {total} versions ({release} release · {other} other)',
  'app.toast.launched': 'Launched {id}',
  'app.toast.install_not_loaded': 'Install module not loaded',
  'app.toast.account_switched': 'Account switched',
  'app.toast.account_created': 'Offline account {name} created',
  'app.toast.killed': 'Process stopped',
  'app.toast.game_exit': 'Game exited',
  'app.toast.update_found': 'New version v{version}',
  'app.toast.update_latest': 'Already up to date',
  'app.toast.update_failed': 'Update check failed: {msg}',
  'app.toast.update_skipped': 'Skipped v{version}',
  'app.toast.saved_to': 'Saved to {path}',
  'app.toast.boot_failed': 'Initialization failed: {msg}',

  // ---- mock 数据里的说明性文字（仅浏览器预览模式可见）
  'app.mock.mod_sodium': 'Modern rendering optimization with a big FPS boost',
  'app.mock.mod_iris': 'Shader loader with OptiFine shaderpack support',
  'app.mock.mod_jei': 'Browse item recipes and uses',
  'app.mock.mod_create': 'Machines and automation',
  'app.mock.mod_jade': 'Block info HUD',
  'app.mock.skin_name': 'Skin {n}',
  'app.mock.official_launcher': 'Official launcher',
  'app.mock.cp_omitted': '…({n} entries)',
});
