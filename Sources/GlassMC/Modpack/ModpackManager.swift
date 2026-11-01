import Foundation
import CommonCrypto

// MARK: - 数据模型

/// modrinth.index.json 里的一个文件条目。
struct ModpackFileEntry: Codable, Hashable {
    /// 相对实例目录的路径，如 mods/xxx.jar
    let path: String
    /// 校验值，常见键：sha1 / sha512
    let hashes: [String: String]
    /// 环境标记，键 client / server，取值 required / optional / unsupported
    let env: [String: String]?
    let downloads: [String]
    let fileSize: Int64?

    /// 客户端是否需要该文件（缺省按 required 处理）。
    var clientNeeded: Bool {
        (env?["client"] ?? "required").lowercased() != "unsupported"
    }
}

/// modrinth.index.json 整体结构。
struct ModpackIndex: Codable {
    var formatVersion: Int
    var game: String
    var versionId: String
    var name: String
    var summary: String?
    var files: [ModpackFileEntry]
    var dependencies: [String: String]
}

/// 已导入整合包的概要信息，同时写入实例目录的 .glassmc-modpack.json。
struct ImportedModpack: Identifiable, Codable {
    var id: String { instanceId }
    let instanceId: String
    let name: String
    let gameVersion: String
    let loader: String
    let loaderVersion: String
    let modCount: Int
}

/// 整合包相关错误。
enum ModpackError: LocalizedError {
    /// 不是有效的 .mrpack（找不到 modrinth.index.json）
    case notMrpack
    /// index 内容不合法
    case badIndex(String)
    case unzipFailed(String)
    case zipFailed(String)
    /// index 缺少 dependencies.minecraft
    case missingGameVersion

    var errorDescription: String? {
        switch self {
        case .notMrpack:
            return "这不是有效的 Modrinth 整合包（缺少 modrinth.index.json）"
        case .badIndex(let detail):
            return "整合包索引解析失败：\(detail)"
        case .unzipFailed(let detail):
            return "解压失败：\(detail)"
        case .zipFailed(let detail):
            return "打包失败：\(detail)"
        case .missingGameVersion:
            return "整合包索引里没有声明 Minecraft 版本"
        }
    }
}

// MARK: - 管理器

/// Modrinth 整合包（.mrpack）导入 / 导出。
/// zip 相关操作全部交给系统命令 /usr/bin/unzip 与 /usr/bin/zip，不自行解析压缩包格式。
enum ModpackManager {

    private static let fm = FileManager.default
    private static let unzipPath = "/usr/bin/unzip"
    private static let zipPath = "/usr/bin/zip"
    /// 实例目录里记录整合包来源的文件名
    static let manifestFileName = ".glassmc-modpack.json"
    /// 同时下载的文件数上限
    private static let maxConcurrentDownloads = 4

    // MARK: - 读取索引

    /// 只读取 .mrpack 里的 modrinth.index.json，不解压其余内容。
    static func inspect(_ url: URL) throws -> ModpackIndex {
        guard fm.fileExists(atPath: url.path) else {
            throw ModpackError.notMrpack
        }
        // -p 表示把指定条目输出到标准输出
        let (code, text) = try runProcess(unzipPath, ["-p", url.path, "modrinth.index.json"], cwd: nil)
        guard code == 0 else { throw ModpackError.notMrpack }

        // 去掉可能存在的 BOM 与首尾空白
        var raw = text
        if raw.hasPrefix("\u{FEFF}") { raw.removeFirst() }
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let data = trimmed.data(using: .utf8) else {
            throw ModpackError.notMrpack
        }
        do {
            return try JSONDecoder().decode(ModpackIndex.self, from: data)
        } catch {
            throw ModpackError.badIndex(error.localizedDescription)
        }
    }

    // MARK: - 导入

