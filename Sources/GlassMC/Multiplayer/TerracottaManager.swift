import Foundation

/// 陶瓦联机组件相关的错误。
enum TCError: LocalizedError {
    case downloadFailed(String)
    case extractFailed(String)
    case daemonNotStarting
    case requestFailed(String)
    case badRoomCode

    var errorDescription: String? {
        switch self {
        case .downloadFailed(let s): return "联机组件下载失败：\(s)"
        case .extractFailed(let s): return "联机组件解压失败：\(s)"
        case .daemonNotStarting: return "联机组件没能启动，请重试；仍失败可重启电脑后再试。"
        case .requestFailed(let s): return "联机组件响应异常：\(s)"
        case .badRoomCode: return "邀请码格式不正确，应形如 U/XXXX-XXXX-XXXX-XXXX。"
        }
    }
}

/// 陶瓦联机（Terracotta）集成。
///
/// 做法：把 Terracotta 的官方二进制原样下载到数据目录，用 `--daemon` 以子进程托管，
/// 再通过它监听在 127.0.0.1 的 HTTP 接口驱动联机状态机。
/// 本文件只做进程与 HTTP 的封装，不含任何 Terracotta 源码。
///
/// 协议文档：https://burningtnt.github.io/Terracotta/http-api/
@MainActor
final class TerracottaManager: ObservableObject {

    static let shared = TerracottaManager()

    // MARK: 版本与来源

    /// 固定版本：联机协议与邀请码格式可能随上游变化，固定住更稳。
    static let version = "0.4.2"
    static let easytierBuild = "v2.5.0-terracotta.2"
    static let binaryName = "terracotta-0.4.2-macos-arm64"
    static let archiveName = "terracotta-0.4.2-macos-arm64-pkg.tar.gz"

    /// 官方发行包（AGPL-3.0，原样分发，未做任何修改）。
    static var downloadURL: URL {
        URL(string: "https://github.com/burningtnt/Terracotta/releases/download/v\(version)/\(archiveName)")!
    }
    /// 国内镜像（下载缓慢时兜底）。
    static var mirrorURL: URL {
        URL(string: "https://gitee.com/burningtnt/Terracotta/releases/download/v\(version)/\(archiveName)")!
    }

    // MARK: 对外状态

    @Published private(set) var status: TCStatus = .idle
    @Published private(set) var port: Int?
    @Published private(set) var serverVersion: String = ""
    @Published private(set) var easytierVersion: String = ""
    /// 组件已下载且守护进程已就绪。
    @Published private(set) var ready = false
    /// 正在下载 / 解压 / 拉起。
    @Published private(set) var preparing = false
    @Published private(set) var prepareText = ""
    @Published private(set) var error: String?

    /// 组件是否已经落在本地。
    var binaryInstalled: Bool {
        FileManager.default.isExecutableFile(atPath: Self.binaryURL.path)
    }

    // MARK: 内部状态

    private var process: Process?
    private var ownsProcess = false
    private var pollTask: Task<Void, Never>?
    private var lastIndex = -1
    private var session: URLSession?
    /// 记录上一次「用户可见」的状态，用于在状态机里做去抖。
    private var lastState = ""

    // MARK: 路径

