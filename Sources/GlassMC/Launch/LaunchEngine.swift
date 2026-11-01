import Foundation
import Darwin

/// 启动失败的具体原因（给出可读中文提示）。
enum LaunchError: LocalizedError {
    case noJava(required: Int)
    case javaUnavailable(path: String)
    case javaTooOld(required: Int, found: Int)
    case clientJarMissing(String)
    case missingLibraries([String])
    case badVersion(String)

    var errorDescription: String? {
        switch self {
        case .noJava(let required):
            return "未找到可用的 Java 运行环境（该版本需要 Java \(required)）。请到「设置 → 运行环境」下载安装，或手动安装 JDK 后重新打开本页。"
        case .javaUnavailable(let p):
            return "所选 Java 不可用（缺少 bin/java）：\(p)。请到「设置 → 运行环境」重新安装或改选其它 Java。"
        case .javaTooOld(let req, let found):
            return "该版本需要 Java \(req) 或更高，当前选择的是 Java \(found)。请到「设置」安装 Java \(req)。"
        case .clientJarMissing(let id):
            return "客户端主程序缺失：\(id).jar。请检查网络后重试，或重新导入该版本。"
        case .missingLibraries(let names):
            return "缺少依赖文件：\(names.prefix(3).joined(separator: "、"))\(names.count > 3 ? " 等" : "")。请重试启动以补齐下载。"
        case .badVersion(let id):
            return "版本文件损坏或缺少 JSON：\(id)"
        }
    }
}

/// 启动引擎：解析版本 JSON（含 inheritsFrom 继承链）、下载依赖（库/原生库/资源），
/// 构建 JVM 启动命令并运行游戏。
///
/// 关键点（都是真实踩过的坑）：
/// - 继承链必须**深合并**：`libraries` 取并集、`arguments` 拼接，否则子版本会覆盖掉父版本的
///   133 个库和 `-cp ${classpath}` 等关键参数（HMCL 的整合包实例就是这种结构）。
/// - 客户端 jar 的名字来自 `jar` 字段（如 `1.21.1-NeoForge`），且在**父版本目录**里。
/// - 启用版本隔离的实例（目录里有 `.hmcl` / `mods` / `config` / `saves`）要把 `--gameDir`
///   指向版本目录，否则会加载错误的 mod 与存档。
final class LaunchEngine {

    // MARK: - 基础环境

    /// 当前 CPU 架构（arm64 / x86_64），用于 libraries 的 rules 过滤。
    static var cpuArch: String {
        var uts = utsname()
        uname(&uts)
        return withUnsafeBytes(of: uts.machine) { raw -> String in
            guard let base = raw.bindMemory(to: CChar.self).baseAddress else { return "arm64" }
            return String(cString: base)
        }
    }

    /// 版本是否启用「版本隔离」（游戏数据放在 versions/<id>/ 里）。
    static func isIsolatedInstance(_ versionId: String) -> Bool {
        let inst = Paths.versionsDir.appendingPathComponent(versionId, isDirectory: true)
        let markers = [".hmcl", "mods", "config", "saves", "options.txt", "resourcepacks", "shaderpacks"]
        return markers.contains { FileManager.default.fileExists(atPath: inst.appendingPathComponent($0).path) }
    }

    /// 该版本实际使用的游戏目录。
    static func gameDirectory(versionId: String) -> URL {
        isIsolatedInstance(versionId)
            ? Paths.versionsDir.appendingPathComponent(versionId, isDirectory: true)
            : Paths.gameDir
    }

