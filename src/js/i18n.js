//! 极简 i18n：`data-i18n` 管静态文案，`GM.t('key', vars)` 管动态文案。
//!
//! 设计取舍：
//! - 不做构建期抽取（项目没有打包器），语言表就是普通 JS，`GM.i18n.add()` 注册；
//! - 每种语言拆成多个文件按模块追加，避免几个人同时改同一个大表；
//! - 键缺失时回退到中文，再回退到键名，界面上不会出现空白。

(function () {
  const GM = (window.GM = window.GM || {});
  const dicts = Object.create(null);
  const listeners = [];
  let locale = 'zh-CN';

  const i18n = {
    add(loc, table) {
      if (!loc || !table) return;
      dicts[loc] = Object.assign(dicts[loc] || {}, table);
    },

    locales() {
      return Object.keys(dicts);
    },

    normalize(loc) {
      const v = String(loc || '').toLowerCase();
      if (v.startsWith('zh')) return 'zh-CN';
      if (v.startsWith('en')) return 'en-US';
      return 'zh-CN';
    },

    /** 把配置里的 language（可能是 system/空）解析成具体语言。 */
    resolve(setting) {
      const s = String(setting || '').trim();
      if (!s || s === 'system') return i18n.normalize(navigator.language);
      return i18n.normalize(s);
    },

    setLocale(loc) {
      const next = dicts[loc] ? loc : 'zh-CN';
      if (next === locale) return;
      locale = next;
      document.documentElement.lang = locale;
      i18n.apply(document);
      listeners.forEach((fn) => {
        try {
          fn(locale);
        } catch (e) {
          console.error('[i18n]', e);
        }
      });
    },

    get locale() {
      return locale;
    },

    t(key, vars) {
      const table = dicts[locale] || {};
      const fallback = dicts['zh-CN'] || {};
      let s = table[key];
      if (s == null) s = fallback[key];
      if (s == null) s = key;
      if (vars) {
        s = s.replace(/\{(\w+)\}/g, (m, k) => (vars[k] == null ? m : String(vars[k])));
      }
      return s;
    },

    onChange(fn) {
      if (typeof fn === 'function') listeners.push(fn);
    },

    /** 把当前页面里带 data-i18n* 的节点刷一遍。 */
    apply(root) {
      const doc = root || document;
      doc.querySelectorAll('[data-i18n]').forEach((el) => {
        el.textContent = i18n.t(el.dataset.i18n);
      });
      doc.querySelectorAll('[data-i18n-placeholder]').forEach((el) => {
        el.setAttribute('placeholder', i18n.t(el.dataset.i18nPlaceholder));
      });
      doc.querySelectorAll('[data-i18n-title]').forEach((el) => {
        el.setAttribute('title', i18n.t(el.dataset.i18nTitle));
      });
      doc.querySelectorAll('[data-i18n-aria-label]').forEach((el) => {
        el.setAttribute('aria-label', i18n.t(el.dataset.i18nAriaLabel));
      });
    },
  };

  GM.i18n = i18n;
  GM.t = i18n.t;
})();
