//! 无障碍 / 背景预览模块的文案。
//! 只负责 JS 注入的新 UI；设置页原有的静态标签仍由 locales/html.js 提供。
//! 所有 key 在 zh-CN / en-US 两侧成对出现，缺键时 i18n 会回退到中文。

window.GM.i18n.add('zh-CN', {
  'a11y.skip': '跳到主内容',

  'a11y.preview.title': '背景图预览',
  'a11y.preview.empty': '未设置背景图',
  'a11y.preview.hint': '在上方「个性化」中选择图片后，这里会实时显示预览；图片只在本地读取。',
  'a11y.preview.alt': '当前背景图预览',
  'a11y.preview.refresh': '刷新预览',
  'a11y.preview.on': '已设置',
  'a11y.preview.off': '未设置',
  'a11y.preview.failed': '预览读取失败',

  'a11y.card.title': '无障碍选项',
  'a11y.card.sub': '让界面更易读、更易操作',
  'a11y.shortcuts.title': '键盘快捷键',
  'a11y.display.title': '显示与辅助',
  'a11y.shortcut.nav': '切换第 1–9 个侧栏页面',
  'a11y.shortcut.launch': '启动当前实例',
  'a11y.shortcut.settings': '打开设置',
  'a11y.shortcut.search': '聚焦当前页面搜索框',
  'a11y.shortcut.escape': '关闭弹层',
  'a11y.shortcut.rerender': '重新渲染界面',

  'a11y.toggle.largeText': '大字体',
  'a11y.toggle.largeText.desc': '放大界面文字与控件，阅读更轻松。',
  'a11y.toggle.highContrast': '高对比度',
  'a11y.toggle.highContrast.desc': '加强文字与边框对比，弱光或弱视下更清晰。',
  'a11y.toggle.reduceMotion': '减少动画',
  'a11y.toggle.reduceMotion.desc': '关闭界面过渡与页面动画。',

  'a11y.toast.saved': '无障碍设置已保存',
});

window.GM.i18n.add('en-US', {
  'a11y.skip': 'Skip to main content',

  'a11y.preview.title': 'Background preview',
  'a11y.preview.empty': 'No background image set',
  'a11y.preview.hint': 'Pick an image in Personalization above and it will preview here; images are read locally only.',
  'a11y.preview.alt': 'Current background preview',
  'a11y.preview.refresh': 'Refresh preview',
  'a11y.preview.on': 'Set',
  'a11y.preview.off': 'Not set',
  'a11y.preview.failed': 'Could not load preview',

  'a11y.card.title': 'Accessibility options',
  'a11y.card.sub': 'Make the interface easier to read and use',
  'a11y.shortcuts.title': 'Keyboard shortcuts',
  'a11y.display.title': 'Display & assistance',
  'a11y.shortcut.nav': 'Switch to sidebar page 1–9',
  'a11y.shortcut.launch': 'Launch the selected instance',
  'a11y.shortcut.settings': 'Open settings',
  'a11y.shortcut.search': 'Focus the current page search box',
  'a11y.shortcut.escape': 'Close the modal',
  'a11y.shortcut.rerender': 'Re-render the interface',

  'a11y.toggle.largeText': 'Larger text',
  'a11y.toggle.largeText.desc': 'Enlarge interface text and controls for easier reading.',
  'a11y.toggle.highContrast': 'High contrast',
  'a11y.toggle.highContrast.desc': 'Stronger text and borders for low light or low vision.',
  'a11y.toggle.reduceMotion': 'Reduce motion',
  'a11y.toggle.reduceMotion.desc': 'Disable interface transitions and page animations.',

  'a11y.toast.saved': 'Accessibility settings saved',
});
