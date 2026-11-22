// 安全中心文案：恶意模组扫描 + Minecraft 新闻。
// features-security.js 通过 GM.t('security.' + key) 取用，侧栏标题用 nav.security。
window.GM.i18n.add('zh-CN', {
  'nav.security': '安全',

  'security.title': '安全中心',
  'security.sub': '恶意模组启发式扫描 · Minecraft 官方新闻',
  'security.disclaimer':
    '启发式提示，不能替代杀毒软件；扫描只读，不会删除或隔离任何文件。',
  'security.fail': '操作失败：{err}',

  'security.scan.title': '恶意模组扫描',
  'security.scan.one': '扫描所选版本',
  'security.scan.all': '扫描全部版本',
  'security.scan.running': '正在扫描 jar / zip（只读）…',
  'security.scan.hint': '选择一个版本后点「扫描所选版本」；扫描只读，不会改动文件。',
  'security.scan.empty': '这个版本没有 mods 目录',
  'security.scan.count': '已扫描 {n} 个文件',
  'security.scan.clean': '没有发现任何提示，看起来很干净',
  'security.scan.detail': '详情',
  'security.scan.openDir': '打开目录',
  'security.scan.noFinding': '没有可展开的提示',
  'security.scan.noVersion': '没有可用版本',
  'security.scan.done': '扫描完成，共 {n} 个文件',

  'security.risk.high': '高危',
  'security.risk.medium': '可疑',
  'security.risk.low': '提示',
  'security.risk.none': '正常',
  'security.risk.score': '风险 {s}',

  'security.news.title': 'Minecraft 新闻',
  'security.news.refresh': '刷新',
  'security.news.hint': '点「刷新」获取官方新闻；离线时会自动显示本地缓存。',
  'security.news.loading': '正在获取官方新闻…',
  'security.news.open': '打开原文',
  'security.news.empty': '暂时没有新闻',
  'security.news.demoNote':
    '离线示例：以下内容不是真实新闻，联网后点「刷新」获取官方动态。',
  'security.news.cachedNote': '当前显示本地缓存，联网后点「刷新」可获取最新新闻。',
});

window.GM.i18n.add('en-US', {
  'nav.security': 'Security',

  'security.title': 'Security Center',
  'security.sub': 'Heuristic mod scan · Official Minecraft news',
  'security.disclaimer':
    'Heuristic hints only, not a substitute for antivirus. Scanning is read-only and never deletes or quarantines files.',
  'security.fail': 'Failed: {err}',

  'security.scan.title': 'Malicious mod scan',
  'security.scan.one': 'Scan selected version',
  'security.scan.all': 'Scan all versions',
  'security.scan.running': 'Scanning jar / zip (read-only)…',
  'security.scan.hint': 'Pick a version and press “Scan selected version”; scanning is read-only.',
  'security.scan.empty': 'This version has no mods folder',
  'security.scan.count': 'Scanned {n} files',
  'security.scan.clean': 'No hints found — looks clean',
  'security.scan.detail': 'Details',
  'security.scan.openDir': 'Open folder',
  'security.scan.noFinding': 'No details to expand',
  'security.scan.noVersion': 'No versions available',
  'security.scan.done': 'Scan complete, {n} files',

  'security.risk.high': 'High',
  'security.risk.medium': 'Medium',
  'security.risk.low': 'Low',
  'security.risk.none': 'Clean',
  'security.risk.score': 'Risk {s}',

  'security.news.title': 'Minecraft news',
  'security.news.refresh': 'Refresh',
  'security.news.hint': 'Press “Refresh” to fetch official news; offline falls back to the local cache.',
  'security.news.loading': 'Fetching official news…',
  'security.news.open': 'Open article',
  'security.news.empty': 'No news at the moment',
  'security.news.demoNote':
    'Offline example: the items below are not real news. Connect and press “Refresh”.',
  'security.news.cachedNote': 'Showing local cache. Press “Refresh” online for the latest.',
});
