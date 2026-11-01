import Foundation

/// 启动器种类。
enum LauncherKind: String {
    case official = "官方启动器"
    case hmcl = "HMCL"
    case prism = "Prism/MultiMC"
}

/// 检测到的可导入启动器（一个来源 = 一个游戏目录）。
struct DetectedLauncher: Identifiable {
    let id = UUID()
    let kind: LauncherKind
    let name: String
    let rootPath: String
    let gameDir: String
    let accountCount: Int
    let versionCount: Int
}

/// 负责扫描本机其他启动器，并把它们的游戏数据合并进 GlassMC。
///
/// 设计取舍（已记入假设）：
/// - 官方启动器 / Prism 的账号凭证存放在系统钥匙串或加密字段中，文件不可直接读取，
///   因此账号统一标记为 needsReauth，导入后由用户在 GlassMC 内重新登录。
/// - HMCL 的 hmcl.json 中若带有可用 token 则尽量迁移复用，否则同样标记重登录。
/// - 游戏文件（versions/mods/saves/resourcepacks/shaderpacks/config）采用合并复制，
///   不会覆盖 GlassMC 已有的同名文件之外的内容。
enum LauncherImporter {

    // MARK: - 检测

    static func detect() -> [DetectedLauncher] {
        var result: [DetectedLauncher] = []
        let home = NSHomeDirectory()
        let appSup = (NSSearchPathForDirectoriesInDomains(.applicationSupportDirectory, .userDomainMask, true).first
                      ?? "\(home)/Library/Application Support")

        // 官方启动器（macOS 默认游戏目录）
        let officialGame = "\(appSup)/minecraft"
        if Paths.isDirectory(officialGame), Paths.isDirectory("\(officialGame)/versions") {
            result.append(DetectedLauncher(
                kind: .official, name: "官方启动器", rootPath: officialGame, gameDir: officialGame,
                accountCount: 0, versionCount: countVersions(in: officialGame)))
        }

        // HMCL：优先使用用户记忆中的 app 内嵌路径，并兜底若干常见位置
        let hmclRoots = [
            "/Applications/HMCL.app/Contents/Resources/.minecraft",
            "\(home)/.minecraft",
            "\(appSup)/HMCL/.minecraft",
            "\(home)/HMCL/.minecraft"
        ]
        for r in hmclRoots where Paths.isDirectory(r) {
            result.append(DetectedLauncher(
                kind: .hmcl, name: "HMCL", rootPath: r, gameDir: r,
                accountCount: hmclAccountCount(gameDir: r),
                versionCount: countVersions(in: r)))
        }

        // Prism / MultiMC / PolyMC
        let prismRoots = [
            "\(appSup)/PrismLauncher",
            "\(appSup)/MultiMC",
            "\(appSup)/PolyMC"
        ]
        for r in prismRoots where Paths.isDirectory(r) {
            let instDir = "\(r)/instances"
            if Paths.isDirectory(instDir) {
                result.append(DetectedLauncher(
                    kind: .prism, name: "Prism/MultiMC", rootPath: r, gameDir: instDir,
                    accountCount: prismAccountCount(root: r),
                    versionCount: prismVersionCount(instancesDir: instDir)))
            }
        }
        return result
    }

    // MARK: - 执行导入

