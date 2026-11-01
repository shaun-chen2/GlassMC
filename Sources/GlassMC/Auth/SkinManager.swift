import Foundation
import AppKit

/// 皮肤下载与缓存管理。
final class SkinManager {

    /// 账号键（uuid 优先，离线账号用用户名）。
    private static func key(for account: MCAccount) -> String {
        (account.uuid ?? account.username)
            .replacingOccurrences(of: ":", with: "_")
            .replacingOccurrences(of: "/", with: "_")
    }

    /// 稳定短哈希：把贴图地址编进文件名，站点换了皮肤后缓存自动失效。
    private static func shortHash(_ s: String) -> String {
        var h: UInt64 = 0xcbf2_9ce4_8422_2325
        for b in s.utf8 { h = (h ^ UInt64(b)) &* 0x0000_0100_0000_01b3 }
        return String(h % 0xFFFFFFF, radix: 36)
    }

    /// 该账号的缓存文件路径（以 uuid/用户名 + 贴图地址哈希为键）。
    static func cacheFile(for account: MCAccount) -> URL {
        let k = key(for: account)
        var suffix = ""
        if let url = account.skinURL, !url.isEmpty { suffix = "-" + shortHash(url) }
        return Paths.skinsDir.appendingPathComponent("\(k)\(suffix).png")
    }

    /// 清理该账号的历史皮肤缓存（贴图地址变化后残留的旧文件）。
    static func purgeStale(for account: MCAccount, keeping current: URL?) {
        let k = key(for: account)
        for name in Paths.contentsOf(Paths.skinsDir) {
            guard name.hasPrefix(k), name.hasSuffix(".png") else { continue }
            if let current, name == current.lastPathComponent { continue }
            try? FileManager.default.removeItem(at: Paths.skinsDir.appendingPathComponent(name))
        }
    }

    /// 确保账号的皮肤已下载到本地缓存，返回本地文件 URL。
    /// 离线且无本地皮肤时返回 nil（调用方应使用默认 Steve）。
    static func ensure(_ account: MCAccount) async -> URL? {
        // 1) 本地指定皮肤文件优先（比如从皮肤库套用的那款）
        if let local = account.skinLocalPath, FileManager.default.fileExists(atPath: local) {
            return URL(fileURLWithPath: local)
        }
        // 2) 已缓存则直接复用（文件名已含贴图地址哈希，换皮肤会自动失效）
        let file = cacheFile(for: account)
        if FileManager.default.fileExists(atPath: file.path) {
            return file
        }
        // 3) 从贴图地址下载
        guard let urlStr = account.skinURL, !urlStr.isEmpty,
              let url = URL(string: urlStr) else { return nil }
        do {
            try? FileManager.default.createDirectory(at: Paths.skinsDir, withIntermediateDirectories: true)
            try await DownloadManager.download(url, to: file)
            // 4) 校验确实是图片（部分站点对匿名请求返回 HTML 或空文件）
            guard let data = try? Data(contentsOf: file), data.count > 64,
                  NSImage(data: data) != nil else {
                try? FileManager.default.removeItem(at: file)
                AppState.shared.log("皮肤文件无效（站点可能要求签名链接）：\(url.absoluteString)")
                return nil
            }
            purgeStale(for: account, keeping: file)
            return file
        } catch {
            AppState.shared.log("皮肤下载失败：\(error.localizedDescription)（\(url.absoluteString)）")
            return nil
        }
    }

    /// 刷新（强制重新下载）指定账号的皮肤。
    static func refresh(_ account: MCAccount) async -> URL? {
        let file = cacheFile(for: account)
        try? FileManager.default.removeItem(at: file)
        return await ensure(account)
    }

    /// 为账号设置本地皮肤文件（覆盖式；离线账号与皮肤库套用都走这里）。
    static func setLocalSkin(_ accountID: UUID, path: String) {
        guard let idx = AppState.shared.accounts.firstIndex(where: { $0.id == accountID }) else { return }
        AppState.shared.accounts[idx].skinLocalPath = path
        AppState.shared.saveAccounts()
    }

    /// 清除本地覆盖，恢复显示认证服务器下发的皮肤。
    static func clearLocalSkin(_ accountID: UUID) {
        guard let idx = AppState.shared.accounts.firstIndex(where: { $0.id == accountID }) else { return }
        AppState.shared.accounts[idx].skinLocalPath = nil
        AppState.shared.saveAccounts()
    }
}
