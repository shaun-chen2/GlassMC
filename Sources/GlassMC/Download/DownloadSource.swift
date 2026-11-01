import Foundation

/// 下载源构造器。游戏文件走 HMCL 镜像（BMCLAPI），Java 单独配置。
struct DownloadSource {
    let base: String

    init(base: String = SettingsStore.shared.downloadBase) {
        self.base = base.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    /// 版本清单（覆盖 Mojang 原版）。
    var manifestURL: URL {
        URL(string: "\(base)/mc/game/version_manifest_v2.json")!
    }

    /// 某个版本的 version.json。
    func versionJSONURL(_ id: String) -> URL {
        URL(string: "\(base)/versions/\(id)/\(id).json")!
    }

    /// Maven 风格 library 路径。
    func libraryURL(_ path: String) -> URL {
        URL(string: "\(base)/libraries/\(path)")!
    }

    /// 客户端主程序 jar：versions/{id}/{id}.jar
    func clientJarURL(_ id: String) -> URL {
        URL(string: "\(base)/versions/\(id)/\(id).jar")!
    }

    /// 资源索引文件。
    func assetIndexURL(_ index: String) -> URL {
        URL(string: "\(base)/assets/indexes/\(index).json")!
    }

    /// 具体资源对象。
    /// 注意：BMCLAPI 的资源对象路径是 `/assets/<hash 前两位>/<hash>`（**没有** `objects/` 段），
    /// 用官方那种 `/assets/objects/...` 会 404。
    func assetObjectURL(hash: String) -> URL {
        let p = String(hash.prefix(2))
        return URL(string: "\(base)/assets/\(p)/\(hash)")!
    }

    /// 官方资源对象地址（回退用）。
    static func officialAssetObjectURL(hash: String) -> URL {
        let p = String(hash.prefix(2))
        return URL(string: "https://resources.download.minecraft.net/\(p)/\(hash)")!
    }
}