    /// 导入 .mrpack 到 versions/<instanceId>/（版本隔离），返回用于继续安装游戏/加载器的信息。
    /// 本方法只负责 overrides 与 files 的落盘，不安装 Minecraft 本体与加载器。
    static func importPack(_ url: URL,
                           targetName: String? = nil,
                           progress: @escaping (Double, String?) -> Void) async throws -> ImportedModpack {
        Paths.ensureDirectories()

        progress(0, "正在读取整合包索引…")
        let index = try inspect(url)

        guard let gameVersion = index.dependencies["minecraft"], !gameVersion.isEmpty else {
            throw ModpackError.missingGameVersion
        }
        let loader = detectLoader(index.dependencies)

        // 1) 实例目录：versions/<instanceId>，作为 --gameDir 实现版本隔离
        let rawName = targetName?.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseName: String = {
            if let rawName, !rawName.isEmpty { return rawName }
            return index.name
        }()
        let instanceId = uniqueInstanceId(from: baseName)
        let instanceDir = Paths.versionsDir.appendingPathComponent(instanceId, isDirectory: true)
        do {
            try fm.createDirectory(at: instanceDir, withIntermediateDirectories: true)
        } catch {
            throw ModpackError.unzipFailed("无法创建实例目录：\(error.localizedDescription)")
        }
        AppState.shared.log("整合包导入：\(index.name) → versions/\(instanceId)")

        // 2) 先铺 overrides / client-overrides
        progress(0.02, "正在解压 overrides…")
        try extractOverrides(url, subdir: "overrides", into: instanceDir)
        progress(0.05, "正在解压 client-overrides…")
        try extractOverrides(url, subdir: "client-overrides", into: instanceDir)

        // 3) 按 files[] 下载（只处理客户端需要的条目）
        let entries = index.files.filter { $0.clientNeeded }
        var failures: [String] = []

        if entries.isEmpty {
            progress(0.9, "没有需要下载的文件")
        } else {
            let tracker = ModpackDownloadTracker(total: entries.count)
            let workerCount = min(maxConcurrentDownloads, entries.count)
            progress(0.06, "开始下载 \(entries.count) 个文件（最高 \(workerCount) 并发）")

            await withTaskGroup(of: Void.self) { group in
                for _ in 0..<workerCount {
                    group.addTask {
                        while let i = await tracker.take() {
                            let entry = entries[i]
                            let reason = await downloadEntry(entry, into: instanceDir)
                            let state = await tracker.finish(reason.map { "\(entry.path)：\($0)" })
                            let frac = Double(state.done) / Double(max(state.total, 1))
                            progress(min(frac, 1), "文件 \(state.done)/\(state.total)：\(entry.path)")
                        }
                    }
                }
                await group.waitForAll()
            }
            failures = await tracker.allFailures()
        }

        // 4) 写入整合包来源记录，便于以后识别与导出
        let modFiles = entries.filter { entry in
            let p = entry.path.lowercased()
            return p.hasPrefix("mods/") && p.hasSuffix(".jar")
        }
        let record = ImportedModpack(instanceId: instanceId,
                                     name: index.name,
                                     gameVersion: gameVersion,
                                     loader: loader.name,
                                     loaderVersion: loader.version,
                                     modCount: modFiles.count)
        if let data = try? JSONEncoder().encode(record) {
            try? data.write(to: instanceDir.appendingPathComponent(manifestFileName), options: .atomic)
        }

        if failures.isEmpty {
            progress(1, "整合包导入完成：\(index.name)")
            AppState.shared.log("整合包导入完成：\(index.name)（\(loader.name) \(loader.version)，\(modFiles.count) 个模组）")
        } else {
            progress(1, "整合包导入完成，其中 \(failures.count) 个文件失败")
            AppState.shared.log("整合包导入：\(failures.count) 个文件下载失败（可重试导入补全）")
            for line in failures.prefix(5) {
                AppState.shared.log("  失败：\(line)")
            }
        }
        return record
    }

    // MARK: - 导出

