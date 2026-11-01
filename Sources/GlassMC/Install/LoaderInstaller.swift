import Foundation

/// 支持的模组加载器。
enum LoaderKind: String, CaseIterable, Identifiable {
    case fabric
    case quilt
    case neoforge
    case forge

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .fabric: "Fabric"
        case .quilt: "Quilt"
        case .neoforge: "NeoForge"
        case .forge: "Forge"
        }
    }

    /// 是否需要跑官方 installer jar（NeoForge / Forge），而不是直接取 profile.json。
    fileprivate var usesInstaller: Bool {
        self == .neoforge || self == .forge
    }
}

/// 安装加载器时可能出现的错误。
enum LoaderInstallError: LocalizedError {
    /// 版本元数据（maven-metadata.xml / 加载器版本列表）获取或解析失败
    case metadataFailed(String)
    /// 加载器 profile.json 获取或解析失败
    case profileFailed(String)
    /// 安装器进程以非 0 退出（退出码、末尾输出）
    case installerExit(Int32, String)
    /// 没有找到安装后的版本目录
    case versionNotFound(String)

    var errorDescription: String? {
        switch self {
        case .metadataFailed(let detail):
            return "获取加载器版本列表失败：\(detail)"
        case .profileFailed(let detail):
            return "获取加载器配置失败：\(detail)"
        case .installerExit(let code, let output):
            let tail = output.isEmpty ? "（安装器没有输出任何信息）" : "\n\(output)"
            return "加载器安装器执行失败（退出码 \(code)）。请确认该版本所需的 Java 已安装，或稍后重试。\(tail)"
        case .versionNotFound(let detail):
            return "未能定位安装后的加载器版本：\(detail)"
        }
    }
}

/// 在线安装模组加载器：Fabric / Quilt 直接写 profile.json；
/// NeoForge / Forge 需要运行官方 installer，再对生成的版本补齐依赖。
enum LoaderInstaller {

    private static let neoForgeMirrorBase = "https://bmclapi2.bangbang93.com/neoforge/maven"
    private static let neoForgeMavenBase = "https://maven.neoforged.net/releases"
    private static let forgeMavenBase = "https://maven.minecraftforge.net"

    // MARK: - 版本列表

    /// 返回指定游戏版本可安装的加载器版本号（新的在前）。
    static func versions(kind: LoaderKind, game: String) async throws -> [String] {
        switch kind {
        case .fabric:
            return try await metaVersions(base: "https://meta.fabricmc.net/v2/versions/loader/\(game)", label: "Fabric")
        case .quilt:
            return try await metaVersions(base: "https://meta.quiltmc.org/v3/versions/loader/\(game)", label: "Quilt")
        case .neoforge:
            return try await neoForgeVersions(game: game)
        case .forge:
            return try await forgeVersions(game: game)
        }
    }