    static var supportDir: URL {
        Paths.dataRoot.appendingPathComponent("terracotta", isDirectory: true)
    }
    static var binaryURL: URL {
        supportDir.appendingPathComponent(binaryName)
    }
    static var logURL: URL {
        supportDir.appendingPathComponent("terracotta.log")
    }
    /// Terracotta 自己维护的全局锁文件（2 字节大端端口号）。
    static var lockURL: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("terracotta/terracotta.lock")
    }

    // MARK: - 启动准备

    /// 确保组件可用并把守护进程拉起来。多次调用无副作用。
    func bootstrap() async {
        guard !preparing else { return }
        if ready { return }
        preparing = true
        error = nil
        defer { preparing = false }

        do {
            if !binaryInstalled {
                try await downloadBinary()
            } else {
                prepareText = "正在启动联机组件…"
            }
            try await startDaemon()
            let meta = try await fetchMeta()
            serverVersion = meta.version
            easytierVersion = meta.easytier
            ready = true
            prepareText = ""
            startPolling()
        } catch {
            self.error = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            prepareText = ""
        }
    }

    /// 下载官方发行包并取出其中的裸二进制（不安装 pkg，不需要管理员权限）。
    private func downloadBinary() async throws {
        let fm = FileManager.default
        try fm.createDirectory(at: Self.supportDir, withIntermediateDirectories: true)

        let tmpDir = fm.temporaryDirectory.appendingPathComponent("glassmc-terracotta", isDirectory: true)
        try? fm.removeItem(at: tmpDir)
        try fm.createDirectory(at: tmpDir, withIntermediateDirectories: true)
        let archive = tmpDir.appendingPathComponent(Self.archiveName)
        defer { try? fm.removeItem(at: tmpDir) }

        prepareText = "正在下载联机组件（约 12 MB）…"
        do {
            try await DownloadManager.download(Self.downloadURL, to: archive, retries: 1) { p, speed in
                let pct = Int(p * 100)
                Task { @MainActor in
                    self.prepareText = speed.map { "正在下载联机组件 \(pct)% · \($0)" }
                        ?? "正在下载联机组件 \(pct)%"
                }
            }
        } catch {
            // 官方源失败时退到国内镜像再试一次。
            prepareText = "官方源较慢，改用镜像下载…"
            try await DownloadManager.download(Self.mirrorURL, to: archive, retries: 1) { p, speed in
                let pct = Int(p * 100)
                Task { @MainActor in
                    self.prepareText = speed.map { "正在下载联机组件 \(pct)% · \($0)" }
                        ?? "正在下载联机组件 \(pct)%"
                }
            }
        }

        guard fm.fileExists(atPath: archive.path) else {
            throw TCError.downloadFailed("文件未落盘")
        }

        prepareText = "正在解压联机组件…"
        // 只取出裸二进制，跳过其中的 .pkg（那是给系统级安装用的，我们不需要）。
        let tar = Process()
        tar.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
        tar.arguments = ["-xzf", archive.path, "-C", Self.supportDir.path, Self.binaryName]
        let pipe = Pipe()
        tar.standardError = pipe
        do {
            try tar.run()
            tar.waitUntilExit()
        } catch {
            throw TCError.extractFailed(error.localizedDescription)
        }
        if tar.terminationStatus != 0 {
            let msg = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            throw TCError.extractFailed(msg.isEmpty ? "tar 退出码 \(tar.terminationStatus)" : msg)
        }
        guard fm.fileExists(atPath: Self.binaryURL.path) else {
            throw TCError.extractFailed("压缩包里没有找到 \(Self.binaryName)")
        }
        // 去掉隔离属性并赋予可执行权限，否则 Gatekeeper 会拦下子进程。
        _ = runQuietly("/usr/bin/xattr", ["-c", Self.binaryURL.path])
        try fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: Self.binaryURL.path)
    }

    // MARK: - 守护进程

    /// 起一个 daemon；若已经有可用的陶瓦在跑，则直接复用它的端口。
    private func startDaemon() async throws {
        if let p = readLockPort(), await ping(p) {
            port = p
            ownsProcess = false
            return
        }
        // 清掉陈旧锁（上次进程被强杀时会残留，导致新进程误判）。
        try? FileManager.default.removeItem(at: Self.lockURL)

        prepareText = "正在启动联机组件…"
        let proc = Process()
        proc.executableURL = Self.binaryURL
        proc.arguments = ["--daemon"]
        proc.currentDirectoryURL = Self.supportDir
        FileManager.default.createFile(atPath: Self.logURL.path, contents: nil)
        if let handle = try? FileHandle(forWritingTo: Self.logURL) {
            proc.standardOutput = handle
            proc.standardError = handle
        }
        do {
            try proc.run()
        } catch {
            throw TCError.daemonNotStarting
        }
        process = proc
        ownsProcess = true

        // 守护进程把端口写进锁文件，最多等 15 秒。
        for _ in 0..<60 {
            try await Task.sleep(nanoseconds: 250_000_000)
            if let p = readLockPort(), await ping(p) {
                port = p
                return
            }
            if !proc.isRunning { break }
        }
        throw TCError.daemonNotStarting
    }

    /// 读锁文件里的端口（2 字节大端）。
    private func readLockPort() -> Int? {
        guard let data = try? Data(contentsOf: Self.lockURL), data.count >= 2 else { return nil }
        let p = Int(data[data.startIndex]) << 8 | Int(data[data.startIndex + 1])
        return p > 0 ? p : nil
    }

    /// 探测端口上是否真的有陶瓦在响应。
    private func ping(_ port: Int) async -> Bool {
        await rawGet(port, "/state")?.0 == 200
    }

    // MARK: - HTTP

    private func httpSession() -> URLSession {
        if let s = session { return s }
        let cfg = URLSessionConfiguration.ephemeral
        cfg.requestCachePolicy = .reloadIgnoringLocalCacheData
        cfg.timeoutIntervalForRequest = 6
        cfg.timeoutIntervalForResource = 10
        // 关键：不走系统代理，否则 127.0.0.1 的请求会被代理吞掉。
        cfg.connectionProxyDictionary = [:]
        let s = URLSession(configuration: cfg)
        session = s
        return s
    }

    private func rawGet(_ port: Int, _ path: String) async -> (Int, Data)? {
        guard let url = URL(string: "http://127.0.0.1:\(port)\(path)") else { return nil }
        do {
            let (data, resp) = try await httpSession().data(from: url)
            let code = (resp as? HTTPURLResponse)?.statusCode ?? 0
            return (code, data)
        } catch {
            return nil
        }
    }

    private func get(_ path: String) async throws -> (Int, Data) {
        guard let port else { throw TCError.requestFailed("组件未启动") }
        guard let r = await rawGet(port, path) else { throw TCError.requestFailed("无法连接本地组件") }
        return r
    }

    private struct Meta: Decodable {
        var version: String
        var easytier_version: String
        var easytier: String { easytier_version }
    }

    private func fetchMeta() async throws -> Meta {
        let (code, data) = try await get("/meta")
        guard code == 200 else { throw TCError.requestFailed("HTTP \(code)") }
        return try JSONDecoder().decode(Meta.self, from: data)
    }

    // MARK: - 状态轮询

    private func startPolling() {
        pollTask?.cancel()
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                let interval: UInt64 = (self?.status.state == "waiting") ? 900_000_000 : 600_000_000
                try? await Task.sleep(nanoseconds: interval)
            }
        }
    }

    /// 拉一次 /state；用 index 去重，避免重复刷新界面。
    func refresh() async {
        guard ready, let port else { return }
        guard let (code, data) = await rawGet(port, "/state"), code == 200 else {
            // 连续失败说明进程没了，标记未就绪。
            error = "联机组件已退出，请重新启动。"
            ready = false
            return
        }
        guard let next = try? JSONDecoder().decode(TCStatus.self, from: data) else { return }
        if next.index != lastIndex || next.state != lastState {
            lastIndex = next.index
            lastState = next.state
            status = next
        }
    }

    // MARK: - 操作

    /// 以房主身份创建房间（要求游戏已经「对局域网开放」）。
    func createRoom(player: String) async {
        error = nil
        await command("/state/scanning?player=\(esc(player))")
        await refresh()
    }

    /// 加入别人的房间。
    func join(room: String, player: String) async {
        error = nil
        let code = room.trimmingCharacters(in: .whitespacesAndNewlines)
        guard code.uppercased().hasPrefix("U/") else {
            error = TCError.badRoomCode.errorDescription
            return
        }
        let (statusCode, _) = (try? await get("/state/guesting?room=\(esc(code))&player=\(esc(player))")) ?? (0, Data())
        if statusCode == 400 {
            error = "邀请码无效，或当前状态不允许加入。"
            return
        }
        await refresh()
    }

    /// 回到待机（相当于「返回主页」）。
    func leave() async {
        error = nil
        await command("/state/ide")
        await refresh()
    }

    /// 退出组件：把自己拉起的守护进程关掉。复用别人已运行的实例时不动它。
    func shutdown() {
        pollTask?.cancel()
        pollTask = nil
        guard ownsProcess, let port else { return }
        Task.detached {
            if let url = URL(string: "http://127.0.0.1:\(port)/panic?peaceful=true") {
                var req = URLRequest(url: url)
                req.timeoutInterval = 3
                _ = try? await URLSession.shared.data(for: req)
            }
        }
        process?.terminate()
        process = nil
        ownsProcess = false
        ready = false
    }

    private func command(_ path: String) async {
        _ = try? await get(path)
    }

    /// 简单 URL 编码（参数里可能出现中文玩家名）。
    private func esc(_ s: String) -> String {
        s.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? s
    }

    @discardableResult
    private func runQuietly(_ path: String, _ args: [String]) -> Int32 {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: path)
        p.arguments = args
        p.standardOutput = Pipe()
        p.standardError = Pipe()
        do { try p.run() } catch { return -1 }
        p.waitUntilExit()
        return p.terminationStatus
    }
}