    /// 把来源的版本 / 模组 / 存档 / 资源包合并进 GlassMC 游戏目录。
    /// 返回导入后识别到的游戏版本列表。
    @discardableResult
    static func `import`(_ launcher: DetectedLauncher,
                         progress: (String) -> Void = { _ in }) throws -> [GameVersion] {
        let dst = Paths.gameDir
        try FileManager.default.createDirectory(at: dst, withIntermediateDirectories: true)

        let foldersToMerge = ["versions", "mods", "saves", "resourcepacks", "shaderpacks", "config", "libraries"]

        if launcher.kind == .prism {
            // Prism 每个实例自带 .minecraft，逐个合并
            let instDir = URL(fileURLWithPath: launcher.gameDir)
            let instances = (try? FileManager.default.contentsOfDirectory(at: instDir, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
            for inst in instances where isDir(inst) {
                let mc = inst.appendingPathComponent(".minecraft")
                let src = isDir(mc) ? mc : inst
                progress("导入实例：\(inst.lastPathComponent)")
                try mergeDirectory(from: src, into: dst, onlySubpaths: foldersToMerge)
            }
        } else {
            progress("导入 \(launcher.name) 游戏目录")
            try mergeDirectory(from: URL(fileURLWithPath: launcher.gameDir), into: dst, onlySubpaths: foldersToMerge)
        }

        // 尝试迁移 HMCL 账号（尽力而为）
        if launcher.kind == .hmcl {
            for acc in hmclAccounts(gameDir: launcher.gameDir) {
                AppState.shared.addAccount(acc)
            }
        }

        progress("扫描已导入版本")
        return scanImportedVersions()
    }

    /// 扫描 GlassMC 游戏目录，生成版本列表。
    static func scanImportedVersions() -> [GameVersion] {
        LocalVersionScanner().scanInstalled()
    }

    // MARK: - 私有：版本计数

    static func countVersions(in gameDir: String) -> Int {
        let vdir = URL(fileURLWithPath: "\(gameDir)/versions")
        guard Paths.isDirectory(vdir.path) else { return 0 }
        let items = (try? FileManager.default.contentsOfDirectory(at: vdir, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
        return items.filter { isDir($0) }.count
    }

    static func prismVersionCount(instancesDir: String) -> Int {
        let dir = URL(fileURLWithPath: instancesDir)
        guard Paths.isDirectory(dir.path) else { return 0 }
        let items = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
        return items.filter { isDir($0) }.count
    }

    static func prismAccountCount(root: String) -> Int {
        let f = URL(fileURLWithPath: "\(root)/accounts.json")
        guard let data = try? Data(contentsOf: f),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let accounts = json["accounts"] as? [[String: Any]] else { return 0 }
        return accounts.count
    }

    static func hmclAccountCount(gameDir: String) -> Int {
        hmclAccounts(gameDir: gameDir).count
    }

    /// 解析 HMCL 的 hmcl.json，尽力提取账号信息。
    static func hmclAccounts(gameDir: String) -> [MCAccount] {
        let candidates = [
            "\(gameDir)/hmcl.json",
            "\(URL(fileURLWithPath: gameDir).deletingLastPathComponent().path)/hmcl.json"
        ]
        for path in candidates {
            guard let data = try? Data(contentsOf: URL(fileURLWithPath: path)),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { continue }
            var out: [MCAccount] = []
            // HMCL 较新版本把账号放在 authInfo 数组
            if let auths = json["authInfo"] as? [[String: Any]] {
                for a in auths {
                    out.append(accountFromHMCL(a))
                }
            }
            // 兼容更老的 accounts 字段
            if let accounts = json["accounts"] as? [[String: Any]] {
                for a in accounts { out.append(accountFromHMCL(a)) }
            }
            if !out.isEmpty { return out }
        }
        return []
    }

    private static func accountFromHMCL(_ a: [String: Any]) -> MCAccount {
        let username = (a["username"] as? String) ?? (a["displayName"] as? String) ?? "未知用户"
        let uuid = a["uuid"] as? String
        let token = a["accessToken"] as? String
        let type = (a["type"] as? String) ?? "microsoft"
        let usable = (token != nil && !(token?.isEmpty ?? true))
        return MCAccount(type: type, username: username, uuid: uuid,
                         accessToken: usable ? token : nil,
                         source: "hmcl", needsReauth: !usable)
    }

    // MARK: - 私有：目录合并

    static func isDir(_ url: URL) -> Bool {
        var isDir: ObjCBool = false
        FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir)
        return isDir.boolValue
    }

    /// 将 src 下指定的子路径合并进 dst（已存在的目录递归合并，文件覆盖）。
    static func mergeDirectory(from src: URL, into dst: URL, onlySubpaths: [String]? = nil) throws {
        let fm = FileManager.default
        let children: [URL]
        if let onlySubpaths {
            children = onlySubpaths.map { src.appendingPathComponent($0) }.filter { fm.fileExists(atPath: $0.path) }
        } else {
            children = (try? fm.contentsOfDirectory(at: src, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
        }
        for item in children {
            let destItem = dst.appendingPathComponent(item.lastPathComponent)
            if isDir(item) {
                if fm.fileExists(atPath: destItem.path) {
                    try mergeDirectory(from: item, into: destItem)
                } else {
                    try fm.copyItem(at: item, to: destItem)
                }
            } else {
                if fm.fileExists(atPath: destItem.path) { try fm.removeItem(at: destItem) }
                try fm.copyItem(at: item, to: destItem)
            }
        }
    }
}
