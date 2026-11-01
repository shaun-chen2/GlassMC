import Foundation
import CommonCrypto

// MARK: - 数据模型

/// Modrinth 搜索结果中的一条项目。
struct MRSearchHit: Identifiable, Hashable {
    let projectId: String
    let slug: String
    let title: String
    let description: String
    let author: String
    let iconURL: String
    let downloads: Int

    var id: String { projectId }

    /// 下载量的人话表示（1.2M / 34.5K）。
    var downloadsText: String {
        if downloads >= 1_000_000 { return String(format: "%.1fM", Double(downloads) / 1_000_000) }
        if downloads >= 1_000 { return String(format: "%.1fK", Double(downloads) / 1_000) }
        return "\(downloads)"
    }
}

/// 版本下的一个可下载文件。
struct MRFile: Hashable {
    let filename: String
    let url: String
    let sha1: String
    let size: Int
}

/// 版本声明的依赖项。type 取值 required / optional / incompatible / embedded。
struct MRDependency: Hashable {
    let projectId: String?
    let versionId: String?
    let type: String

    var typeText: String {
        switch type.lowercased() {
        case "required": return "必需"
        case "optional": return "可选"
        case "incompatible": return "不兼容"
        case "embedded": return "已内置"
        default: return type
        }
    }
}

/// Modrinth 上的一个具体版本。
struct MRVersion: Identifiable, Hashable {
    let id: String
    let versionNumber: String
    let name: String
    let gameVersions: [String]
    let loaders: [String]
    let files: [MRFile]
    let dependencies: [MRDependency]
    let datePublished: String

    /// 主文件（Modrinth 的 primary 文件排在最前，见 versions() 的排序逻辑）。
    var primaryFile: MRFile? { files.first }

    /// 发布日期只保留 yyyy-MM-dd，列表里更好看。
    var dateText: String {
        guard datePublished.count >= 10 else { return datePublished }
        return String(datePublished.prefix(10))
    }
}

/// 本地 mods 目录里的一个 jar。
struct InstalledMod: Identifiable, Hashable {
    let fileName: String
    let path: String
    let modId: String
    let name: String
    let version: String
    let size: Int64
    let enabled: Bool

    var id: String { path }

    var sizeText: String {
        ByteCountFormatter.string(fromByteCount: size, countStyle: .file)
    }
}

// MARK: - 错误

enum ModrinthError: LocalizedError {
    case apiFailed(Int)
    case sha1Mismatch(String)
    case badArchive(String)

    var errorDescription: String? {
        switch self {
        case .apiFailed(let code):
            switch code {
            case 400: return "Modrinth 拒绝了请求（400）：查询参数有误"
            case 401, 403: return "Modrinth 拒绝了访问（\(code)），请稍后重试"
            case 404: return "Modrinth 上找不到该项目或版本（404）"
            case 429: return "请求过于频繁（429），请稍后再试"
            case 500...599: return "Modrinth 服务端错误（\(code)），请稍后重试"
            default: return "Modrinth 请求失败（HTTP \(code)）"
            }
        case .sha1Mismatch(let file):
            return "文件校验失败：\(file) 的 SHA1 与官方记录不一致，已删除下载文件"
        case .badArchive(let file):
            return "无法解析压缩包：\(file)（可能不是有效的 mod jar）"
        }
    }
}

// MARK: - Modrinth 网络客户端

enum ModrinthClient {

    /// Modrinth 对缺少 User-Agent 的请求会限流，所有请求必须带上。
    static let userAgent = "GlassMC/1.0 (macOS)"
    private static let apiRoot = "https://api.modrinth.com/v2"

    // MARK: 搜索

