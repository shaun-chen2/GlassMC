/* MCP 服务端：中英文案。由 index.html 在 i18n.js 之后加载。 */
(function () {
  const GM = (window.GM = window.GM || {});
  if (!GM.i18n) return;

  GM.i18n.add('zh-CN', {
    'mcp.title': 'MCP 服务端（stdio）',
    'mcp.desc': '把 GlassMC 作为 MCP 工具服务器提供给 Claude 等客户端。客户端拉起 glassmc --mcp，通过标准输入输出通信。',
    'mcp.executable': '可执行文件路径',
    'mcp.args': '启动参数',
    'mcp.tools': '已暴露工具数',
    'mcp.config': '客户端配置示例（mcp.json）',
    'mcp.copy': '复制配置',
    'mcp.copy.ok': 'MCP 配置已复制',
    'mcp.copy.fail': '复制失败，请手动选中文本复制',
    'mcp.refresh': '刷新状态',
    'mcp.status.fail': '读取 MCP 状态失败：{err}',
    'mcp.tip': '工具调用只在本机执行。清理类工具只扫描不删除；启动和备份必须由你在客户端明确请求。',
    'mcp.tip.win': 'Windows 正式版是无控制台的 GUI 程序；若客户端无法通过 stdio 连接，请改用 debug 构建或自行用控制台包装。'
  });

  GM.i18n.add('en-US', {
    'mcp.title': 'MCP Server (stdio)',
    'mcp.desc': 'Expose GlassMC as an MCP tool server to clients such as Claude. Clients spawn glassmc --mcp and talk over stdin/stdout.',
    'mcp.executable': 'Executable path',
    'mcp.args': 'Launch arguments',
    'mcp.tools': 'Tools exposed',
    'mcp.config': 'Client configuration example (mcp.json)',
    'mcp.copy': 'Copy configuration',
    'mcp.copy.ok': 'MCP configuration copied',
    'mcp.copy.fail': 'Copy failed, please select the text manually',
    'mcp.refresh': 'Refresh status',
    'mcp.status.fail': 'Failed to read MCP status: {err}',
    'mcp.tip': 'Tool calls run locally. Cleanup tools only scan and never delete; launching and backups require an explicit request from your client.',
    'mcp.tip.win': 'The Windows release is a GUI binary without a console. If stdio connection fails, use a debug build or wrap it with a console.'
  });
})();