    /// 把某个实例导出为 .mrpack。只打包 mods/*.jar、config/**、resourcepacks/*.zip，
    /// 存档/日志/崩溃报告不会进入整合包。
    static func exportPack(versionId: String,
                           to destination: URL,
                           name: String,
                           summary: String,
                           progress: @escaping (Double, String?) -> Void) async throws -> URL {
        Paths.ensureDirectories()

        let instanceDir = Paths.versionsDir.appendingPathComponent(versionId, isDirectory: true)
        let sourceDir = Paths.isDirectory(instanceDir.path) ? instanceDir : Paths.gameDir
        guard Paths.isDirectory(sourceDir.path) else {
            throw ModpackError.zipFailed("找不到实例目录：\(sourceDir.path)")
        }

        let tmp = try makeTempDirectory()
        defer { try? fm.removeItem(at: tmp) }
        let overridesDir = tmp.appendingPathComponent("overrides", isDirectory: true)
        try fm.createDirectory(at: overridesDir, withIntermediateDirectories: true)

        progress(0.05, "正在收集模组与配置…")

        // 1) mods/*.jar
        let modsTarget = overridesDir.appendingPathComponent("mods", isDirectory: true)
        let modNames = try copyFlat(from: sourceDir.appendingPathComponent("mods", isDirectory: true),
                                    to: modsTarget,
                                    extensions: ["jar"])

        // 2) config/** （整体递归，保留目录结构）
        let configSource = sourceDir.appendingPathComponent("config", isDirectory: true)
        if Paths.isDirectory(configSource.path) {
            try copyTree(from: configSource, to: overridesDir.appendingPathComponent("config", isDirectory: true))
        }

        // 3) resourcepacks/*.zip
        progress(0.25, "正在收集资源包…")
        let packsTarget = overridesDir.appendingPathComponent("resourcepacks", isDirectory: true)
        try copyFlat(from: sourceDir.appendingPathComponent("resourcepacks", isDirectory: true),
                     to: packsTarget,
                     extensions: ["zip"])

        // 4) 生成 modrinth.index.json：本地文件都放在 overrides 里，故 files 里的 download 地址留空
        progress(0.35, "正在生成索引…")
        var entries: [ModpackFileEntry] = []
        for modName in modNames {
            let fileURL = modsTarget.appendingPathComponent(modName)
            var hashes: [String: String] = [:]
            if let value = sha1(of: fileURL) { hashes["sha1"] = value }
            entries.append(ModpackFileEntry(path: "mods/\(modName)",
                                            hashes: hashes,
                                            env: ["client": "required", "server": "required"],
                                            downloads: [],
                                            fileSize: fileSize(of: fileURL)))
        }

        let index = ModpackIndex(formatVersion: 1,
                                 game: "minecraft",
                                 versionId: versionId,
                                 name: name,
                                 summary: summary,
                                 files: entries,
                                 dependencies: inferDependencies(versionId: versionId))

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        let indexData = try encoder.encode(index)
        try indexData.write(to: tmp.appendingPathComponent("modrinth.index.json"), options: .atomic)

        // 5) 打包：在临时目录内用相对路径，避免 zip 里带上多余的绝对路径层级
        progress(0.55, "正在打包…")
        let outputTmp = tmp.appendingPathComponent("pack.mrpack")
        let (code, out) = try runProcess(zipPath,
                                         ["-r", "-q", outputTmp.path, "modrinth.index.json", "overrides"],
                                         cwd: tmp)
        guard code == 0, fm.fileExists(atPath: outputTmp.path) else {
            throw ModpackError.zipFailed(out.isEmpty ? "zip 退出码 \(code)" : out)
        }

        // 6) 落到目标位置
        progress(0.9, "正在写入目标文件…")
        try fm.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        if fm.fileExists(atPath: destination.path) {
            try? fm.removeItem(at: destination)
        }
        do {
            try fm.moveItem(at: outputTmp, to: destination)
        } catch {
            throw ModpackError.zipFailed("无法写入 \(destination.lastPathComponent)：\(error.localizedDescription)")
        }

        progress(1, "整合包导出完成：\(destination.lastPathComponent)")
        AppState.shared.log("整合包导出：\(name) → \(destination.path)（\(modNames.count) 个模组）")
        return destination
    }

    // MARK: - 已导入列表