    /// 搜索项目。game / loader 为空字符串时不加对应 facet。
    static func search(query: String,
                       game: String,
                       loader: String,
                       offset: Int,
                       limit: Int = 20) async throws -> (hits: [MRSearchHit], total: Int) {
        var facets: [String] = []
        if !game.trimmingCharacters(in: .whitespaces).isEmpty {
            facets.append("[\"versions:\(game)\"]")
        }
        if !loader.trimmingCharacters(in: .whitespaces).isEmpty {
            facets.append("[\"categories:\(loader)\"]")
        }

        var components = URLComponents(string: apiRoot + "/search")
        var items: [URLQueryItem] = [
            URLQueryItem(name: "query", value: query),
            URLQueryItem(name: "limit", value: String(max(1, min(limit, 100)))),
            URLQueryItem(name: "offset", value: String(max(0, offset))),
            URLQueryItem(name: "index", value: "relevance")
        ]
        if !facets.isEmpty {
            items.append(URLQueryItem(name: "facets", value: "[" + facets.joined(separator: ",") + "]"))
        }
        components?.queryItems = items
        guard let url = components?.url else { throw URLError(.badURL) }

        let data = try await get(url)
        let decoded = try JSONDecoder().decode(SearchResponse.self, from: data)
        let hits = decoded.hits.map { hit in
            MRSearchHit(projectId: hit.projectId ?? hit.slug ?? "",
                        slug: hit.slug ?? "",
                        title: hit.title ?? "（未命名）",
                        description: hit.description ?? "",
                        author: hit.author ?? "",
                        iconURL: hit.iconURL ?? "",
                        downloads: hit.downloads ?? 0)
        }
        return (hits, decoded.totalHits ?? hits.count)
    }

    // MARK: 版本列表

    /// 拉取项目的版本列表，按 game / loader 过滤（为空则不过滤）。
    static func versions(projectId: String, game: String, loader: String) async throws -> [MRVersion] {
        guard var components = URLComponents(string: apiRoot + "/project/\(projectId)/version") else {
            throw URLError(.badURL)
        }
        var items: [URLQueryItem] = []
        if !loader.trimmingCharacters(in: .whitespaces).isEmpty {
            items.append(URLQueryItem(name: "loaders", value: "[\"\(loader)\"]"))
        }
        if !game.trimmingCharacters(in: .whitespaces).isEmpty {
            items.append(URLQueryItem(name: "game_versions", value: "[\"\(game)\"]"))
        }
        components.queryItems = items.isEmpty ? nil : items
        guard let url = components.url else { throw URLError(.badURL) }

        let data = try await get(url)
        let decoded = try JSONDecoder().decode([VersionDTO].self, from: data)

        return decoded.map { dto in
            // primary 文件排前面，UI 直接取 first 即可
            let files = (dto.files ?? []).sorted { ($0.primary ?? false) && !($1.primary ?? false) }
            let mapped = files.compactMap { f -> MRFile? in
                guard let name = f.filename, let link = f.url else { return nil }
                return MRFile(filename: name,
                              url: link,
                              sha1: (f.hashes?["sha1"] ?? "").lowercased(),
                              size: f.size ?? 0)
            }
            let deps = (dto.dependencies ?? []).map { d in
                MRDependency(projectId: d.projectID, versionId: d.versionID, type: d.dependencyType ?? "optional")
            }
            let id = dto.id ?? ""
            return MRVersion(id: id,
                             versionNumber: dto.versionNumber ?? "",
                             name: dto.name ?? (dto.versionNumber ?? ""),
                             gameVersions: dto.gameVersions ?? [],
                             loaders: dto.loaders ?? [],
                             files: mapped,
                             dependencies: deps,
                             datePublished: dto.datePublished ?? "")
        }
        .filter { !$0.id.isEmpty }
    }

    // MARK: 下载

    /// 下载文件到指定目录，并用 SHA1 校验；校验失败会删除文件。
    @discardableResult
    static func download(_ file: MRFile,
                         toDirectory dir: URL,
                         progress: ((Double, String?) -> Void)? = nil) async throws -> URL {
        guard let link = URL(string: file.url) else { throw URLError(.badURL) }
        let fm = FileManager.default
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        let destination = dir.appendingPathComponent(file.filename)

        try await DownloadManager.download(link, to: destination, progress: progress)

        let expected = file.sha1.lowercased()
        if !expected.isEmpty {
            let actual = try sha1Hex(of: destination)
            if actual != expected {
                try? fm.removeItem(at: destination)
                throw ModrinthError.sha1Mismatch(file.filename)
            }
        }
        return destination
    }

    /// 计算文件的 SHA1（小写 hex），流式读取，避免大 jar 占内存。
    static func sha1Hex(of url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }

        var ctx = CC_SHA1_CTX()
        CC_SHA1_Init(&ctx)
        while true {
            guard let chunk = try handle.read(upToCount: 1 << 20), !chunk.isEmpty else { break }
            chunk.withUnsafeBytes { buffer in
                _ = CC_SHA1_Update(&ctx, buffer.baseAddress, CC_LONG(buffer.count))
            }
        }
        var digest = [UInt8](repeating: 0, count: Int(CC_SHA1_DIGEST_LENGTH))
        CC_SHA1_Final(&digest, &ctx)
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    // MARK: 依赖项目名解析

