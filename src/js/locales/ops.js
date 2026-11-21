// 实例运维模块（分类胶囊 / 每行菜单 / 复制 / 归档 / 桌面快捷方式）的文案表。
// 键统一用 ops. 前缀，features-ops.js 里通过 GM.t('ops.' + key) 取用。
window.GM.i18n.add('zh-CN', {
  'ops.menu': '实例…',
  'ops.menu.title': '实例「{id}」',
  'ops.menu.sub': '复制、分类、归档，或建一个桌面快捷方式',

  'ops.copy': '复制实例',
  'ops.copy.title': '复制「{id}」',
  'ops.copy.sub': '副本是完整独立的目录，之后的改动不会影响原实例',
  'ops.copy.name': '新实例名称',
  'ops.copy.suffix': '-副本',
  'ops.copy.withMods': '包含模组',
  'ops.copy.withSaves': '包含存档',
  'ops.copy.running': '正在复制实例',
  'ops.copy.done': '已创建 {name}',
  'ops.copy.empty': '名称不能为空',

  'ops.cat': '设置分类',
  'ops.cat.title': '给「{id}」分类',
  'ops.cat.sub': '留空即取消分类；同名分类会自动归到一起',
  'ops.cat.name': '分类名',
  'ops.cat.ph': '例如 生存',
  'ops.cat.existing': '已有分类',
  'ops.cat.done': '分类已更新',
  'ops.cat.cleared': '已取消分类',

  'ops.arch': '归档',
  'ops.unarch': '取消归档',
  'ops.arch.done': '已归档 {id}',
  'ops.unarch.done': '已取消归档 {id}',

  'ops.shortcut': '创建桌面快捷方式',
  'ops.shortcut.ok': '快捷方式已创建：{path}',

  'ops.badge.archived': '已归档',
  'ops.filter.all': '全部',
  'ops.filter.none': '未分类',
  'ops.empty': '当前筛选下没有实例',
  'ops.mock': '浏览器预览模式下不可用',
  'ops.fail': '操作失败：{err}',
  'ops.confirm': '确定',
  'ops.close': '关闭',
});

window.GM.i18n.add('en-US', {
  'ops.menu': 'Instance…',
  'ops.menu.title': 'Instance “{id}”',
  'ops.menu.sub': 'Copy, categorize, archive, or create a desktop shortcut',

  'ops.copy': 'Copy instance',
  'ops.copy.title': 'Copy “{id}”',
  'ops.copy.sub': 'The copy is a fully independent folder; later changes stay in the copy',
  'ops.copy.name': 'New instance name',
  'ops.copy.suffix': '-copy',
  'ops.copy.withMods': 'Include mods',
  'ops.copy.withSaves': 'Include saves',
  'ops.copy.running': 'Copying instance',
  'ops.copy.done': 'Created {name}',
  'ops.copy.empty': 'Name cannot be empty',

  'ops.cat': 'Set category',
  'ops.cat.title': 'Categorize “{id}”',
  'ops.cat.sub': 'Leave empty to clear; instances with the same name share a group',
  'ops.cat.name': 'Category name',
  'ops.cat.ph': 'e.g. Survival',
  'ops.cat.existing': 'Existing',
  'ops.cat.done': 'Category updated',
  'ops.cat.cleared': 'Category cleared',

  'ops.arch': 'Archive',
  'ops.unarch': 'Unarchive',
  'ops.arch.done': 'Archived {id}',
  'ops.unarch.done': 'Unarchived {id}',

  'ops.shortcut': 'Create desktop shortcut',
  'ops.shortcut.ok': 'Shortcut created: {path}',

  'ops.badge.archived': 'Archived',
  'ops.filter.all': 'All',
  'ops.filter.none': 'Uncategorized',
  'ops.empty': 'No instances match this filter',
  'ops.mock': 'Not available in browser preview',
  'ops.fail': 'Failed: {err}',
  'ops.confirm': 'Confirm',
  'ops.close': 'Close',
});
