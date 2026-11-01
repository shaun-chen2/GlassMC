import Foundation

/// 全局目录约定。游戏数据与 HMCL/官方启动器保持一致放在用户目录下，
/// 方便导入时直接复用既有结构。
enum Paths {
    static let fileManager = FileManager.default

    static let appSupport: URL = {
        fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    }()

    /// GlassMC 数据根目录：~/Library/Application Support/GlassMC
    static let dataRoot = appSupport.appendingPathComponent("GlassMC", isDirectory: true)
    /// 游戏目录（等价于官方启动器的 .minecraft）
    static let gameDir = dataRoot.appendingPathComponent("minecraft", isDirectory: true)
    static let javaDir = dataRoot.appendingPathComponent("java", isDirectory: true)
    static let versionsDir = gameDir.appendingPathComponent("versions", isDirectory: true)
    static let librariesDir = gameDir.appendingPathComponent("libraries", isDirectory: true)
    static let assetsDir = gameDir.appendingPathComponent("assets", isDirectory: true)
    static let nativesDir = gameDir.appendingPathComponent("natives", isDirectory: true)

    static let accountsFile = dataRoot.appendingPathComponent("accounts.json")
    static let versionsCacheFile = dataRoot.appendingPathComponent("versions_cache.json")
    static let authServersFile = dataRoot.appendingPathComponent("auth_servers.json")

    /// 皮肤缓存目录（按 uuid/用户名 缓存 PNG）。
    static let skinsDir = dataRoot.appendingPathComponent("skins", isDirectory: true)

    /// 确保所需的目录都存在。
    static func ensureDirectories() {
        for dir in [dataRoot, gameDir, javaDir, versionsDir, librariesDir, assetsDir, nativesDir, skinsDir] {
            try? fileManager.createDirectory(at: dir, withIntermediateDirectories: true)
        }
    }

    /// 读取一个目录下的直接子项名称（仅文件夹/文件基础名）。
    static func contentsOf(_ url: URL) -> [String] {
        (try? fileManager.contentsOfDirectory(atPath: url.path)) ?? []
    }

    /// 判断路径是否存在且为目录。
    static func isDirectory(_ path: String) -> Bool {
        var isDir: ObjCBool = false
        return fileManager.fileExists(atPath: path, isDirectory: &isDir) && isDir.boolValue
    }
}