    /// 批量查项目标题（依赖提示用），返回 id/slug → title。
    /// 批量接口是 `GET /v2/projects?ids=["AABBCC"]`；旧的 POST 形式已被 Modrinth 下线（返回 404）。
    static func resolveProjectTitles(ids: [String]) async throws -> [String: String] {
        let unique = Array(Set(ids.filter { !$0.isEmpty }))
        guard !unique.isEmpty else { return [:] }
        guard var components = URLComponents(string: apiRoot + "/projects") else { throw URLError(.badURL) }

        // 值是 JSON 字符串数组，例如 ["AABBCC"]，URLComponents 会负责百分号编码
        let list = "[" + unique.map { "\"\($0)\"" }.joined(separator: ",") + "]"
        components.queryItems = [URLQueryItem(name: "ids", value: list)]
        guard let url = components.url else { throw URLError(.badURL) }

        let data = try await get(url)
        let decoded = try JSONDecoder().decode([ProjectDTO].self, from: data)

        var result: [String: String] = [:]
        for project in decoded {
            let title = project.title ?? ""
            if let id = project.id, !id.isEmpty { result[id] = title }
            if let slug = project.slug, !slug.isEmpty { result[slug] = title }
        }
        return result
    }

    // MARK: 内部请求

    private static func get(_ url: URL) async throws -> Data {
        try await request(url, method: "GET", body: nil)
    }

    private static func request(_ url: URL, method: String, body: Data?) async throws -> Data {
        var request = URLRequest(url: url, timeoutInterval: 30)
        request.httpMethod = method
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let body {
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            throw ModrinthError.apiFailed(http.statusCode)
        }
        return data
    }

    // MARK: DTO（字段全部可选，缺字段不崩）

    private struct SearchResponse: Decodable {
        let hits: [Hit]
        let totalHits: Int?

        enum CodingKeys: String, CodingKey {
            case hits
            case totalHits = "total_hits"
        }

        struct Hit: Decodable {
            let projectId: String?
            let slug: String?
            let title: String?
            let description: String?
            let author: String?
            let iconURL: String?
            let downloads: Int?

            enum CodingKeys: String, CodingKey {
                case projectId = "project_id"
                case slug, title, description, author, downloads
                case iconURL = "icon_url"
            }
        }
    }

    private struct VersionDTO: Decodable {
        let id: String?
        let versionNumber: String?
        let name: String?
        let gameVersions: [String]?
        let loaders: [String]?
        let files: [FileDTO]?
        let dependencies: [DependencyDTO]?
        let datePublished: String?

        enum CodingKeys: String, CodingKey {
            case id, name, loaders, files, dependencies
            case versionNumber = "version_number"
            case gameVersions = "game_versions"
            case datePublished = "date_published"
        }

        struct FileDTO: Decodable {
            let filename: String?
            let url: String?
            let hashes: [String: String]?
            let size: Int?
            let primary: Bool?
        }

        struct DependencyDTO: Decodable {
            let projectID: String?
            let versionID: String?
            let dependencyType: String?

            enum CodingKeys: String, CodingKey {
                case projectID = "project_id"
                case versionID = "version_id"
                case dependencyType = "dependency_type"
            }
        }
    }

    private struct ProjectDTO: Decodable {
        let id: String?
        let slug: String?
        let title: String?
    }
}

// MARK: - 本地 mods 管理

enum LocalMods {

    /// 优先使用 HMCL 隔离实例的 mods 目录（versions/<id>/mods），否则用全局 mods。
    static func modsDirectory(versionId: String) -> URL? {
        let trimmed = versionId.trimmingCharacters(in: .whitespaces)
        if !trimmed.isEmpty {
            let isolated = Paths.gameDir
                .appendingPathComponent("versions", isDirectory: true)
                .appendingPathComponent(trimmed, isDirectory: true)
                .appendingPathComponent("mods", isDirectory: true)
            if Paths.isDirectory(isolated.path) { return isolated }
        }
        // 全局目录即使暂不存在也返回，调用方写入时会自动创建
        return Paths.gameDir.appendingPathComponent("mods", isDirectory: true)
    }

