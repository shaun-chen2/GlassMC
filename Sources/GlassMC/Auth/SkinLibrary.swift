import Foundation
import AppKit

/// 皮肤库条目（Blessing Skin 站点 `/skinlib/list` 返回结构的简化版）。
struct SkinLibraryItem: Identifiable, Hashable {
    let tid: Int
    let name: String
    /// 皮肤模型：alex（细臂）/ steve（粗臂）
    let type: String
    let likes: Int
    let author: String

    var id: Int { tid }
    var isAlex: Bool { type.lowercased() == "alex" }
    var modelLabel: String { isAlex ? "Alex" : "Steve" }
}

enum SkinLibraryError: LocalizedError {
    case badBase
    case http(Int)
    case parse

    var errorDescription: String? {
        switch self {
        case .badBase: return "皮肤站地址无效，请在设置中检查。"
        case .http(let code): return "皮肤站返回 HTTP \(code)。"
        case .parse: return "皮肤站返回的数据无法解析。"
        }
    }
}

/// 皮肤库客户端：对接 Blessing Skin 6 的 `/skinlib` 与 `/textures` 接口，
/// 兼容 MiniSkins（www.mcpeau.com）等同类皮肤站。纯原生网络请求，不涉及任何 Web 技术。
final class SkinLibraryClient {

    static let shared = SkinLibraryClient()

    /// 一页数据。
    struct Page {
        var items: [SkinLibraryItem] = []
        var page: Int = 1
        var lastPage: Int = 1
        var total: Int = 0
    }

    private let session: URLSession

    private init() {
        let cfg = URLSessionConfiguration.ephemeral
        cfg.timeoutIntervalForRequest = 25
        cfg.httpAdditionalHeaders = [
            "User-Agent": "GlassMC/1.0 (macOS launcher)",
            "Accept": "application/json, image/png",
            "X-Requested-With": "XMLHttpRequest"
        ]
        self.session = URLSession(configuration: cfg)
    }

    /// 站点基址（可在设置里改，默认 MiniSkins）。
    var base: String {
        var s = SettingsStore.shared.skinLibraryBase.trimmingCharacters(in: .whitespacesAndNewlines)
        while s.hasSuffix("/") { s.removeLast() }
        return s
    }

    // MARK: - 列表

    /// 拉取指定页的皮肤列表。
    func fetchPage(_ page: Int, category: String = "skin") async throws -> Page {
        guard !base.isEmpty, var comps = URLComponents(string: base + "/skinlib/list") else {
            throw SkinLibraryError.badBase
        }
        comps.queryItems = [
            URLQueryItem(name: "page", value: "\(max(1, page))"),
            URLQueryItem(name: "category", value: category)
        ]
        guard let url = comps.url else { throw SkinLibraryError.badBase }
        let data = try await get(url)
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw SkinLibraryError.parse
        }
        var out = Page()
        out.page = (root["current_page"] as? Int) ?? page
        out.lastPage = (root["last_page"] as? Int) ?? 1
        out.total = (root["total"] as? Int) ?? 0
        let rows = (root["data"] as? [[String: Any]]) ?? []
        out.items = rows.compactMap { row in
            guard let tid = row["tid"] as? Int else { return nil }
            let name = (row["name"] as? String) ?? "未命名"
            let type = (row["type"] as? String) ?? "steve"
            let likes = (row["likes"] as? Int) ?? 0
            let author = (row["nickname"] as? String) ?? ""
            return SkinLibraryItem(tid: tid, name: name, type: type, likes: likes, author: author)
        }
        return out
    }

    // MARK: - 贴图

    /// 查询条目详情以取得贴图 hash。
    func hash(for tid: Int) async throws -> String {
        guard let url = URL(string: base + "/skinlib/info/\(tid)") else { throw SkinLibraryError.badBase }
        let data = try await get(url)
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let hash = root["hash"] as? String, !hash.isEmpty else {
            throw SkinLibraryError.parse
        }
        return hash
    }

    /// 本地缓存文件（按 tid 命名，便于直接应用到账号）。
    func cacheFile(tid: Int) -> URL {
        Paths.skinsDir.appendingPathComponent("library-\(tid).png")
    }

    /// 下载并缓存某个条目的原始 64×64 皮肤，返回本地文件 URL。
    func localTexture(for item: SkinLibraryItem) async throws -> URL {
        let file = cacheFile(tid: item.tid)
        if FileManager.default.fileExists(atPath: file.path) { return file }
        let hash = try await self.hash(for: item.tid)
        guard let url = URL(string: base + "/textures/\(hash)") else { throw SkinLibraryError.badBase }
        let data = try await get(url)
        guard !data.isEmpty else { throw SkinLibraryError.parse }
        try? FileManager.default.createDirectory(at: Paths.skinsDir, withIntermediateDirectories: true)
        try data.write(to: file, options: .atomic)
        return file
    }

    // MARK: - 网络

    private func get(_ url: URL) async throws -> Data {
        let (data, resp) = try await session.data(from: url)
        if let http = resp as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            throw SkinLibraryError.http(http.statusCode)
        }
        return data
    }
}