    /// 扫描 versions/*/.glassmc-modpack.json，返回所有导入过的整合包。
    static func listImportedPacks() -> [ImportedModpack] {
        var result: [ImportedModpack] = []
        for name in Paths.contentsOf(Paths.versionsDir) {
            let dir = Paths.versionsDir.appendingPathComponent(name, isDirectory: true)
            guard Paths.isDirectory(dir.path) else { continue }
            let file = dir.appendingPathComponent(manifestFileName)
            guard let data = try? Data(contentsOf: file),
                  let pack = try? JSONDecoder().decode(ImportedModpack.self, from: data) else { continue }
            result.append(pack)
        }
        return result.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    // MARK: - 进程调用

    /// 运行一个可执行文件并读取全部输出（stdout+stderr 合并）。
    /// 参数直接以数组传递，不经过 shell，因此中文与空格路径无需转义。
    static func runProcess(_ launchPath: String, _ args: [String], cwd: URL?) throws -> (Int32, String) {
        let (code, data) = try captureProcess(launchPath, args, cwd: cwd)
        return (code, String(decoding: data, as: UTF8.self))
    }

    private static func captureProcess(_ launchPath: String,
                                       _ args: [String],
                                       cwd: URL?) throws -> (Int32, Data) {
        guard fm.isExecutableFile(atPath: launchPath) else {
            throw ModpackError.unzipFailed("找不到系统命令：\(launchPath)")
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: launchPath)
        process.arguments = args
        if let cwd { process.currentDirectoryURL = cwd }
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        process.standardInput = FileHandle.nullDevice

        do {
            try process.run()
        } catch {
            throw ModpackError.unzipFailed("无法启动 \(launchPath)：\(error.localizedDescription)")
        }
        // 关键：必须先读到 EOF 再 waitUntilExit，否则输出量大时管道缓冲区被写满会让子进程永久阻塞。
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus, data)
    }

    // MARK: - 私有：overrides

    /// 解压 <subdir>/ 到实例目录；包内没有该目录时静默跳过。
    private static func extractOverrides(_ pack: URL, subdir: String, into instanceDir: URL) throws {
        let tmp = try makeTempDirectory()
        defer { try? fm.removeItem(at: tmp) }

        // 通配符由 unzip 自己展开，不需要 shell
        let (code, out) = try runProcess(unzipPath,
                                         ["-q", "-o", pack.path, "\(subdir)/*", "-d", tmp.path],
                                         cwd: nil)
        let srcDir = tmp.appendingPathComponent(subdir, isDirectory: true)
        guard fm.fileExists(atPath: srcDir.path) else {
            // 退出码 11 = 没有匹配到条目，说明包里没有这个目录，正常情况
            if code == 0 || code == 11 { return }
            throw ModpackError.unzipFailed(out.isEmpty ? "unzip 退出码 \(code)" : out)
        }
        try mergeDirectory(from: srcDir, into: instanceDir)
    }

    /// 把 src 的内容递归合并进 dst，已存在的同名文件以 src 覆盖。
    private static func mergeDirectory(from src: URL, into dst: URL) throws {
        try fm.createDirectory(at: dst, withIntermediateDirectories: true)
        for name in Paths.contentsOf(src) {
            let s = src.appendingPathComponent(name)
            let d = dst.appendingPathComponent(name)
            if Paths.isDirectory(s.path) {
                try mergeDirectory(from: s, into: d)
            } else {
                if fm.fileExists(atPath: d.path) { try? fm.removeItem(at: d) }
                try fm.createDirectory(at: d.deletingLastPathComponent(), withIntermediateDirectories: true)
                try fm.copyItem(at: s, to: d)
            }
        }
    }

    // MARK: - 私有：下载