    /// 扫描目录下的 jar（含已禁用），元数据来自 jar 内 mods.toml。
    /// 每个 jar 都要 fork 一次 unzip 读 toml，几百个模组顺序扫要几十秒，
    /// 这里按 CPU 核心并发执行（结果按文件名排序写回固定下标，线程安全）。
    static func scan(_ dir: URL) -> [InstalledMod] {
        guard Paths.isDirectory(dir.path) else { return [] }
        let names = Paths.contentsOf(dir).sorted().filter { name in
            let lower = name.lowercased()
            guard lower.hasSuffix(".jar") || lower.hasSuffix(".jar.disabled") else { return false }
            return !Paths.isDirectory(dir.appendingPathComponent(name).path)
        }
        guard !names.isEmpty else { return [] }

        var slots = [InstalledMod?](repeating: nil, count: names.count)
        slots.withUnsafeMutableBufferPointer { buf in
            DispatchQueue.concurrentPerform(iterations: names.count) { i in
                let name = names[i]
                let lower = name.lowercased()
                let enabled = lower.hasSuffix(".jar")
                let url = dir.appendingPathComponent(name)
                let meta = JarMetaCache.shared.meta(for: url)
                let fallback = cleanedName(name)
                buf[i] = InstalledMod(fileName: name,
                                      path: url.path,
                                      modId: meta.modId ?? sanitizedID(fallback.title),
                                      name: meta.displayName ?? fallback.title,
                                      version: meta.version ?? fallback.version,
                                      size: fileSize(url),
                                      enabled: enabled)
            }
        }
        return slots.compactMap { $0 }
    }

    /// 所有已启用 jar 的 modId 集合（含 jarjar 内嵌 jar 的 modId）。
    static func installedModIds(_ dir: URL) -> Set<String> {
        guard Paths.isDirectory(dir.path) else { return [] }
        var ids: Set<String> = []
        for name in Paths.contentsOf(dir) {
            // 只统计启用的（.jar.disabled 不参与运行，不算已安装）
            guard name.lowercased().hasSuffix(".jar"),
                  !name.lowercased().hasSuffix(".jar.disabled") else { continue }
            let url = dir.appendingPathComponent(name)
            guard !Paths.isDirectory(url.path) else { continue }
            let meta = JarMetaCache.shared.meta(for: url)
            if let id = meta.modId { ids.insert(id.lowercased()) }
            for inner in meta.embeddedIDs { ids.insert(inner.lowercased()) }
        }
        return ids
    }

    /// 启用/禁用：改名 .jar ↔ .jar.disabled。
    static func setEnabled(_ mod: InstalledMod, enabled: Bool) throws {
        let fm = FileManager.default
        let url = URL(fileURLWithPath: mod.path)
        let name = url.lastPathComponent
        let lower = name.lowercased()

        let targetName: String
        if enabled {
            guard lower.hasSuffix(".jar.disabled") else { return } // 已是启用状态
            targetName = String(name.dropLast(".disabled".count))
        } else {
            guard lower.hasSuffix(".jar") else { return }          // 已是禁用状态
            targetName = name + ".disabled"
        }

        let target = url.deletingLastPathComponent().appendingPathComponent(targetName)
        guard !fm.fileExists(atPath: target.path) else { throw CocoaError(.fileWriteFileExists) }
        try fm.moveItem(at: url, to: target)
        JarMetaCache.shared.forget(url)
        JarMetaCache.shared.forget(target)
    }

    /// 删除：移动到 <dir>/.trash/ 而不是真删，方便回滚。
    static func remove(_ mod: InstalledMod) throws {
        let fm = FileManager.default
        let url = URL(fileURLWithPath: mod.path)
        let trash = url.deletingLastPathComponent().appendingPathComponent(".trash", isDirectory: true)
        try fm.createDirectory(at: trash, withIntermediateDirectories: true)

        var destination = trash.appendingPathComponent(url.lastPathComponent)
        if fm.fileExists(atPath: destination.path) {
            let stamp = Int(Date().timeIntervalSince1970)
            destination = trash.appendingPathComponent("\(stamp)-\(url.lastPathComponent)")
        }
        try fm.moveItem(at: url, to: destination)
        JarMetaCache.shared.forget(url)
    }

