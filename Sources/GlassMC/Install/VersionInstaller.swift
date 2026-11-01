import Foundation

/// 远程版本清单中的一条记录（来自 Mojang / BMCLAPI 的 version_manifest_v2）。
struct RemoteVersion: Identifiable, Hashable {
    let versionId: String
    /// release / snapshot / old_beta / old_alpha
    let type: String
    /// ISO8601 时间串（列表展示时可直接截断成日期）
    let releaseTime: String

    var id: String { versionId }
}

/// 安装原版版本时可能出现的错误。
enum VersionInstallError: LocalizedError {
    /// 版本清单拉取失败（镜像与官方源都不可用）
    case manifestFailed
    /// 版本 JSON 无法解析
    case badJSON(String)
    /// 父版本（inheritsFrom）缺失且无法安装
    case missingParent(String)
    /// 文件下载失败
    case downloadFailed(String)
    /// 继承链异常（层级过深或自引用）
    case tooDeepInheritance(String)

    var errorDescription: String? {
        switch self {
        case .manifestFailed:
            return "无法获取游戏版本清单，请检查网络或到「设置」更换下载源后重试。"
        case .badJSON(let id):
            return "版本文件损坏或格式不正确：\(id).json。请删除该版本目录后重新下载。"
        case .missingParent(let id):
            return "缺少前置父版本：\(id)。加载器版本必须建立在对应的原版版本之上，请先安装原版 \(id)。"
        case .downloadFailed(let id):
            return "下载 \(id) 的版本文件失败，请检查网络后重试。"
        case .tooDeepInheritance(let id):
            return "版本继承关系异常（\(id)），无法解析前置版本。"
        }
    }
}

/// 在线安装原版 Minecraft 版本：拉取清单 → 写入 version.json → 补齐 jar / 库 / 资源。
///
/// 关键点：
/// - 清单用 URLSession + JSONSerialization 解析，不依赖项目里的 VersionManifest，
///   避免镜像与官方字段差异导致解码失败。
/// - 带 `inheritsFrom` 的版本（整合包实例、部分镜像版本）必须先有父版本 JSON，
///   否则 LaunchEngine 的 prepare 会直接失败，所以这里递归先装父版本。
enum VersionInstaller {

    /// 官方清单地址，仅在镜像不可用时兜底。
    private static let officialManifestURL =
        URL(string: "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json")!

    // MARK: - 清单

    /// 清单条目（内部使用，额外保留 json 官方地址用于回退）。
    private struct ManifestEntry {
        let id: String
        let type: String
        let releaseTime: String
        let url: URL?
    }

    /// 拉取远程版本列表；includeSnapshots 为 false 时只返回正式版。
    static func fetchAll(includeSnapshots: Bool) async throws -> [RemoteVersion] {
        let entries = try await manifestEntries()
        let allowed: Set<String> = ["release", "snapshot", "old_beta", "old_alpha"]
        let filtered = entries.filter { includeSnapshots ? allowed.contains($0.type) : $0.type == "release" }

        let formatter = ISO8601DateFormatter()
        let sorted = filtered.sorted { lhs, rhs in
            let l = formatter.date(from: lhs.releaseTime) ?? .distantPast
            let r = formatter.date(from: rhs.releaseTime) ?? .distantPast
            return l > r
        }
        AppState.shared.log("版本清单共 \(filtered.count) 个\(includeSnapshots ? "（含快照）" : "正式版")")
        return sorted.map { RemoteVersion(versionId: $0.id, type: $0.type, releaseTime: $0.releaseTime) }
    }

    /// 该版本是否已安装（存在 versions/{id}/{id}.json 即视为已安装）。
    static func isInstalled(_ versionId: String) -> Bool {
        let json = Paths.versionsDir
            .appendingPathComponent(versionId, isDirectory: true)
            .appendingPathComponent("\(versionId).json")
        return FileManager.default.fileExists(atPath: json.path)
    }

    // MARK: - 安装

    /// 安装指定原版版本。progress 的 0~0.7 用于版本 JSON（含父版本），0.7~1.0 用于依赖下载。
    static func install(_ versionId: String,
                        progress: @escaping (Double, String?) -> Void) async throws {
        Paths.ensureDirectories()
        AppState.shared.log("开始安装版本：\(versionId)")
        progress(0, "准备下载 \(versionId)")

        try await ensureVersionJSON(versionId, depth: 0, from: 0, to: 0.7, progress: progress)

        progress(0.7, "下载游戏文件")
        do {
            try await LaunchEngine.prepare(versionId: versionId) { p, message in
                progress(0.7 + min(max(p, 0), 1) * 0.3, message)
            }
        } catch {
            AppState.shared.log("版本 \(versionId) 依赖补齐失败：\(error.localizedDescription)")
            throw error
        }

        AppState.shared.log("版本 \(versionId) 安装完成")
        progress(1, "安装完成")
    }

