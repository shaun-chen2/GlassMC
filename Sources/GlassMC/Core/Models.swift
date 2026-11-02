import Foundation
import CommonCrypto

/// 一个 Minecraft 账号。
struct MCAccount: Identifiable, Codable, Hashable {
    var id = UUID()
    /// offline / yggdrasil / microsoft / mojang
    var type: String
    var username: String
    var uuid: String?
    /// 登录后获得的访问令牌（外置登录/在线有效；离线可为非空占位）。
    var accessToken: String?
    /// 外置登录的 clientToken，用于令牌刷新。
    var clientToken: String? = nil
    /// 关联的外置认证服务器 id（UUID 字符串），nil 表示非 Yggdrasil 账号。
    var authServerId: String? = nil
    /// 来源：hmcl / official / prism / glassmc
    var source: String
    /// true 表示 token 不可用或缺失，需要重新登录。
    var needsReauth: Bool
    /// 皮肤贴图地址（外置登录返回或手动设置）。
    var skinURL: String? = nil
    /// 模型：steve / alex（slim）。
    var skinVariant: String? = nil
    /// 离线账号可选：本地皮肤文件路径。
    var skinLocalPath: String? = nil

    var displayName: String { username }

    /// 计算 Minecraft 离线 UUID（NameUUIDHash，MD5("OfflinePlayer:名字")，版本号 3、RFC 4122 变体位）。
    static func offlineUUID(for name: String) -> String {
        let input = Data("OfflinePlayer:\(name)".utf8)
        var digest = [UInt8](repeating: 0, count: Int(CC_MD5_DIGEST_LENGTH))
        input.withUnsafeBytes { ptr in
            _ = CC_MD5(ptr.baseAddress, CC_LONG(input.count), &digest)
        }
        digest[6] = (digest[6] & 0x0F) | 0x30
        digest[8] = (digest[8] & 0x3F) | 0x80
        let hx = digest.map { String(format: "%02x", $0) }.joined()
        let a = String(hx.prefix(8))
        let b = String(hx.dropFirst(8).prefix(4))
        let c = String(hx.dropFirst(12).prefix(4))
        let d = String(hx.dropFirst(16).prefix(4))
        let e = String(hx.dropFirst(20).prefix(12))
        return "\(a)-\(b)-\(c)-\(d)-\(e)"
    }
}

/// 一个 Yggdrasil 兼容的认证服务器（外置登录 / 统一通行证）。
/// 例如：LittleSkin、Ely.by、或自建 authlib-injector 服务。
struct AuthServer: Identifiable, Codable, Hashable {
    var id = UUID()
    /// 展示名称，如 "LittleSkin"
    var name: String
    /// Yggdrasil API 基址，如 "https://littleskin.cn/api/yggdrasil"
    var baseURL: String
    /// 是否为内置预设（预设不可删除）。
    var isPreset: Bool

    init(name: String, baseURL: String, isPreset: Bool = false) {
        self.name = name
        self.baseURL = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        self.isPreset = isPreset
    }

    /// 规范化后的基址（确保以 /authenticate 这类路径拼接正确）。
    var normalizedBase: String {
        var s = baseURL
        if s.hasSuffix("/") { s = String(s.dropLast()) }
        return s
    }

    /// 内置预设服务器。
    /// 注意：不同服务器的认证路径不同，AuthManager 会自动探测：
    /// - LittleSkin：{root}/authserver/authenticate
    /// - MiniSkins（Blessing Skin）：{root}/authserver/authenticate
    /// - Ely.by：{root}/auth/authenticate
    /// - 标准 Mojang：{root}/authenticate
    static let presets: [AuthServer] = [
        AuthServer(name: "LittleSkin", baseURL: "https://littleskin.cn/api/yggdrasil", isPreset: true),
        AuthServer(name: "MiniSkins", baseURL: "https://www.mcpeau.com/api/yggdrasil", isPreset: true),
        AuthServer(name: "Ely.by", baseURL: "https://authserver.ely.by", isPreset: true)
    ]
}

/// 一个已安装的游戏版本（可能是原版或带 Mod 加载器）。
struct GameVersion: Identifiable, Codable, Hashable {
    var id = UUID()
    /// 版本号，如 1.21.1
    var versionId: String
    /// release / snapshot / old_alpha / old_beta
    var releaseType: String
    /// forge / fabric / quilt / nil
    var modLoader: String?
    var modCount: Int
    var saveCount: Int
    /// 该版本对应的游戏目录（导入时可能是外部路径）
    var gameDir: String
    /// 是否已导入到 GlassMC 管理目录
    var imported: Bool = false

    var title: String {
        var t = versionId
        if let loader = modLoader { t += " (\(loader))" }
        return t
    }
}

/// 扫描到的可导入来源（某个启动器的某个游戏目录）。
struct ImportCandidate: Identifiable, Codable, Hashable {
    var id = UUID()
    /// HMCL / 官方启动器 / Prism Launcher
    var launcher: String
    /// 启动器配置/根目录
    var rootPath: String
    /// 实际游戏目录（versions/mods/saves 所在）
    var gameDir: String
    var accountCount: Int
    var versionCount: Int
}

/// 侧边栏标签页。
enum Tab: String, CaseIterable, Identifiable {
    case home = "首页"
    case accounts = "账号"
    case versions = "版本"
    case install = "安装"
    case mods = "模组"
    case multiplayer = "联机"
    case `import` = "导入"
    case settings = "设置"
    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .home: return "house.fill"
        case .accounts: return "person.circle.fill"
        case .versions: return "square.stack.3d.up.fill"
        case .install: return "arrow.down.circle.fill"
        case .mods: return "puzzlepiece.extension.fill"
        case .multiplayer: return "person.3.fill"
        case .import: return "arrow.down.doc.fill"
        case .settings: return "gearshape.fill"
        }
    }
}
