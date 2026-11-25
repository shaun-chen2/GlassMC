/* =====================================================================
 * features-mcp.js —— MCP 服务端设置卡片
 *
 * 不新增侧栏入口：只在设置页幂等注入一张卡片，展示本机可执行文件路径、
 * 客户端配置示例与工具数量。真正的协议实现在 Rust 的 --mcp 模式里。
 * ===================================================================== */
(function () {
  'use strict';
  const GM = (window.GM = window.GM || {});
  const t = (key, vars) => (typeof GM.t === 'function' ? GM.t('mcp.' + key, vars) : key);
  const esc = (s) => (typeof GM.esc === 'function' ? GM.esc(s) : String(s == null ? '' : s));

  let status = null;
  let loading = false;
  let loadRetry = 0;

  function cardHtml() {
    return (
      '<div class="card" id="mcp-card">' +
        '<div class="card-title" data-i18n="mcp.title">MCP 服务端（stdio）</div>' +
        '<div class="desc" data-i18n="mcp.desc">把 GlassMC 作为 MCP 工具服务器提供给 Claude 等客户端。</div>' +
        '<div class="field">' +
          '<div class="lbl" data-i18n="mcp.executable">可执行文件路径</div>' +
          '<div class="mono-dim" id="mcp-exec">—</div>' +
        '</div>' +
        '<div class="field hstack" style="gap:12px;flex-wrap:wrap">' +
          '<span class="badge-pill" id="mcp-args">—</span>' +
          '<span class="badge-pill ok" id="mcp-tools">—</span>' +
        '</div>' +
        '<div class="field">' +
          '<div class="lbl" data-i18n="mcp.config">客户端配置示例（mcp.json）</div>' +
          '<pre id="mcp-config" style="white-space:pre-wrap;word-break:break-all;max-height:180px;overflow:auto">—</pre>' +
        '</div>' +
        '<div class="hstack" style="gap:8px;flex-wrap:wrap">' +
          '<button class="btn primary" id="mcp-copy" data-i18n="mcp.copy">复制配置</button>' +
          '<button class="btn" id="mcp-refresh" data-i18n="mcp.refresh">刷新状态</button>' +
        '</div>' +
        '<div class="desc" style="margin-top:8px" data-i18n="mcp.tip">工具调用只在本机执行。</div>' +
        '<div class="desc" id="mcp-win-tip" style="display:none" data-i18n="mcp.tip.win">Windows 正式版没有控制台。</div>' +
      '</div>'
    );
  }

  async function load() {
    // 首帧 ready 钩子可能早于 app.js 暴露 __GM，等它就绪再取状态
    if (!window.__GM || typeof window.__GM.invoke !== 'function') {
      if (loadRetry++ < 10) setTimeout(load, 150);
      return;
    }
    if (loading) return;
    loading = true;
    loadRetry = 0;
    try {
      status = await GM.invoke('mcp_status');
      if (!status) throw new Error('empty mcp_status');
      const exec = document.getElementById('mcp-exec');
      const args = document.getElementById('mcp-args');
      const tools = document.getElementById('mcp-tools');
      const cfg = document.getElementById('mcp-config');
      if (exec) exec.textContent = status.executable || '—';
      if (args) args.textContent = (status.args || []).join(' ');
      if (tools) tools.textContent = t('tools') + ': ' + (status.toolCount || 0);
      if (cfg) cfg.textContent = status.configExample || '—';
      const isWin = /win/i.test(navigator.userAgent) || /^[A-Za-z]:[\\/]/.test(status.executable || '');
      const win = document.getElementById('mcp-win-tip');
      if (win) win.style.display = isWin ? '' : 'none';
    } catch (e) {
      GM.toast(t('status.fail', { err: String((e && e.message) || e) }), 'err');
    } finally {
      loading = false;
    }
  }

  function copyConfig() {
    const text = (status && status.configExample) || '';
    if (!text) return;
    const done = () => GM.toast(t('copy.ok'), 'ok');
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done).catch(fallback);
    } else {
      fallback();
    }
    function fallback() {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try {
        document.execCommand('copy');
        done();
      } catch (e) {
        GM.toast(t('copy.fail'), 'err');
      }
      document.body.removeChild(ta);
    }
  }

  function inject() {
    const page = document.querySelector('.page[data-page="settings"]');
    if (!page) return;
    let card = document.getElementById('mcp-card');
    if (!card) {
      const grid = page.querySelector('.grid-2') || page;
      grid.insertAdjacentHTML('beforeend', cardHtml());
      card = document.getElementById('mcp-card');
    }
    if (card && !card.dataset.bound) {
      card.dataset.bound = '1';
      card.querySelector('#mcp-copy').onclick = copyConfig;
      card.querySelector('#mcp-refresh').onclick = load;
      load();
    } else if (card && status) {
      // 切换语言后动态拼出来的工具数标签不会被 i18n.apply 自动刷新
      const label = card.querySelector('#mcp-tools');
      if (label) label.textContent = t('tools') + ': ' + (status.toolCount || 0);
    }
  }

  GM.register({
    ready: inject,
    render: inject,
    nav: (page) => {
      if (page !== 'settings') return;
      inject();
      if (!status) load();
    },
  });
})();