    /// 下载单个条目；返回 nil 表示成功，否则返回失败原因。sha1 不匹配时重下一次。
    private static func downloadEntry(_ entry: ModpackFileEntry, into instanceDir: URL) async -> String? {
        // 防止 ../ 之类路径跳出实例目录
        guard !entry.path.hasPrefix("/"), !entry.path.contains("..") else {
            return "非法的目标路径"
        }
        let dest = instanceDir.appendingPathComponent(entry.path)
        let expectedSHA1 = entry.hashes["sha1"]

        // 已存在且校验通过则跳过（重复导入可续传；导出包里 downloads 为空时也靠这里生效）
        if fm.fileExists(atPath: dest.path) {
            if let expectedSHA1 {
                if sha1(of: dest) == expectedSHA1 { return nil }
            } else {
                return nil
            }
        }

        let urls = entry.downloads.compactMap { URL(string: $0) }
        guard !urls.isEmpty else { return "没有可用的下载地址" }

        var lastReason = "未知错误"
        for _ in 0..<2 {
            for url in urls {
                do {
                    try await DownloadManager.download(url, to: dest)
                    if let expectedSHA1 {
                        let actual = sha1(of: dest)
                        if actual != expectedSHA1 {
                            try? fm.removeItem(at: dest)
                            let actualShort = actual.map { String($0.prefix(8)) } ?? "无"
                            lastReason = "sha1 校验失败（期望 \(expectedSHA1.prefix(8))…，实际 \(actualShort)…）"
                            continue
                        }
                    }
                    return nil
                } catch {
                    lastReason = error.localizedDescription
                }
            }
        }
        return lastReason
    }

    /// 并发下载计数器：next 决定取哪个条目，done 用于进度，failures 收集失败原因。
    private actor ModpackDownloadTracker {
        private let total: Int
        private var next = 0
        private var done = 0
        private var failures: [String] = []

        init(total: Int) { self.total = total }

        func take() -> Int? {
            guard next < total else { return nil }
            defer { next += 1 }
            return next
        }

        func finish(_ failure: String?) -> (done: Int, total: Int) {
            done += 1
            if let failure { failures.append(failure) }
            return (done, total)
        }

        func allFailures() -> [String] { failures }
    }

    // MARK: - 私有：工具

    private static func makeTempDirectory() throws -> URL {
        let url = fm.temporaryDirectory
            .appendingPathComponent("GlassMC-Modpack-\(UUID().uuidString)", isDirectory: true)
        do {
            try fm.createDirectory(at: url, withIntermediateDirectories: true)
        } catch {
            throw ModpackError.unzipFailed("无法创建临时目录：\(error.localizedDescription)")
        }
        return url
    }

    /// 从 index.name 或用户输入派生出合法的实例目录名。
    private static func uniqueInstanceId(from raw: String) -> String {
        var s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        // 文件名里不允许的字符统一换成短横线
        s = s.components(separatedBy: CharacterSet(charactersIn: "\\/:*?\"<>|")).joined(separator: "-")
        s = s.replacingOccurrences(of: "\n", with: "-").replacingOccurrences(of: "\t", with: "-")
        while s.contains("--") { s = s.replacingOccurrences(of: "--", with: "-") }
        while s.hasPrefix(".") { s.removeFirst() }
        while s.hasSuffix("-") { s.removeLast() }
        s = String(s.prefix(64))
        if s.isEmpty { s = "Modpack-\(Int(Date().timeIntervalSince1970))" }

        // 避免覆盖已有实例
        var candidate = s
        var i = 2
        while fm.fileExists(atPath: Paths.versionsDir.appendingPathComponent(candidate).path) {
            candidate = "\(s)-\(i)"
            i += 1
        }
        return candidate
    }

    /// 从 dependencies 判断加载器，优先级与 Modrinth 一致：neoforge > forge > fabric > quilt。
    private static func detectLoader(_ dependencies: [String: String]) -> (name: String, version: String) {
        let order = [("neoforge", "neoforge"), ("forge", "forge"),
                     ("fabric-loader", "fabric"), ("quilt-loader", "quilt")]
        for (key, name) in order {
            if let v = dependencies[key], !v.isEmpty { return (name, v) }
        }
        return ("vanilla", "")
    }

    /// 导出时推断 dependencies：minecraft 取 inheritsFrom 或版本名前缀，加载器版本从版本 id 尾部截取。
    private static func inferDependencies(versionId: String) -> [String: String] {
        var deps: [String: String] = [:]

        let jsonURL = Paths.versionsDir.appendingPathComponent("\(versionId)/\(versionId).json")
        var json: [String: Any] = [:]
        if let data = try? Data(contentsOf: jsonURL),
           let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            json = obj
        }
        let inherits = json["inheritsFrom"] as? String

