//! 个性化模块的**动态**文案：背景 / 音效的状态、文件名占位、操作提示。
//!
//! 设置页卡片上的静态标签由 locales/html.js 提供（`html.*`），
//! 这里只放 JS 运行时才会出现的句子，避免两边各写一份同样的文案。
window.GM.i18n.add('zh-CN', {
  'prefs.language.zh': '简体中文',
  'prefs.language.en': 'English',

  'prefs.bg.on': '已设置',
  'prefs.bg.off': '未设置',

  'prefs.file.none': '未设置',

  'prefs.toast.lang': '界面语言已切换为{lang}',
  'prefs.toast.bg': '背景图已更新',
  'prefs.toast.bgClear': '已清除背景图',
  'prefs.toast.bgFail': '背景图读取失败：{msg}',
  'prefs.toast.sound': '启动音效已设置',
  'prefs.toast.soundClear': '已关闭启动音效',
  'prefs.toast.soundFail': '音效播放失败：{msg}',
});

window.GM.i18n.add('en-US', {
  'prefs.language.zh': '简体中文',
  'prefs.language.en': 'English',

  'prefs.bg.on': 'Set',
  'prefs.bg.off': 'Not set',

  'prefs.file.none': 'Not set',

  'prefs.toast.lang': 'Language switched to {lang}',
  'prefs.toast.bg': 'Background updated',
  'prefs.toast.bgClear': 'Background cleared',
  'prefs.toast.bgFail': 'Could not load background: {msg}',
  'prefs.toast.sound': 'Launch sound set',
  'prefs.toast.soundClear': 'Launch sound turned off',
  'prefs.toast.soundFail': 'Could not play sound: {msg}',
});