    /// Fabric / Quilt 的版本列表接口，结构为 `[{"loader":{"version":"0.16.10",...}}, ...]`。
    private static func metaVersions(base: String, label: String) async throws -> [String] {
        guard let url = URL(string: base) else {
            throw LoaderInstallError.metadataFailed("\(label) 接口地址无效")
        }
        let data: Data
        do {
            data = try await requestData(url)
        } catch {
            throw LoaderInstallError.metadataFailed("\(label) 接口不可用（\(error.localizedDescription)）")
        }
        guard let list = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            throw LoaderInstallError.metadataFailed("\(label) 返回内容不是合法的版本列表")
        }
        let result = list.compactMap { ($0["loader"] as? [String: Any])?["version"] as? String }
        guard !result.isEmpty else {
            throw LoaderInstallError.versionNotFound("\(label) 没有适用于该游戏版本的加载器版本")
        }
        return result.sorted { isNewer($0, than: $1) }
    }

    /// NeoForge：从 maven-metadata.xml 取版本，按游戏版本换算前缀过滤（1.21.1 → 21.1.）。
    private static func neoForgeVersions(game: String) async throws -> [String] {
        let url = URL(string: "\(neoForgeMavenBase)/net/neoforged/neoforge/maven-metadata.xml")!
        let xml: String
        do {
            xml = try await requestText(url)
        } catch {
            throw LoaderInstallError.metadataFailed("NeoForge 元数据不可用（\(error.localizedDescription)）")
        }
        let all = xmlVersions(xml)
        guard !all.isEmpty else {
            throw LoaderInstallError.metadataFailed("NeoForge 元数据中没有解析到任何版本号")
        }
        guard let prefix = neoForgePrefix(game: game) else {
            throw LoaderInstallError.versionNotFound("无法由游戏版本 \(game) 推导 NeoForge 版本前缀")
        }
        let matched = all.filter { $0.hasPrefix(prefix) && !$0.contains("-beta") && !$0.contains("-alpha") }
        guard !matched.isEmpty else {
            throw LoaderInstallError.versionNotFound("NeoForge 没有适用于 \(game) 的版本（需要 \(prefix)x）")
        }
        return matched.sorted { isNewer($0, than: $1) }
    }

    /// Forge：`1.21.1-52.0.1` → 去掉游戏版本前缀后返回 `52.0.1`。
    private static func forgeVersions(game: String) async throws -> [String] {
        let url = URL(string: "\(forgeMavenBase)/net/minecraftforge/forge/maven-metadata.xml")!
        let xml: String
        do {
            xml = try await requestText(url)
        } catch {
            throw LoaderInstallError.metadataFailed("Forge 元数据不可用（\(error.localizedDescription)）")
        }
        let all = xmlVersions(xml)
        guard !all.isEmpty else {
            throw LoaderInstallError.metadataFailed("Forge 元数据中没有解析到任何版本号")
        }
        let prefix = game + "-"
        let matched = all.filter { $0.hasPrefix(prefix) }
            .map { String($0.dropFirst(prefix.count)) }
            .filter { !$0.isEmpty }
        guard !matched.isEmpty else {
            throw LoaderInstallError.versionNotFound("Forge 没有适用于 \(game) 的版本")
        }
        return matched.sorted { isNewer($0, than: $1) }
    }

    /// `1.21.1` → `21.1.`；`1.21` → `21.0.`；`1.20.1` → `20.1.`。
    private static func neoForgePrefix(game: String) -> String? {
        let parts = game.split(separator: ".").map(String.init)
        guard parts.count >= 2, parts[0] == "1" else { return nil }
        let minor = parts[1]
        let patch = parts.count >= 3 ? parts[2] : "0"
        guard Int(minor) != nil, Int(patch) != nil else { return nil }
        return "\(minor).\(patch)."
    }

    // MARK: - 安装

    /// 安装加载器，返回安装后的版本 id（需与 account 一起用于启动）。
    @discardableResult
    static func install(kind: LoaderKind,
                        game: String,
                        loaderVersion: String,
                        progress: @escaping (Double, String?) -> Void) async throws -> String {
        Paths.ensureDirectories()
        AppState.shared.log("安装 \(kind.displayName) \(loaderVersion)（基于 \(game)）")
        let versionId: String
        if kind.usesInstaller {
            versionId = try await installWithInstaller(kind: kind, game: game,
                                                       loaderVersion: loaderVersion, progress: progress)
        } else {
            versionId = try await installProfile(kind: kind, game: game,
                                                 loaderVersion: loaderVersion, progress: progress)
        }
        AppState.shared.log("加载器安装完成：\(versionId)")
        progress(1, "\(kind.displayName) 安装完成")
        return versionId
    }

    /// Fabric / Quilt：直接下载 profile.json 落盘，再由 LaunchEngine 补齐继承的原版依赖。
    private static func installProfile(kind: LoaderKind,
                                       game: String,
                                       loaderVersion: String,
                                       progress: @escaping (Double, String?) -> Void) async throws -> String {
        let base = kind == .fabric
            ? "https://meta.fabricmc.net/v2/versions/loader"
            : "https://meta.quiltmc.org/v3/versions/loader"
        guard let url = URL(string: "\(base)/\(game)/\(loaderVersion)/profile/json") else {
            throw LoaderInstallError.profileFailed("加载器地址无效")
        }

        progress(0, "获取 \(kind.displayName) 配置")
        let data: Data
        do {
            data = try await requestData(url)
        } catch {
            throw LoaderInstallError.profileFailed("\(kind.displayName) 未提供 \(game) 的配置（\(error.localizedDescription)）")
        }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let profileId = json["id"] as? String, !profileId.isEmpty else {
            throw LoaderInstallError.profileFailed("\(kind.displayName) 返回的配置缺少 id 字段")
        }

        let fm = FileManager.default
        let dir = Paths.versionsDir.appendingPathComponent(profileId, isDirectory: true)
        do {
            try fm.createDirectory(at: dir, withIntermediateDirectories: true)
            try data.write(to: dir.appendingPathComponent("\(profileId).json"), options: .atomic)
        } catch {
            throw LoaderInstallError.profileFailed("写入版本文件失败：\(error.localizedDescription)")
        }
        AppState.shared.log("已写入加载器版本文件：\(profileId)")
        progress(0.3, "补齐依赖")

        try await LaunchEngine.prepare(versionId: profileId) { p, message in
            progress(0.3 + min(max(p, 0), 1) * 0.7, message)
        }
        return profileId
    }

    /// NeoForge / Forge：下载安装器 jar，跑 `--installClient`，再定位生成的版本目录。
    private static func installWithInstaller(kind: LoaderKind,
                                             game: String,
                                             loaderVersion: String,
                                             progress: @escaping (Double, String?) -> Void) async throws -> String {
        let fm = FileManager.default
        let before = Set(Paths.contentsOf(Paths.versionsDir))

        let primary: URL
        let fallback: URL?
        if kind == .neoforge {
            let v = loaderVersion
            guard let p = URL(string: "\(neoForgeMirrorBase)/net/neoforged/neoforge/\(v)/neoforge-\(v)-installer.jar"),
                  let f = URL(string: "\(neoForgeMavenBase)/net/neoforged/neoforge/\(v)/neoforge-\(v)-installer.jar") else {
                throw LoaderInstallError.versionNotFound("NeoForge \(loaderVersion) 安装器地址无效")
            }
            primary = p
            fallback = f
        } else {
            let full = "\(game)-\(loaderVersion)"
            guard let p = URL(string: "\(forgeMavenBase)/net/minecraftforge/forge/\(full)/forge-\(full)-installer.jar") else {
                throw LoaderInstallError.versionNotFound("Forge \(full) 安装器地址无效")
            }
            primary = p
            fallback = nil
        }

        // 安装器放临时目录，装完即删，避免污染数据目录。
        let tmpDir = fm.temporaryDirectory
            .appendingPathComponent("glassmc-\(kind.rawValue)-\(UUID().uuidString.prefix(8))", isDirectory: true)
        do {
            try fm.createDirectory(at: tmpDir, withIntermediateDirectories: true)
        } catch {
            throw LoaderInstallError.installerExit(-1, "无法创建临时目录：\(error.localizedDescription)")
        }
        defer { try? fm.removeItem(at: tmpDir) }

        let jar = tmpDir.appendingPathComponent("installer.jar")
        progress(0.02, "下载 \(kind.displayName) 安装器")
        do {
            try await DownloadManager.download(primary, to: jar) { p, _ in
                progress(0.02 + min(max(p, 0), 1) * 0.28, "下载 \(kind.displayName) 安装器")
            }
        } catch {
            guard let fallback else { throw error }
            AppState.shared.log("镜像下载 \(kind.displayName) 安装器失败，回退官方 maven：\(error.localizedDescription)")
            try await DownloadManager.download(fallback, to: jar) { p, _ in
                progress(0.02 + min(max(p, 0), 1) * 0.28, "下载 \(kind.displayName) 安装器")
            }
        }

        // installer 需要 Java 21（NeoForge/新版 Forge 的最低要求）。
        let java: String
        if let best = JavaManager.bestMatch(required: 21), best.isUsable {
            java = best.executable
        } else {
            java = "/usr/bin/java"
            AppState.shared.log("未检测到 Java 21，尝试使用系统 /usr/bin/java 运行安装器")
        }

        AppState.shared.log("运行 \(kind.displayName) 安装器：\(java) -jar \(jar.lastPathComponent) --installClient \(Paths.gameDir.path)")
        progress(0.32, "运行 \(kind.displayName) 安装器（可能需要数分钟）")
        let output = try await runInstaller(java: java, jar: jar)
        AppState.shared.log("\(kind.displayName) 安装器退出成功")

        // 扫描新增的版本目录（排除原版自身），取最后修改的那个。
        let after = Set(Paths.contentsOf(Paths.versionsDir))
        var candidates = after.subtracting(before).filter {
            $0 != game && Paths.isDirectory(Paths.versionsDir.appendingPathComponent($0).path)
        }
        if candidates.isEmpty {
            candidates = after.filter {
                $0 != game && $0.contains(loaderVersion)
                    && Paths.isDirectory(Paths.versionsDir.appendingPathComponent($0).path)
            }
        }
        guard let installedId = candidates.max(by: { modificationDate($0) < modificationDate($1) }) else {
            throw LoaderInstallError.versionNotFound(
                "安装器已运行完毕，但 versions 目录里没有找到新的版本目录。安装器输出：\n\(tailText(output))")
        }

        AppState.shared.log("安装后的版本 id：\(installedId)")
        progress(0.6, "补齐依赖")
        try await LaunchEngine.prepare(versionId: installedId) { p, message in
            progress(0.6 + min(max(p, 0), 1) * 0.4, message)
        }
        return installedId
    }

    // MARK: - 进程执行

    /// 运行 installer jar，捕获输出（含 stderr），超时 300 秒。
    /// - Returns: 完整输出（用于日志与错误提示）。
    private static func runInstaller(java: String, jar: URL,
                                     timeout: TimeInterval = 300) async throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: java)
        process.arguments = ["-jar", jar.path, "--installClient", Paths.gameDir.path]
        process.currentDirectoryURL = Paths.gameDir
        var env = ProcessInfo.processInfo.environment
        env["JAVA_HOME"] = ((java as NSString).deletingLastPathComponent as NSString).deletingLastPathComponent
        process.environment = env

        // stdout / stderr 共用一个 Pipe，并由 readabilityHandler 持续抽干，避免管道写满导致卡死。
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        let collector = OutputCollector()
        pipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            collector.append(String(decoding: data, as: UTF8.self))
        }

        do {
            try process.run()
        } catch {
            pipe.fileHandleForReading.readabilityHandler = nil
            throw LoaderInstallError.installerExit(-1, "无法启动 Java 安装器：\(error.localizedDescription)")
        }

        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning {
            if Date() >= deadline {
                process.terminate()
                pipe.fileHandleForReading.readabilityHandler = nil
                throw LoaderInstallError.installerExit(-1, "安装器运行超过 \(Int(timeout)) 秒，已终止。\n\(collector.tail)")
            }
            try? await Task.sleep(nanoseconds: 200_000_000)
        }
        process.waitUntilExit()
        pipe.fileHandleForReading.readabilityHandler = nil

        let text = collector.text
        for line in text.split(separator: "\n").suffix(40) {
            AppState.shared.log("[Loader] " + String(line.prefix(300)))
        }

        guard process.terminationStatus == 0 else {
            throw LoaderInstallError.installerExit(process.terminationStatus, collector.tail)
        }
        return text
    }

    // MARK: - 工具

    /// 线程安全的输出累积器（readabilityHandler 在后台队列回调）。
    private final class OutputCollector {
        private let lock = NSLock()
        private var buffer = ""

        func append(_ text: String) {
            lock.lock()
            defer { lock.unlock() }
            buffer += text
            // 只保留尾部，防止超长输出吃光内存
            if buffer.count > 20_000 { buffer = String(buffer.suffix(20_000)) }
        }

        var text: String {
            lock.lock()
            defer { lock.unlock() }
            return buffer
        }

        /// 末尾一段输出，用于错误提示。
        var tail: String { LoaderInstaller.tailText(text) }
    }

    private static func tailText(_ text: String) -> String {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return t.count > 800 ? String(t.suffix(800)) : t
    }

    private static func modificationDate(_ versionId: String) -> Date {
        let path = Paths.versionsDir.appendingPathComponent(versionId).path
        let attrs = try? FileManager.default.attributesOfItem(atPath: path)
        return (attrs?[.modificationDate] as? Date) ?? .distantPast
    }

    /// 从 maven-metadata.xml 抽出所有 `<version>…</version>`。
    private static func xmlVersions(_ xml: String) -> [String] {
        guard let regex = try? NSRegularExpression(pattern: "<version>([^<]+)</version>") else { return [] }
        let ns = xml as NSString
        let matches = regex.matches(in: xml, range: NSRange(location: 0, length: ns.length))
        var seen = Set<String>()
        var result: [String] = []
        for match in matches where match.numberOfRanges > 1 {
            let v = ns.substring(with: match.range(at: 1)).trimmingCharacters(in: .whitespacesAndNewlines)
            guard !v.isEmpty, seen.insert(v).inserted else { continue }
            result.append(v)
        }
        return result
    }

    /// 版本号比较：逐段按数字比较，全等时用字符串兜底（用于 `-beta` 之类的后缀）。
    static func isNewer(_ lhs: String, than rhs: String) -> Bool {
        let a = numericParts(lhs)
        let b = numericParts(rhs)
        for i in 0..<max(a.count, b.count) {
            let l = i < a.count ? a[i] : 0
            let r = i < b.count ? b[i] : 0
            if l != r { return l > r }
        }
        return lhs > rhs
    }

    private static func numericParts(_ value: String) -> [Int] {
        var parts: [Int] = []
        var digits = ""
        for ch in value {
            if ch.isNumber {
                digits.append(ch)
            } else if !digits.isEmpty {
                parts.append(Int(digits) ?? 0)
                digits = ""
            }
        }
        if !digits.isEmpty { parts.append(Int(digits) ?? 0) }
        return parts
    }

    private static func requestData(_ url: URL, timeout: TimeInterval = 30) async throws -> Data {
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.setValue("GlassMC/1.0 (macOS; Minecraft Launcher)", forHTTPHeaderField: "User-Agent")
        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            throw LoaderInstallError.metadataFailed("HTTP \(http.statusCode)：\(url.absoluteString)")
        }
        guard !data.isEmpty else {
            throw LoaderInstallError.metadataFailed("响应为空：\(url.absoluteString)")
        }
        return data
    }

    private static func requestText(_ url: URL, timeout: TimeInterval = 30) async throws -> String {
        let data = try await requestData(url, timeout: timeout)
        guard let text = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .isoLatin1) else {
            throw LoaderInstallError.metadataFailed("无法将响应解码为文本：\(url.absoluteString)")
        }
        return text
    }
}