    /// 仍缺失的必需依赖（只做 projectId 与已装 modId/slug 的字符串比较）。
    static func missingDependencies(forMRVersion v: MRVersion, installed: Set<String>) -> [MRDependency] {
        let known = Set(installed.map { $0.lowercased() })
        return v.dependencies.filter { dep in
            guard dep.type.lowercased() == "required" else { return false }
            // 拿不到 projectId 就无法判断，按“缺失”返回交给界面提示
            guard let pid = dep.projectId?.lowercased(), !pid.isEmpty else { return true }
            return !known.contains(pid)
        }
    }

    // MARK: 内部：jar 元数据

    fileprivate struct JarMeta {
        var modId: String?
        var version: String?
        var displayName: String?
        var embeddedIDs: [String] = []
    }

    /// 读取 jar 元数据：优先 META-INF/neoforge.mods.toml，其次 META-INF/mods.toml；
    /// 主 jar 没有 modId 时，回退到 META-INF/jarjar/ 内第一个内嵌 jar 的 modId。
    fileprivate static func readMeta(_ jar: URL, includeEmbedded: Bool) -> JarMeta {
        var meta = JarMeta()
        for entry in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
            guard let text = unzipPipe(["-p", jar.path, entry]) else { continue }
            let parsed = parseToml(text)
            if meta.modId == nil { meta.modId = parsed.modId }
            if meta.version == nil { meta.version = parsed.version }
            if meta.displayName == nil { meta.displayName = parsed.displayName }
            if meta.modId != nil { break }
        }

        let entries = jarjarEntries(in: jar)
        guard !entries.isEmpty else { return meta }

