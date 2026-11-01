import Foundation

/// 版本清单中的单个版本条目。
struct VersionEntry: Codable, Identifiable, Hashable {
    let id: String
    let type: String
    let url: String
}

/// Mojang / BMCLAPI 版本清单。
struct VersionManifest: Codable {
    let latest: [String: String]
    let versions: [VersionEntry]

    static func fetch() async throws -> VersionManifest {
        try await DownloadManager.getJSON(DownloadSource().manifestURL)
    }
}

/// 扫描本地已安装版本，生成版本列表（供 UI 与导入复用）。
struct LocalVersionScanner {
    func scanInstalled() -> [GameVersion] {
        let vdir = Paths.versionsDir
        guard Paths.isDirectory(vdir.path) else { return [] }
        let items = (try? FileManager.default.contentsOfDirectory(at: vdir, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
        var out: [GameVersion] = []
        for item in items where LauncherImporter.isDir(item) {
            let vid = item.lastPathComponent
            var releaseType = "release"
            var inherits: String?
            let json = item.appendingPathComponent("\(vid).json")
            if let data = try? Data(contentsOf: json),
               let j = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                releaseType = (j["type"] as? String) ?? "release"
                inherits = j["inheritsFrom"] as? String
            }
            let modLoader = detectModLoader(versionId: vid, inherits: inherits)
            let modCount = countItems(Paths.gameDir.appendingPathComponent("mods").path, filesOnly: true)
            let saveCount = countItems(Paths.gameDir.appendingPathComponent("saves").path, filesOnly: false)
            out.append(GameVersion(versionId: vid, releaseType: releaseType, modLoader: modLoader,
                                   modCount: modCount, saveCount: saveCount,
                                   gameDir: Paths.gameDir.path, imported: true))
        }
        return out.sorted { $0.versionId > $1.versionId }
    }

    private func detectModLoader(versionId: String, inherits: String?) -> String? {
        let combined = [versionId, inherits].compactMap { $0 }.joined(separator: " ").lowercased()
        if combined.contains("forge") { return "forge" }
        if combined.contains("fabric") { return "fabric" }
        if combined.contains("quilt") { return "quilt" }
        if combined.contains("neoforge") { return "neoforge" }
        return nil
    }

    private func countItems(_ path: String, filesOnly: Bool) -> Int {
        guard Paths.isDirectory(path) else { return 0 }
        let items = (try? FileManager.default.contentsOfDirectory(atPath: path)) ?? []
        if filesOnly {
            return items.filter { !Paths.isDirectory("\(path)/\($0)") }.count
        }
        return items.filter { Paths.isDirectory("\(path)/\($0)") }.count
    }
}