    /// 读取 HMCL 实例级设置（memory / 自定义 Java 路径），用于与实例原配置对齐。
    static func instanceSettings(_ versionId: String) -> [String: Any] {
        let p = Paths.versionsDir
            .appendingPathComponent("\(versionId)/.hmcl/config/instance-game-settings.json")
        guard let data = try? Data(contentsOf: p),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return [:] }
        return json
    }

    /// 实例要求的最大内存（MB），来自 HMCL 的 maxMemory。
    static func instanceMaxMemoryMB(_ versionId: String) -> Int? {
        if let mb = instanceSettings(versionId)["maxMemory"] as? Int, mb > 0 { return mb }
        if let mb = instanceSettings(versionId)["maxMemory"] as? Double, mb > 0 { return Int(mb) }
        return nil
    }

    /// 实例指定的 Java 可执行文件路径（HMCL customJavaPath）。
    static func instanceJavaExecutable(_ versionId: String) -> String? {
        guard let p = instanceSettings(versionId)["customJavaPath"] as? String, !p.isEmpty,
              FileManager.default.isExecutableFile(atPath: p) else { return nil }
        return p
    }

    // MARK: - 版本 JSON 解析（含 inheritsFrom 继承）

    /// 读取版本 JSON：优先本地，否则从镜像下载并缓存。
    static func localOrRemoteVersionJSON(_ id: String) async throws -> [String: Any] {
        let local = Paths.versionsDir.appendingPathComponent(id).appendingPathComponent("\(id).json")
        if let data = try? Data(contentsOf: local),
           let j = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            return j
        }
        let url = DownloadSource().versionJSONURL(id)
        let (data, resp) = try await URLSession.shared.data(from: url)
        if let http = resp as? HTTPURLResponse, http.statusCode >= 400 {
            throw LaunchError.badVersion(id)
        }
        try? FileManager.default.createDirectory(at: local.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard let j = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw LaunchError.badVersion(id)
        }
        try? data.write(to: local)
        return j
    }

    /// 子版本 → 父版本的完整链条（child first）。
    static func versionChain(_ versionId: String) async throws -> [[String: Any]] {
        var chain: [[String: Any]] = []
        var current = versionId
        var seen = Set<String>()
        for _ in 0..<16 {
            guard !seen.contains(current) else { break }
            seen.insert(current)
            let json = try await localOrRemoteVersionJSON(current)
            chain.append(json)
            guard let parent = json["inheritsFrom"] as? String, !parent.isEmpty else { break }
            current = parent
        }
        return chain
    }

    /// 沿 inheritsFrom 深合并出完整版本定义。
    static func mergedVersion(_ versionId: String) async throws -> [String: Any] {
        merge(chain: try await versionChain(versionId))
    }

    /// 库的唯一标识：group:artifact[:classifier]（**不含版本**）。
    /// 子版本里同 group:artifact 的库应当替换父版本的旧版本，而不是并存——
    /// 否则会出现 asm 9.8（模块路径）与 9.10.1（classpath）同时存在，
    /// Forge/NeoForge 直接抛 "Module ... was already on the JVMs module path"。
    static func libraryKey(_ lib: [String: Any]) -> String {
        guard let name = lib["name"] as? String else { return UUID().uuidString }
        let parts = name.split(separator: ":", omittingEmptySubsequences: false).map(String.init)
        if parts.count >= 4 { return "\(parts[0]):\(parts[1]):\(parts[3])" }
        if parts.count >= 3 { return "\(parts[0]):\(parts[1])" }
        return name
    }

    /// 深合并：libraries 按 group:artifact 取并集（子版本覆盖父版本），arguments 的 jvm/game 依次拼接。
    static func merge(chain: [[String: Any]]) -> [String: Any] {
        var merged: [String: Any] = [:]
        var libraries: [[String: Any]] = []
        var libraryIndex: [String: Int] = [:]
        var jvmArgs: [Any] = []
        var gameArgs: [Any] = []
        var hasArguments = false

        for dict in chain.reversed() {          // 父 → 子
            for (key, value) in dict {
                switch key {
                case "libraries":
                    for lib in (value as? [[String: Any]] ?? []) {
                        let name = libraryKey(lib)
                        if let i = libraryIndex[name] {
                            libraries[i] = lib
                        } else {
                            libraryIndex[name] = libraries.count
                            libraries.append(lib)
                        }
                    }
                case "arguments":
                    hasArguments = true
                    let a = value as? [String: Any] ?? [:]
                    jvmArgs.append(contentsOf: (a["jvm"] as? [Any]) ?? [])
                    gameArgs.append(contentsOf: (a["game"] as? [Any]) ?? [])
                default:
                    merged[key] = value
                }
            }
        }
        if !libraries.isEmpty { merged["libraries"] = libraries }
        if hasArguments { merged["arguments"] = ["jvm": jvmArgs, "game": gameArgs] }
        return merged
    }

    // MARK: - 客户端 jar 定位

    /// 客户端 jar 所在版本 id：优先 `jar` 字段，其次链上第一个带 `downloads.client` 的版本。
    static func clientJarID(chain: [[String: Any]], merged: [String: Any], versionId: String) -> String {
        if let jar = merged["jar"] as? String, !jar.isEmpty { return jar }
        for dict in chain {
            if let d = dict["downloads"] as? [String: Any], d["client"] != nil,
               let id = dict["id"] as? String, !id.isEmpty {
                return id
            }
        }
        return versionId
    }

    static func clientJarPath(_ id: String) -> URL {
        Paths.versionsDir.appendingPathComponent("\(id)/\(id).jar")
    }

    /// 找已存在的客户端 jar（先按 jar 字段，再退回本版本目录）。
    static func existingClientJar(jarID: String, versionId: String) -> URL? {
        for id in [jarID, versionId] {
            let p = clientJarPath(id)
            if FileManager.default.fileExists(atPath: p.path) { return p }
        }
        return nil
    }

    // MARK: - 依赖准备

    /// 准备运行所需的一切：客户端 jar、库文件、原生库、资源索引与对象。
    static func prepare(versionId: String,
                        progress: ((Double, String?) -> Void)? = nil) async throws {
        let chain = try await versionChain(versionId)
        let merged = merge(chain: chain)
        let libs = merged["libraries"] as? [[String: Any]] ?? []
        let source = DownloadSource()
        let jarID = clientJarID(chain: chain, merged: merged, versionId: versionId)

        // 0) 客户端主程序 jar
        if existingClientJar(jarID: jarID, versionId: versionId) == nil {
            let dest = clientJarPath(jarID)
            var official: URL? = nil
            if let d = merged["downloads"] as? [String: Any],
               let c = d["client"] as? [String: Any],
               let u = c["url"] as? String {
                official = URL(string: u)
            }
            AppState.shared.log("下载客户端主程序：\(jarID).jar（HMCL 镜像优先）")
            try await downloadWithFallback(source.clientJarURL(jarID), official, to: dest, progress: progress)
        }

        // 1) 普通库（按 rules 过滤，只下当前系统需要的）
        let wanted = libs.filter { rulesAllow($0["rules"] as? [[String: Any]]) }
        let total = max(wanted.count, 1)
        var done = 0
        var failures: [String] = []

        for lib in wanted {
            let name = lib["name"] as? String ?? ""
            if let downloads = lib["downloads"] as? [String: Any],
               let artifact = downloads["artifact"] as? [String: Any],
               let path = artifact["path"] as? String {
                let dest = Paths.librariesDir.appendingPathComponent(path)
                if !FileManager.default.fileExists(atPath: dest.path) {
                    let official = (artifact["url"] as? String).flatMap { URL(string: $0) }
                    do {
                        try await downloadWithFallback(source.libraryURL(path), official, to: dest)
                    } catch {
                        failures.append(dest.lastPathComponent)
                    }
                }
            } else if !name.isEmpty {
                let rel = mavenPath(name)
                let dest = Paths.librariesDir.appendingPathComponent(rel)
                if !FileManager.default.fileExists(atPath: dest.path) {
                    // 部分版本（Fabric/Quilt）用 libraries[].url 指定自有 Maven 基址
                    var official: URL? = nil
                    if let base = lib["url"] as? String,
                       let u = URL(string: base.hasSuffix("/") ? base + rel : base + "/" + rel) {
                        official = u
                    }
                    do {
                        try await downloadWithFallback(source.libraryURL(rel), official, to: dest)
                    } catch {
                        failures.append(dest.lastPathComponent)
                    }
                }
            }
            done += 1
            progress?(Double(done) / Double(total) * 0.35, "依赖 \(done)/\(wanted.count)")
        }

        if !failures.isEmpty {
            AppState.shared.log("有 \(failures.count) 个依赖下载失败：\(failures.prefix(5).joined(separator: "、"))")
            throw LaunchError.missingLibraries(failures)
        }

        // 2) 资源索引
        let assetIndexId = (merged["assets"] as? String)
            ?? ((merged["assetIndex"] as? [String: Any])?["id"] as? String)
            ?? "legacy"
        let indexDest = Paths.assetsDir.appendingPathComponent("indexes/\(assetIndexId).json")
        if !FileManager.default.fileExists(atPath: indexDest.path) {
            var official: URL? = nil
            if let ai = merged["assetIndex"] as? [String: Any], let u = ai["url"] as? String {
                official = URL(string: u)
            }
            try await downloadWithFallback(source.assetIndexURL(assetIndexId), official, to: indexDest)
        }

        // 3) 资源对象（并发、容错；缺个别纹理不会导致启动失败）
        try await prepareAssets(indexId: assetIndexId, indexPath: indexDest, source: source, progress: progress)

        AppState.shared.log("依赖准备完成：\(versionId)")
        progress?(1, "依赖就绪")
    }

    private static func prepareAssets(indexId: String, indexPath: URL, source: DownloadSource,
                                      progress: ((Double, String?) -> Void)?) async throws {
        guard let data = try? Data(contentsOf: indexPath),
              let idx = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let objects = idx["objects"] as? [String: [String: Any]] else { return }

        var jobs: [String] = []
        jobs.reserveCapacity(objects.count)
        for (_, obj) in objects {
            guard let hash = obj["hash"] as? String else { continue }
            let dest = Paths.assetsDir.appendingPathComponent("objects/\(String(hash.prefix(2)))/\(hash)")
            if !FileManager.default.fileExists(atPath: dest.path) { jobs.append(hash) }
        }
        if jobs.isEmpty {
            progress?(0.99, "资源已就绪")
            return
        }
        AppState.shared.log("下载资源对象 \(jobs.count) 个（首次启动较慢，请稍候）")

        let total = jobs.count
        var completed = 0
        var failed = 0
        await withTaskGroup(of: Bool.self) { group in
            var next = 0
            let width = min(8, jobs.count)
            for _ in 0..<width {
                let hash = jobs[next]; next += 1
                group.addTask { await fetchAsset(hash: hash, source: source) }
            }
            while let ok = await group.next() {
                if ok { completed += 1 } else { failed += 1 }
                let finished = completed + failed
                progress?(0.35 + Double(finished) / Double(total) * 0.6, "资源 \(finished)/\(total)")
                if next < jobs.count {
                    let hash = jobs[next]; next += 1
                    group.addTask { await fetchAsset(hash: hash, source: source) }
                }
            }
        }
        if failed > 0 {
            AppState.shared.log("有 \(failed) 个资源文件下载失败（通常不影响进入游戏）")
        }
    }

    private static func fetchAsset(hash: String, source: DownloadSource) async -> Bool {
        let dest = Paths.assetsDir.appendingPathComponent("objects/\(String(hash.prefix(2)))/\(hash)")
        if FileManager.default.fileExists(atPath: dest.path) { return true }
        // 镜像优先，失败回退官方资源站
        do {
            try await DownloadManager.download(source.assetObjectURL(hash: hash), to: dest, retries: 0)
            return true
        } catch {
            do {
                try await DownloadManager.download(DownloadSource.officialAssetObjectURL(hash: hash), to: dest)
                return true
            } catch {
                return false
            }
        }
    }

    /// 镜像优先、官方回退的下载：primary（HMCL 镜像）失败时尝试 fallback（官方源）。
    private static func downloadWithFallback(_ primary: URL, _ fallback: URL?, to dest: URL,
                                             progress: ((Double, String?) -> Void)? = nil) async throws {
        do {
            try await DownloadManager.download(primary, to: dest, progress: progress)
        } catch {
            guard let fallback else { throw error }
            AppState.shared.log("镜像下载失败，回退官方源：\(dest.lastPathComponent)")
            try await DownloadManager.download(fallback, to: dest, progress: progress)
        }
    }

    // MARK: - 命令构建

    /// 根据合并后的版本定义与账号、Java 构建启动命令。
    static func buildCommand(versionId: String,
                             account: MCAccount,
                             java: JavaInstall) async throws -> [String] {
        let chain = try await versionChain(versionId)
        let merged = merge(chain: chain)
        let jarID = clientJarID(chain: chain, merged: merged, versionId: versionId)

        // Java 可用性 & 版本要求
        guard FileManager.default.isExecutableFile(atPath: java.executable) else {
            throw LaunchError.javaUnavailable(path: java.homePath)
        }
        if let jv = merged["javaVersion"] as? [String: Any],
           let major = jv["majorVersion"] as? Int, major > java.version {
            throw LaunchError.javaTooOld(required: major, found: java.version)
        }

        var classpath = resolveClasspath(merged, jarID: jarID, versionId: versionId)
        guard !classpath.isEmpty else { throw LaunchError.clientJarMissing(jarID) }
        // 客户端 jar 必须在首位：主类（含 BootstrapLauncher / KnotClient）从它加载。
        if let jar = existingClientJar(jarID: jarID, versionId: versionId) {
            classpath.removeAll { $0 == jar.path }
            classpath.insert(jar.path, at: 0)
        } else {
            throw LaunchError.clientJarMissing(jarID)
        }

        let workDir = gameDirectory(versionId: versionId)
        try? FileManager.default.createDirectory(at: workDir, withIntermediateDirectories: true)

        let nativesDir = Paths.nativesDir.appendingPathComponent(versionId, isDirectory: true)
        try? FileManager.default.createDirectory(at: nativesDir, withIntermediateDirectories: true)
        extractNatives(merged, nativesDir: nativesDir)

        let assetsId = (merged["assets"] as? String)
            ?? ((merged["assetIndex"] as? [String: Any])?["id"] as? String)
            ?? "legacy"
        let versionType = (merged["type"] as? String) ?? "release"
        let mainClass = (merged["mainClass"] as? String) ?? "net.minecraft.client.main.Main"

        // 内存：取「全局设置」与「实例自带 HMCL 设置」的较大值，大型整合包才不会 OOM。
        let settingMB = Int(SettingsStore.shared.memoryGB.rounded()) * 1024
        let maxMB = max(settingMB, instanceMaxMemoryMB(versionId) ?? 0)

        var cmd: [String] = [java.executable]
        let jvm = jvmArguments(merged, nativesDir: nativesDir.path, classpath: classpath,
                               versionId: versionId, jarID: jarID, gameDir: workDir.path,
                               assetsId: assetsId, versionType: versionType, maxMemoryMB: maxMB)
        cmd.append(contentsOf: jvm)
        // 外置登录（皮肤站 / Yggdrasil）：必须注入 authlib-injector，
        // 否则游戏会去问 Mojang 要皮肤与认证，结果就是默认皮肤、进不了站点正版服。
        if account.type == "yggdrasil", let server = yggdrasilServer(for: account) {
            if let jar = await AuthLibInjector.ensure() {
                cmd.append("-javaagent:\(jar.path)=\(server.normalizedBase)")
            } else {
                AppState.shared.log("警告：未取得 authlib-injector，本次启动将不会同步站点皮肤。")
            }
        }
        // 版本 JSON 的 jvm 参数通常已含 `-cp ${classpath}`；没有时才补。
        if !jvm.contains("-cp") && !jvm.contains("-classpath") {
            cmd.append("-cp")
            cmd.append(classpath.joined(separator: ":"))
        }
        // 自定义 JVM 参数（设置页填写），放在主类之前，避免被当成游戏参数。
        let extra = SettingsStore.shared.extraJVMArgs
            .split(whereSeparator: { $0 == " " || $0 == "\n" || $0 == "\t" })
            .map(String.init)
        cmd.append(contentsOf: extra)
        cmd.append(mainClass)
        let gameArgs = gameArguments(merged, account: account, assetsId: assetsId,
                                     versionId: versionId, versionType: versionType,
                                     gameDir: workDir.path)
        cmd.append(contentsOf: applyWindowSize(gameArgs))
        return cmd
    }

    /// 覆盖/追加游戏窗口大小参数（`--width` / `--height`）。
    /// 设置为 0 表示不干预，沿用版本 JSON 自带的值。
    static func applyWindowSize(_ args: [String]) -> [String] {
        let w = SettingsStore.shared.windowWidth
        let h = SettingsStore.shared.windowHeight
        guard w > 0 || h > 0 else { return args }
        var out = args
        func set(_ flag: String, _ value: Int) {
            guard value > 0 else { return }
            if let i = out.firstIndex(of: flag), i + 1 < out.count {
                out[i + 1] = String(value)
            } else if !out.contains(flag) {
                out.append(flag)
                out.append(String(value))
            }
        }
        set("--width", w)
        set("--height", h)
        return out
    }

    /// 导出可双击运行的启动脚本（.command），方便排查与调试。
    /// 脚本内记录了完整的 java 命令行、工作目录与账号参数。
    @discardableResult
    static func exportLaunchScript(versionId: String,
                                   account: MCAccount? = nil,
                                   java: JavaInstall? = nil,
                                   to destination: URL) async throws -> URL {
        let acc = account
            ?? AppState.shared.activeAccount
            ?? MCAccount(type: "offline", username: "Player", uuid: nil, accessToken: nil,
                         source: "glassmc", needsReauth: false)
        let runtime: JavaInstall
        if let java {
            runtime = java
        } else if let best = JavaManager.bestMatch(required: 21) ?? JavaManager.installed().first {
            runtime = best
        } else {
            throw LaunchError.javaUnavailable(path: "未检测到可用 Java")
        }
        let cmd = try await buildCommand(versionId: versionId, account: acc, java: runtime)
        let workDir = gameDirectory(versionId: versionId)
        let java = cmd.first ?? "/usr/bin/java"
        let rest = Array(cmd.dropFirst())
        func quote(_ s: String) -> String {
            "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
        }
        var lines: [String] = [
            "#!/bin/zsh",
            "# GlassMC 生成的自定义启动脚本 · 版本：\(versionId)",
            "# 生成时间：\(ISO8601DateFormatter().string(from: Date()))",
            "cd \(quote(workDir.path)) || exit 1",
            "exec \(quote(java)) \\"
        ]
        for (i, a) in rest.enumerated() {
            let tail = i == rest.count - 1 ? "" : " \\"
            lines.append("  \(quote(a))\(tail)")
        }
        let text = lines.joined(separator: "\n") + "\n"
        try text.write(to: destination, atomically: true, encoding: .utf8)
        try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: destination.path)
        AppState.shared.log("已导出启动脚本：\(destination.lastPathComponent)")
        return destination
    }

    /// 外置登录账号对应的认证服务器（用于 authlib-injector 的 API 基址）。
    static func yggdrasilServer(for account: MCAccount) -> AuthServer? {
        guard let id = account.authServerId else { return nil }
        return SettingsStore.shared.authServers.first { $0.id.uuidString == id }
    }

    /// 拼 classpath：客户端 jar + 满足 rules 的库（去重、保持顺序）。
    static func resolveClasspath(_ merged: [String: Any], jarID: String, versionId: String) -> [String] {
        var paths: [String] = []
        var seen = Set<String>()
        func add(_ p: String) {
            guard !seen.contains(p) else { return }
            seen.insert(p)
            paths.append(p)
        }
        if let jar = existingClientJar(jarID: jarID, versionId: versionId) { add(jar.path) }

        let libs = merged["libraries"] as? [[String: Any]] ?? []
        for lib in libs {
            guard rulesAllow(lib["rules"] as? [[String: Any]]) else { continue }
            let name = lib["name"] as? String ?? ""
            if let downloads = lib["downloads"] as? [String: Any],
               let artifact = downloads["artifact"] as? [String: Any],
               let path = artifact["path"] as? String {
                add(Paths.librariesDir.appendingPathComponent(path).path)
            } else if !name.isEmpty {
                add(Paths.librariesDir.appendingPathComponent(mavenPath(name)).path)
            }
        }
        return paths
    }

    static func jvmArguments(_ merged: [String: Any], nativesDir: String,
                            classpath: [String], versionId: String, jarID: String,
                            gameDir: String, assetsId: String, versionType: String,
                            maxMemoryMB: Int) -> [String] {
        var args: [String] = []
        if let arguments = merged["arguments"] as? [String: Any],
           let jvm = arguments["jvm"] as? [Any] {
            args.append(contentsOf: flattenArguments(jvm, os: "osx"))
        } else {
            args.append(contentsOf: ["-Xss1M", "-Djava.library.path=\(nativesDir)",
                                     "-Dminecraft.launcher.brand=GlassMC", "-Dminecraft.launcher.version=1.0"])
        }
        let cp = classpath.joined(separator: ":")
        let jarFileName = "\(jarID).jar"
        var out = args.map { raw -> String in
            raw
                .replacingOccurrences(of: "${natives_directory}", with: nativesDir)
                .replacingOccurrences(of: "${classpath}", with: cp)
                .replacingOccurrences(of: "${classpath_separator}", with: ":")
                .replacingOccurrences(of: "${library_directory}", with: Paths.librariesDir.path)
                .replacingOccurrences(of: "${primary_jar_name}", with: jarFileName)
                .replacingOccurrences(of: "${primary_jar}", with: clientJarPath(jarID).path)
                .replacingOccurrences(of: "${launcher_name}", with: "GlassMC")
                .replacingOccurrences(of: "${launcher_version}", with: "1.0")
                .replacingOccurrences(of: "${game_directory}", with: gameDir)
                .replacingOccurrences(of: "${assets_root}", with: Paths.assetsDir.path)
                .replacingOccurrences(of: "${assets_index_name}", with: assetsId)
                .replacingOccurrences(of: "${version_type}", with: versionType)
                // 注意：JVM 参数里的 ${version_name}.jar 指“游戏主 jar 文件名”，
                // 对整合包实例（jar 名 != 版本名）必须用 jar 名，否则 ignoreList 失效。
                .replacingOccurrences(of: "${version_name}", with: jarID)
        }

        // 重复的单值选项（父子版本各写一份）保留最后一个，避免 -p / -cp 冲突。
        out = dedupeOptions(out)

        // ignoreList 必须包含真实的客户端 jar 文件名，否则 modlauncher 会把客户端 jar 当模组处理。
        if let i = out.firstIndex(where: { $0.hasPrefix("-DignoreList=") }) {
            var items = out[i].replacingOccurrences(of: "-DignoreList=", with: "")
                .split(separator: ",").map(String.init)
            for extra in [jarFileName, "\(versionId).jar"] where !items.contains(extra) {
                items.append(extra)
            }
            out[i] = "-DignoreList=" + items.joined(separator: ",")
        }

        if !out.contains(where: { $0.hasPrefix("-Xmx") }) {
            out.insert("-Xmx\(max(maxMemoryMB, 1024))M", at: 0)
        }
        // macOS 上 LWJGL/GLFW 必须在主线程启动
        if !out.contains("-XstartOnFirstThread") {
            out.insert("-XstartOnFirstThread", at: out.count)
        }
        return out
    }

    static func gameArguments(_ merged: [String: Any], account: MCAccount,
                              assetsId: String, versionId: String, versionType: String,
                              gameDir: String) -> [String] {
        var args: [String]
        if let arguments = merged["arguments"] as? [String: Any],
           let game = arguments["game"] as? [Any] {
            args = flattenArguments(game, os: "osx")
        } else if let legacy = merged["minecraftArguments"] as? String {
            args = legacy.split(separator: " ").map(String.init)
        } else {
            args = []
        }
        let uuid: String
        if let u = account.uuid, !u.isEmpty {
            uuid = u.replacingOccurrences(of: "-", with: "")
        } else if account.type == "offline" {
            uuid = MCAccount.offlineUUID(for: account.username).replacingOccurrences(of: "-", with: "")
        } else {
            uuid = "00000000000000000000000000000000"
        }
        // 外置登录（Yggdrasil / 皮肤站）由 authlib-injector 接管认证与皮肤，
        // userType 必须是 mojang（msa 会让客户端走微软的 XBL/XSTS 流程而失败）；
        // 微软正版账号则必须传 msa，否则拿不到档案与皮肤。
        let isMicrosoft = (account.type == "msa" || account.type == "microsoft")
        let userType: String = isMicrosoft ? "msa" : "mojang"
        let token: String = account.accessToken ?? (account.type == "offline" ? account.username : "0")
        let map: [String: String] = [
            "${auth_player_name}": account.username,
            "${version_name}": versionId,
            "${game_directory}": gameDir,
            "${assets_root}": Paths.assetsDir.path,
            "${assets_index_name}": assetsId,
            "${auth_uuid}": uuid,
            "${auth_access_token}": token,
            "${auth_session}": token,
            "${user_type}": userType,
            "${version_type}": versionType,
            "${launcher_name}": "GlassMC",
            "${launcher_version}": "1.0",
            "${primary_jar_name}": "\(versionId).jar",
            "${clientid}": "",
            "${auth_xuid}": "",
            "${user_properties}": "{}",
            "${quickPlayPath}": "",
            "${quickPlaySingleplayer}": "",
            "${quickPlayMultiplayer}": "",
            "${quickPlayRealms}": ""
        ]
        var out: [String] = []
        for a in args {
            var s = a
            for (k, v) in map where s.contains(k) { s = s.replacingOccurrences(of: k, with: v) }
            out.append(s)
        }
        // 极少数版本会留下未知占位符（如 quickPlay 系列），直接丢掉，避免游戏解析参数失败。
        out.removeAll { $0.contains("${") }
        // 父子版本各自都带一份 --fml.* / --launchTarget，重复会被 jopt-simple 判为非法；
        // 同名选项保留子版本（后出现的那份）。
        return dedupeGameArguments(out)
    }

    /// 无语义的单值开关（不跟参数值）。
    private static let gameFlagOptions: Set<String> = [
        "--demo", "--fullscreen", "--disableMultiplayer", "--disableChat"
    ]

    /// 游戏参数去重：`--key value` 形式同名只保留最后一个（子版本覆盖父版本）。
    static func dedupeGameArguments(_ args: [String]) -> [String] {
        var slots: [(key: String?, values: [String])] = []
        var i = 0
        while i < args.count {
            let a = args[i]
            if a.hasPrefix("--"), !gameFlagOptions.contains(a),
               i + 1 < args.count, !args[i + 1].hasPrefix("--") {
                slots.append((a, [a, args[i + 1]]))
                i += 2
                continue
            }
            slots.append((nil, [a]))
            i += 1
        }
        var keep = [Bool](repeating: true, count: slots.count)
        var lastIndex: [String: Int] = [:]
        for (idx, slot) in slots.enumerated() {
            guard let key = slot.key else { continue }
            if let prev = lastIndex[key] { keep[prev] = false }
            lastIndex[key] = idx
        }
        var out: [String] = []
        for (idx, slot) in slots.enumerated() where keep[idx] {
            out.append(contentsOf: slot.values)
        }
        return out
    }

    /// 将 arguments 数组中的字符串或 {rules,value} 对象展开（按当前 OS / 架构过滤）。
    static func flattenArguments(_ arr: [Any], os: String) -> [String] {
        var out: [String] = []
        for item in arr {
            if let str = item as? String { out.append(str); continue }
            guard let dict = item as? [String: Any] else { continue }
            guard rulesAllow(dict["rules"] as? [[String: Any]]) else { continue }
            guard let value = dict["value"] else { continue }
            if let v = value as? String { out.append(v) }
            else if let va = value as? [String] { out.append(contentsOf: va) }
        }
        return out
    }

    /// 标准 rules 语义：默认不允许；按顺序应用匹配到的规则，最后一条匹配规则决定结果。
    /// 带 `features` 的规则一律视为不匹配（本启动器不启用 demo / quickPlay）。
    static func rulesAllow(_ rules: [[String: Any]]?) -> Bool {
        guard let rules, !rules.isEmpty else { return true }
        var allowed = false
        var matched = false
        for rule in rules {
            if rule["features"] != nil { continue }
            var matches = true
            if let osRule = rule["os"] as? [String: Any] {
                if let name = (osRule["name"] as? String)?.lowercased() {
                    let ok = (name == "osx") || (name == "macos") || (name == "os x")
                    if !ok { matches = false }
                }
                if matches, let arch = (osRule["arch"] as? String)?.lowercased() {
                    let cur = cpuArch.lowercased()
                    let ok = (arch == cur)
                        || (arch == "x86" && cur == "x86_64")
                        || (arch == "arm64" && cur == "arm64")
                    if !ok { matches = false }
                }
            }
            if matches {
                matched = true
                allowed = ((rule["action"] as? String) ?? "allow") == "allow"
            }
        }
        return matched ? allowed : false
    }

    /// 去重「单值选项」（同一 key 出现多次时保留最后一个），例如父子版本各自写了一份 -p。
    private static func dedupeOptions(_ args: [String]) -> [String] {
        let pairKeys: Set<String> = ["-p", "--module-path", "-cp", "-classpath", "--add-modules"]
        var slots: [(key: String?, values: [String])] = []
        var i = 0
        while i < args.count {
            let a = args[i]
            if pairKeys.contains(a), i + 1 < args.count {
                slots.append((a, [a, args[i + 1]]))
                i += 2
                continue
            }
            if a.hasPrefix("-D"), let eq = a.firstIndex(of: "=") {
                slots.append((String(a[a.startIndex..<eq]), [a]))
                i += 1
                continue
            }
            if a == "-XstartOnFirstThread" {
                slots.append((a, [a]))
                i += 1
                continue
            }
            slots.append((nil, [a]))
            i += 1
        }
        var keep = [Bool](repeating: true, count: slots.count)
        var lastIndex: [String: Int] = [:]
        for (idx, slot) in slots.enumerated() {
            guard let key = slot.key else { continue }
            if let prev = lastIndex[key] { keep[prev] = false }
            lastIndex[key] = idx
        }
        var out: [String] = []
        for (idx, slot) in slots.enumerated() where keep[idx] {
            out.append(contentsOf: slot.values)
        }
        return out
    }

    /// 旧版（1.12-）原生库需要解压到 natives 目录。
    static func extractNatives(_ merged: [String: Any], nativesDir: URL) {
        let libs = merged["libraries"] as? [[String: Any]] ?? []
        let fm = FileManager.default
        for lib in libs {
            guard rulesAllow(lib["rules"] as? [[String: Any]]) else { continue }
            let name = lib["name"] as? String ?? ""
            let natives = (lib["natives"] as? [String: String]) ?? [:]
            guard let classifier = natives["osx"] ?? natives["macos"] else { continue }
            let jar = Paths.librariesDir.appendingPathComponent(mavenPath(name + ":\(classifier)"))
            guard fm.fileExists(atPath: jar.path) else { continue }
            // 已解压过就跳过，避免每次启动都 unzip。
            let marker = nativesDir.appendingPathComponent(".done-\(jar.lastPathComponent)")
            if fm.fileExists(atPath: marker.path) { continue }
            let proc = Process()
            proc.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
            proc.arguments = ["-oq", jar.path, "-d", nativesDir.path]
            try? proc.run()
            proc.waitUntilExit()
            if proc.terminationStatus == 0 { try? Data().write(to: marker) }
        }
    }

    // MARK: - 运行

    private(set) static var runningProcess: Process?

    /// 真正启动游戏进程，并把输出接进应用日志。
    static func launch(versionId: String, account: MCAccount, java: JavaInstall) async throws -> Process {
        let cmd = try await buildCommand(versionId: versionId, account: account, java: java)
        let workDir = gameDirectory(versionId: versionId)
        AppState.shared.log("启动 \(versionId)（Java \(java.version)）：\(cmd[0])")
        AppState.shared.log("游戏目录：\(workDir.path)")

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: cmd[0])
        proc.arguments = Array(cmd.dropFirst())
        proc.currentDirectoryURL = workDir
        var env = ProcessInfo.processInfo.environment
        env["JAVA_HOME"] = java.homePath
        proc.environment = env

        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = pipe
        pipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            let text = String(decoding: data, as: UTF8.self)
            for line in text.split(separator: "\n") where !line.trimmingCharacters(in: .whitespaces).isEmpty {
                AppState.shared.log("[MC] " + String(line.prefix(400)))
            }
        }
        proc.terminationHandler = { p in
            AppState.shared.log(p.terminationStatus == 0
                ? "游戏进程已退出（正常）"
                : "游戏异常退出，退出码 \(p.terminationStatus)。请查看上方日志定位原因。")
        }
        do {
            try proc.run()
        } catch {
            throw LaunchError.javaUnavailable(path: java.executable)
        }
        runningProcess = proc
        return proc
    }

    // MARK: - 工具

    /// 将 Maven 坐标 group:artifact:version[:classifier] 转为仓库路径。
    static func mavenPath(_ coordinate: String) -> String {
        let parts = coordinate.split(separator: ":", omittingEmptySubsequences: false).map(String.init)
        guard parts.count >= 3 else { return coordinate }
        let group = parts[0].replacingOccurrences(of: ".", with: "/")
        let artifact = parts[1]
        let version = parts[2]
        if parts.count >= 4 {
            let classifier = parts[3]
            return "\(group)/\(artifact)/\(version)/\(artifact)-\(version)-\(classifier).jar"
        }
        return "\(group)/\(artifact)/\(version)/\(artifact)-\(version).jar"
    }
}
