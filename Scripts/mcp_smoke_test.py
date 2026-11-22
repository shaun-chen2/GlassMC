#!/usr/bin/env python3
"""MCP 客户端冒烟测试：把 GlassMC 当真正的 MCP 服务端来对话。

用法：
    python3 Scripts/mcp_smoke_test.py [glassmc 可执行文件]

会自动创建临时 GLASSMC_DATA，不会碰真实数据；只调用只读工具，
launch / backup_save 用不存在的版本调用，验证错误路径而不真的启动游戏。
"""

import json
import os
import select
import socket
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_EXE = ROOT / "src-tauri" / "target" / "debug" / "glassmc"

EXPECTED_TOOLS = {
    "list_versions", "get_config", "list_mods", "list_saves", "backup_save",
    "crash_explain", "cleanup_scan", "mod_conflicts", "server_ping",
    "frp_status", "auto_update_status", "launch",
}


def varint(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        out.append(b | (0x80 if n else 0))
        if not n:
            return bytes(out)


class FakeSLPServer(threading.Thread):
    """一个最小的 Java 版 Server List Ping 应答端，用来验证 server_ping 工具。"""

    def __init__(self):
        super().__init__(daemon=True)
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(1)
        self.port = self.sock.getsockname()[1]

    def run(self):
        try:
            conn, _ = self.sock.accept()
            conn.settimeout(3)
            conn.recv(4096)  # handshake + status request
            payload = json.dumps({
                "description": {"text": "GlassMC MCP Test"},
                "players": {"online": 5, "max": 20},
                "version": {"name": "1.21.1", "protocol": 765},
            }).encode()
            body = b"\x00" + varint(len(payload)) + payload
            conn.sendall(varint(len(body)) + body)
            conn.close()
        except Exception:
            pass


class MCPClient:
    def __init__(self, exe: Path):
        self.tmp = tempfile.TemporaryDirectory(prefix="glassmc-mcp-test-")
        env = dict(os.environ, GLASSMC_DATA=self.tmp.name, RUST_BACKTRACE="0")
        self.proc = subprocess.Popen(
            [str(exe), "--mcp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            text=True,
            bufsize=1,
        )

    def send(self, obj):
        self.proc.stdin.write((obj if isinstance(obj, str) else json.dumps(obj)) + "\n")
        self.proc.stdin.flush()

    def recv(self, timeout=20):
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            raise TimeoutError("MCP 服务端没有在 %ss 内回复" % timeout)
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("MCP 服务端提前退出，stderr=" + self.proc.stderr.read())
        return json.loads(line)

    def request(self, rid, method, params=None):
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
        return self.recv()

    def tool(self, rid, name, args=None):
        return self.request(rid, "tools/call", {"name": name, "arguments": args or {}})

    def close(self):
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.terminate()
        self.tmp.cleanup()


def check(cond, msg, failures):
    print(("  OK   " if cond else "  FAIL ") + msg)
    if not cond:
        failures.append(msg)


def main():
    exe = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else DEFAULT_EXE.resolve()
    if not exe.is_file():
        sys.exit("找不到可执行文件：" + str(exe))
    print("测试 MCP 服务端：", exe)
    c = MCPClient(exe)
    failures = []
    try:
        init = c.request(1, "initialize", {"protocolVersion": "2024-11-05"})
        check(init.get("id") == 1, "initialize 回显 id", failures)
        result = init.get("result", {})
        check(result.get("serverInfo", {}).get("name") == "glassmc", "initialize 返回 serverInfo", failures)
        check("tools" in result.get("capabilities", {}), "initialize 声明 tools 能力", failures)
        check(bool(result.get("protocolVersion")), "initialize 返回 protocolVersion", failures)

        # 现代客户端普遍用 2025-06-18，服务端应回显它支持的版本而不是自己的固定值
        modern = c.request(3, "initialize", {"protocolVersion": "2025-06-18"})
        check(
            modern.get("result", {}).get("protocolVersion") == "2025-06-18",
            "initialize 回显客户端的 2025-06-18 协议版本",
            failures,
        )

        # 通知不回复：0.5 秒内不应有多余输出
        c.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        ready, _, _ = select.select([c.proc.stdout], [], [], 0.5)
        check(not ready, "notifications/initialized 不产生响应", failures)

        listing = c.request(2, "tools/list")
        tools = listing.get("result", {}).get("tools", [])
        names = {t.get("name") for t in tools}
        check(names == EXPECTED_TOOLS, "tools/list 工具集合完整（%d 个）" % len(tools), failures)
        for t in tools:
            check(isinstance(t.get("inputSchema"), dict), "工具 %s 有 inputSchema" % t.get("name"), failures)

        # 只读工具 + 安全的错误路径
        cases = [
            (10, "list_versions", {}),
            (11, "get_config", {}),
            (12, "list_mods", {"versionId": "no-such-version"}),
            (13, "list_saves", {"versionId": "no-such-version"}),
            (14, "backup_save", {"versionId": "no-such-version", "saveId": "world"}),
            (15, "crash_explain", {"versionId": "no-such-version"}),
            (16, "cleanup_scan", {"versionId": "no-such-version"}),
            (17, "mod_conflicts", {"versionId": "no-such-version"}),
            (18, "server_ping", {"address": "127.0.0.1", "port": 1}),
            (19, "frp_status", {}),
            (20, "auto_update_status", {}),
            (21, "launch", {"versionId": "no-such-version"}),
        ]
        for rid, name, args in cases:
            resp = c.tool(rid, name, args)
            check(resp.get("id") == rid, "%s 回显 id" % name, failures)
            body = resp.get("result", {})
            content = body.get("content")
            check(
                isinstance(content, list) and content and content[0].get("type") == "text",
                "%s 返回 text content%s" % (name, "（工具内部错误）" if body.get("isError") else ""),
                failures,
            )

        # ---------------------------------------------------------------- 协议边界
        str_id = c.request("str-id", "ping")
        check(str_id.get("id") == "str-id", "字符串 id 原样回显", failures)

        null_id = c.request(None, "ping")
        check(null_id.get("id") is None, "null id 原样回显", failures)

        unsupported = c.request(50, "initialize", {"protocolVersion": "1999-01-01"})
        check(
            unsupported.get("result", {}).get("protocolVersion") == "2024-11-05",
            "不支持的协议版本回落到 2024-11-05",
            failures,
        )

        resources = c.request(51, "resources/list")
        check(
            resources.get("result") == {"resources": []},
            "resources/list 只返回 resources 字段",
            failures,
        )
        prompts = c.request(52, "prompts/list")
        check(prompts.get("result") == {"prompts": []}, "prompts/list 只返回 prompts 字段", failures)

        # 非字符串 method / 非对象根节点：必须回 -32600，不能静默丢弃让客户端空等
        c.send({"jsonrpc": "2.0", "id": 53, "method": 123})
        bad_method = c.recv()
        check(bad_method.get("error", {}).get("code") == -32600, "非字符串 method 返回 -32600", failures)
        check(bad_method.get("id") == 53, "非字符串 method 保留 id", failures)

        c.send([1, 2, 3])
        bad_root = c.recv()
        check(bad_root.get("error", {}).get("code") == -32600, "非对象根节点返回 -32600", failures)
        check(bad_root.get("id") is None, "非对象根节点 id=null", failures)

        # 流水线：连续两条请求不等待，按顺序各拿一条响应
        c.send({"jsonrpc": "2.0", "id": 60, "method": "ping"})
        c.send({"jsonrpc": "2.0", "id": 61, "method": "tools/list"})
        r60, r61 = c.recv(), c.recv()
        check(r60.get("id") == 60 and r61.get("id") == 61, "连续请求按顺序响应", failures)

        # CRLF：Windows 客户端常见的行尾
        c.proc.stdin.write('{"jsonrpc":"2.0","id":62,"method":"ping"}\r\n')
        c.proc.stdin.flush()
        crlf = c.recv()
        check(crlf.get("id") == 62, "CRLF 行尾可解析", failures)

        # ---------------------------------------------------------------- 深度测试
        # 造一份最小实例与日志，确认工具读到的是真实数据而不是空壳
        root = Path(c.tmp.name)
        inst = root / "minecraft" / "versions" / "test"
        (inst / "mods").mkdir(parents=True, exist_ok=True)
        (inst / "test.json").write_text(json.dumps({
            "id": "test", "type": "release", "libraries": [],
            "mainClass": "net.minecraft.client.main.Main",
        }), encoding="utf-8")
        (inst / "mods" / "demo.jar").write_bytes(b"PK\x03\x04fake-jar")
        (inst / "mods" / "demo.jar.disabled").write_bytes(b"PK\x03\x04disabled")
        logs = root / "logs"
        logs.mkdir(parents=True, exist_ok=True)
        (logs / "game-latest.log").write_text(
            "---- Minecraft Crash Report ----\njava.lang.OutOfMemoryError: Java heap space\n"
            "at net.minecraft.client.main.Main.main(Main.java:42)\n", encoding="utf-8"
        )

        def text_of(resp):
            return resp.get("result", {}).get("content", [{}])[0].get("text", "")

        versions = text_of(c.tool(40, "list_versions", {}))
        check('"test"' in versions, "list_versions 能读到伪造实例", failures)
        mods = text_of(c.tool(41, "list_mods", {"versionId": "test"}))
        check("demo.jar" in mods, "list_mods 能读到伪造 jar", failures)
        crash = text_of(c.tool(42, "crash_explain", {"versionId": "test"}))
        check("oom" in crash.lower() or "内存" in crash, "crash_explain 识别 OOM", failures)

        # 造一个真正的世界目录验证备份工具（故意放无效 level.dat，验证不会崩）
        world = inst / "saves" / "world"
        world.mkdir(parents=True, exist_ok=True)
        (world / "level.dat").write_bytes(b"not-a-gzip-level-dat")
        saved = text_of(c.tool(44, "backup_save", {
            "versionId": "test", "saveId": "world", "label": "mcp-test",
        }))
        check("world" in saved, "backup_save 能备份伪造世界", failures)
        check("mcp-test" in saved or "backup" in saved.lower(), "backup_save 返回备份信息", failures)
        saves = text_of(c.tool(45, "list_saves", {"versionId": "test"}))
        check("world" in saves, "list_saves 在 level.dat 损坏时仍能列出世界", failures)

        # 重复 jar 触发清理扫描
        (inst / "mods" / "dup.jar").write_bytes(b"PK\x03\x04fake-jar")
        cleanup = text_of(c.tool(46, "cleanup_scan", {"versionId": "test"}))
        check("duplicate" in cleanup.lower(), "cleanup_scan 识别重复 jar", failures)

        # 两个 jar 声明同一个 fabric modid，应被冲突检测发现
        import zipfile
        for name in ("conflict-a.jar", "conflict-b.jar"):
            with zipfile.ZipFile(inst / "mods" / name, "w") as zf:
                zf.writestr("fabric.mod.json", json.dumps({"id": "same-mod", "version": "1.0"}))
        conflicts = text_of(c.tool(47, "mod_conflicts", {"versionId": "test"}))
        check("same-mod" in conflicts, "mod_conflicts 识别重复 modid", failures)

        slp = FakeSLPServer()
        slp.start()
        ping = text_of(c.tool(43, "server_ping", {"address": "127.0.0.1", "port": slp.port}))
        check('"ok": true' in ping or '"ok":true' in ping, "server_ping 连上本地假服务器", failures)
        check("GlassMC MCP Test" in ping, "server_ping 解析 MOTD", failures)
        check('"playersOnline": 5' in ping or '"playersOnline":5' in ping, "server_ping 解析人数", failures)

        # 错误路径
        bad = c.tool(30, "list_mods", {})
        check(bad.get("result", {}).get("isError") is True, "缺少必填参数时 isError=true", failures)
        check("versionId" in bad["result"]["content"][0]["text"], "错误信息包含缺失参数名", failures)

        unknown = c.tool(31, "not_a_tool", {})
        check(unknown.get("result", {}).get("isError") is True, "未知工具 isError=true", failures)

        for bad_port in (-1, 0, 65536, "25565"):
            resp = c.tool(35, "server_ping", {"address": "127.0.0.1", "port": bad_port})
            check(
                resp.get("result", {}).get("isError") is True
                and "port" in resp["result"]["content"][0]["text"],
                "非法端口 %r 被明确拒绝" % (bad_port,),
                failures,
            )

        method = c.request(32, "not/a/method")
        check(method.get("error", {}).get("code") == -32601, "未知方法返回 -32601", failures)

        c.send('{"jsonrpc":"2.0","id":33,"method":')
        broken = c.recv()
        check(broken.get("error", {}).get("code") == -32700, "坏 JSON 返回 -32700", failures)
        check(broken.get("id") is None, "解析错误 id=null", failures)

        # ping：确认服务还活着
        pong = c.request(34, "ping")
        check(pong.get("result") == {}, "ping 返回空对象", failures)

        # 单独起一个进程验证：最后一行没有换行符、随后 EOF，也要处理并退出
        eof = MCPClient(exe)
        try:
            eof.proc.stdin.write('{"jsonrpc":"2.0","id":70,"method":"ping"}')
            eof.proc.stdin.flush()
            eof.proc.stdin.close()
            no_nl = eof.recv(5)
            check(no_nl.get("id") == 70, "EOF 前无换行符的请求也能处理", failures)
            check(eof.proc.wait(timeout=5) == 0, "stdin EOF 后服务端正常退出", failures)
        finally:
            eof.close()
    except Exception as exc:
        print("  测试异常：", repr(exc))
        failures.append("测试异常：" + repr(exc))
    finally:
        c.close()

    print("\n结果：%d 项失败" % len(failures))
    for f in failures:
        print(" -", f)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