        // 主 jar 缺 modId 或调用方需要完整集合时，才去解析内嵌 jar（解压较慢）
        if includeEmbedded || meta.modId == nil {
            meta.embeddedIDs = embeddedModIDs(in: jar, entries: Array(entries.prefix(3)))
            if meta.modId == nil, let first = meta.embeddedIDs.first {
                meta.modId = first
            }
        }
        return meta
    }

    /// 列出 jar 内 META-INF/jarjar/*.jar（不实现 zip 解析，交给 /usr/bin/unzip）。
    fileprivate static func jarjarEntries(in jar: URL) -> [String] {
        guard let listing = unzipPipe(["-l", jar.path]) else { return [] }
        var result: [String] = []
        for line in listing.split(separator: "\n") {
            guard let range = line.range(of: "META-INF/jarjar/") else { continue }
            let name = String(line[range.lowerBound...]).trimmingCharacters(in: .whitespaces)
            guard name.lowercased().hasSuffix(".jar") else { continue }
            result.append(name)
        }
        return result
    }

    fileprivate static func embeddedModIDs(in jar: URL, entries: [String]) -> [String] {
        let fm = FileManager.default
        let tmpDir = fm.temporaryDirectory.appendingPathComponent("GlassMC-jarjar-\(UUID().uuidString)", isDirectory: true)
        guard (try? fm.createDirectory(at: tmpDir, withIntermediateDirectories: true)) != nil else { return [] }
        defer { try? fm.removeItem(at: tmpDir) }

        var ids: [String] = []
        for entry in entries {
            let inner = tmpDir.appendingPathComponent("inner.jar")
            guard extract(entry: entry, from: jar, to: inner) else { continue }
            for toml in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
                guard let text = unzipPipe(["-p", inner.path, toml]) else { continue }
                if let id = parseToml(text).modId {
                    ids.append(id)
                    break
                }
            }
        }
        return ids
    }

    /// 只解析第一个 [[mods]] 段，避免把 [[dependencies."xxx"]] 里的 modId 当成主 mod。
    fileprivate static func parseToml(_ text: String) -> (modId: String?, version: String?, displayName: String?) {
        var scope = text
        if let header = text.range(of: "[[mods]]") {
            let rest = text[header.upperBound...]
            if let next = rest.range(of: "[[") {
                scope = String(rest[..<next.lowerBound])
            } else {
                scope = String(rest)
            }
        }
        return (tomlValue("modId", in: scope),
                tomlValue("version", in: scope),
                tomlValue("displayName", in: scope))
    }

    /// 取 key = "value" 或 key = 'value' 的第一个值（允许空格与行尾注释）。
    /// 双引号值与单引号值分别匹配，避免 `displayName = "YUNG's API"` 这种值里的撇号提前截断。
    fileprivate static func tomlValue(_ key: String, in text: String) -> String? {
        let pattern = "(?m)^\\s*\(key)\\s*=\\s*(?:\"([^\"\\n]*)\"|'([^'\\n]*)')"
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return nil }
        let ns = text as NSString
        guard let match = regex.firstMatch(in: text, range: NSRange(location: 0, length: ns.length)) else { return nil }
        for index in 1..<match.numberOfRanges {
            let range = match.range(at: index)
            guard range.location != NSNotFound else { continue }
            let value = ns.substring(with: range).trimmingCharacters(in: .whitespaces)
            // ${file.jarVersion} 这类占位符没有展示价值，当作缺失
            if !value.isEmpty && !value.hasPrefix("$") { return value }
        }
        return nil
    }

    /// 执行 unzip 并取标准输出；失败返回 nil。
    fileprivate static func unzipPipe(_ args: [String]) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            return nil
        }
        // 先读完管道再等待退出，避免大文件（内嵌 jar 可达 20MB+）写满缓冲区导致死锁
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0, !data.isEmpty else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// 把 jar 内的一个条目解到临时文件（用于读取内嵌 jar 的 toml）。
    fileprivate static func extract(entry: String, from jar: URL, to destination: URL) -> Bool {
        let fm = FileManager.default
        try? fm.removeItem(at: destination)
        guard fm.createFile(atPath: destination.path, contents: nil),
              let handle = try? FileHandle(forWritingTo: destination) else { return false }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
        process.arguments = ["-p", jar.path, entry]
        process.standardOutput = handle
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            try? handle.close()
            return false
        }
        process.waitUntilExit()
        try? handle.close()
        return process.terminationStatus == 0 && fileSize(destination) > 0
    }

    // MARK: 内部：文件名兜底

    fileprivate static func fileSize(_ url: URL) -> Int64 {
        let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
        return (attrs?[.size] as? NSNumber)?.int64Value ?? 0
    }

    /// 从文件名猜（名称, 版本）。例如 YungsApi-1.21.1-NeoForge-5.1.6.jar → ("YungsApi 1.21.1 NeoForge", "5.1.6")
    fileprivate static func cleanedName(_ fileName: String) -> (title: String, version: String) {
        var stem = fileName
        if stem.lowercased().hasSuffix(".disabled") { stem = String(stem.dropLast(9)) }
        if stem.lowercased().hasSuffix(".jar") { stem = String(stem.dropLast(4)) }

        var version = ""
        let pattern = #"(?i)[-_.]v?\d+(\.\d+)+([-+_.][A-Za-z0-9]+)*$"#
        if let range = stem.range(of: pattern, options: .regularExpression) {
            version = String(stem[range]).drop(while: { "-_.vV".contains($0) }).description
            stem = String(stem[..<range.lowerBound])
        }
        let title = stem
            .replacingOccurrences(of: "_", with: " ")
            .replacingOccurrences(of: "-", with: " ")
            .trimmingCharacters(in: .whitespaces)
        return (title.isEmpty ? fileName : title, version)
    }

    /// 兜底 modId：小写、非字母数字统一成下划线。
    fileprivate static func sanitizedID(_ title: String) -> String {
        let allowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyz0123456789_")
        let lowered = title.lowercased().replacingOccurrences(of: " ", with: "_")
        let scalars = lowered.unicodeScalars.map { allowed.contains($0) ? Character($0) : "_" }
        let id = String(scalars)
        return id.isEmpty ? "unknown" : id
    }
}

// MARK: - jar 元数据缓存

/// 解析 jar 里的 toml 需要 fork 进程，缓存结果避免列表刷新时反复解压。
private final class JarMetaCache: @unchecked Sendable {
    static let shared = JarMetaCache()

    private let lock = NSLock()
    private var store: [String: (stamp: String, meta: LocalMods.JarMeta)] = [:]

    func meta(for url: URL) -> LocalMods.JarMeta {
        let key = url.path
        let stamp = Self.stamp(of: url)

        lock.lock()
        if let cached = store[key], cached.stamp == stamp {
            lock.unlock()
            return cached.meta
        }
        lock.unlock()

        let meta = LocalMods.readMeta(url, includeEmbedded: true)

        lock.lock()
        store[key] = (stamp, meta)
        lock.unlock()
        return meta
    }

    func forget(_ url: URL) {
        lock.lock()
        store.removeValue(forKey: url.path)
        lock.unlock()
    }

    private static func stamp(of url: URL) -> String {
        let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
        let size = (attrs?[.size] as? NSNumber)?.int64Value ?? 0
        let date = (attrs?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
        return "\(size)-\(date)"
    }
}