    // MARK: - 内部实现

    /// 确保某版本（及其父版本）的 JSON 存在于本地。
    /// - Parameters:
    ///   - depth: 继承层级，防止畸形链导致无限递归。
    ///   - from/to: 该层级占用的进度区间。
    private static func ensureVersionJSON(_ versionId: String,
                                          depth: Int,
                                          from: Double,
                                          to: Double,
                                          progress: @escaping (Double, String?) -> Void) async throws {
        guard depth < 8 else { throw VersionInstallError.tooDeepInheritance(versionId) }

        let fm = FileManager.default
        let dir = Paths.versionsDir.appendingPathComponent(versionId, isDirectory: true)
        if !Paths.isDirectory(dir.path) {
            try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        let jsonPath = dir.appendingPathComponent("\(versionId).json")

        // ① 下载 version.json（镜像优先，失败回退清单里的官方地址）
        if !fm.fileExists(atPath: jsonPath.path) {
            progress(from, "下载版本信息 \(versionId)")
            AppState.shared.log("下载版本描述文件：\(versionId).json")
            do {
                try await DownloadManager.download(DownloadSource().versionJSONURL(versionId), to: jsonPath) { p, message in
                    progress(from + (to - from) * 0.5 * min(max(p, 0), 1), message ?? "下载版本信息 \(versionId)")
                }
            } catch {
                AppState.shared.log("镜像获取 \(versionId).json 失败，回退官方源：\(error.localizedDescription)")
                guard let official = await officialJSONURL(versionId) else {
                    throw depth > 0
                        ? VersionInstallError.missingParent(versionId)
                        : VersionInstallError.downloadFailed(versionId)
                }
                do {
                    try await DownloadManager.download(official, to: jsonPath) { p, message in
                        progress(from + (to - from) * 0.5 * min(max(p, 0), 1), message ?? "下载版本信息 \(versionId)")
                    }
                } catch {
                    throw depth > 0
                        ? VersionInstallError.missingParent(versionId)
                        : VersionInstallError.downloadFailed(versionId)
                }
            }
        }

        // ② 解析 JSON，必要时先递归安装父版本
        guard let data = try? Data(contentsOf: jsonPath),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw VersionInstallError.badJSON(versionId)
        }
        if let parent = json["inheritsFrom"] as? String, !parent.isEmpty, parent != versionId,
           !isInstalled(parent) {
            AppState.shared.log("版本 \(versionId) 依赖父版本 \(parent)，先安装父版本")
            progress(from + (to - from) * 0.6, "安装前置版本 \(parent)")
            try await ensureVersionJSON(parent, depth: depth + 1, from: from, to: to, progress: progress)
        }

        progress(to, "版本信息就绪")
    }

    /// 从清单里取出该版本的官方 json 地址（用于镜像失败后的回退）。
    private static func officialJSONURL(_ versionId: String) async -> URL? {
        guard let entries = try? await manifestEntries() else { return nil }
        return entries.first { $0.id == versionId }?.url
    }

    /// 拉清单：先镜像，失败再官方源。
    private static func manifestEntries() async throws -> [ManifestEntry] {
        let primary = DownloadSource().manifestURL
        do {
            return try await fetchManifest(primary)
        } catch {
            AppState.shared.log("获取版本清单失败，回退官方源：\(error.localizedDescription)")
            guard primary != officialManifestURL else { throw VersionInstallError.manifestFailed }
            return try await fetchManifest(officialManifestURL)
        }
    }

    private static func fetchManifest(_ url: URL) async throws -> [ManifestEntry] {
        var request = URLRequest(url: url, timeoutInterval: 30)
        request.setValue("GlassMC/1.0 (macOS; Minecraft Launcher)", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        let data: Data
        do {
            let (body, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
                throw VersionInstallError.manifestFailed
            }
            data = body
        } catch let error as VersionInstallError {
            throw error
        } catch {
            throw VersionInstallError.manifestFailed
        }

        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let versions = root["versions"] as? [[String: Any]] else {
            throw VersionInstallError.manifestFailed
        }

        let entries: [ManifestEntry] = versions.compactMap { item in
            guard let id = item["id"] as? String, !id.isEmpty else { return nil }
            let type = (item["type"] as? String) ?? "release"
            let time = (item["releaseTime"] as? String) ?? ""
            let url = (item["url"] as? String).flatMap { URL(string: $0) }
            return ManifestEntry(id: id, type: type, releaseTime: time, url: url)
        }
        guard !entries.isEmpty else { throw VersionInstallError.manifestFailed }
        return entries
    }
}