        // 基础游戏版本：优先继承字段，否则取版本 id 里第一个 "-" 之前的部分（如 1.21.1-NeoForge_21.1.236）
        var base = inherits ?? versionId
        if inherits == nil, let dash = versionId.firstIndex(of: "-") {
            let head = String(versionId[versionId.startIndex..<dash])
            if head.first?.isNumber == true { base = head }
        }
        deps["minecraft"] = base

        // 加载器：靠版本目录名判断（GlassMC 的版本 id 直接沿用整合包的可读名）
        let lower = versionId.lowercased()
        let jsonId = (json["id"] as? String) ?? versionId
        if lower.contains("neoforge") {
            deps["neoforge"] = loaderVersion(from: versionId, marker: "neoforge", fallback: jsonId)
        } else if lower.contains("forge") {
            deps["forge"] = loaderVersion(from: versionId, marker: "forge", fallback: jsonId)
        } else if lower.contains("fabric") {
            deps["fabric-loader"] = loaderVersion(from: versionId, marker: "fabric", fallback: jsonId)
        } else if lower.contains("quilt") {
            deps["quilt-loader"] = loaderVersion(from: versionId, marker: "quilt", fallback: jsonId)
        }
        return deps
    }

    /// 取 marker 之后的部分作为加载器版本号，去掉开头的分隔符；取不到时退回 fallback。
    private static func loaderVersion(from versionId: String, marker: String, fallback: String) -> String {
        guard let range = versionId.range(of: marker, options: .caseInsensitive) else { return fallback }
        var tail = String(versionId[range.upperBound...])
        while let f = tail.first, f == "-" || f == "_" || f == " " || f == "v" || f == "V" {
            tail.removeFirst()
        }
        return tail.isEmpty ? fallback : tail
    }

    /// 递归复制目录（保留结构），filter 作用在文件名上。
    private static func copyTree(from source: URL, to target: URL, filter: ((String) -> Bool)? = nil) throws {
        try fm.createDirectory(at: target, withIntermediateDirectories: true)
        for name in Paths.contentsOf(source) {
            let s = source.appendingPathComponent(name)
            let d = target.appendingPathComponent(name)
            if Paths.isDirectory(s.path) {
                try copyTree(from: s, to: d, filter: filter)
            } else {
                if let filter, !filter(name) { continue }
                if fm.fileExists(atPath: d.path) { try? fm.removeItem(at: d) }
                try fm.copyItem(at: s, to: d)
            }
        }
    }

    /// 只复制目录顶层的指定后缀文件，返回被复制的文件名。
    @discardableResult
    private static func copyFlat(from source: URL, to target: URL, extensions exts: Set<String>) throws -> [String] {
        var copied: [String] = []
        guard Paths.isDirectory(source.path) else { return copied }
        try fm.createDirectory(at: target, withIntermediateDirectories: true)
        for name in Paths.contentsOf(source) {
            let s = source.appendingPathComponent(name)
            guard !Paths.isDirectory(s.path) else { continue }
            guard exts.contains(s.pathExtension.lowercased()) else { continue }
            let d = target.appendingPathComponent(name)
            if fm.fileExists(atPath: d.path) { try? fm.removeItem(at: d) }
            try fm.copyItem(at: s, to: d)
            copied.append(name)
        }
        return copied
    }

    /// 文件大小（字节），读取失败返回 nil。
    private static func fileSize(of url: URL) -> Int64? {
        guard let attrs = try? fm.attributesOfItem(atPath: url.path),
              let num = attrs[.size] as? NSNumber else { return nil }
        return num.int64Value
    }

    /// 计算文件的 SHA-1（十六进制小写），失败返回 nil。
    static func sha1(of url: URL) -> String? {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }

        var context = CC_SHA1_CTX()
        CC_SHA1_Init(&context)
        while true {
            let chunk = handle.readData(ofLength: 1 << 20)
            if chunk.isEmpty { break }
            chunk.withUnsafeBytes { raw in
                _ = CC_SHA1_Update(&context, raw.baseAddress, CC_LONG(chunk.count))
            }
        }
        var digest = [UInt8](repeating: 0, count: Int(CC_SHA1_DIGEST_LENGTH))
        CC_SHA1_Final(&digest, &context)
        return digest.map { String(format: "%02x", $0) }.joined()
    }
}
