import Foundation
import CommonCrypto

/// authlib-injector 的下载与缓存。
///
/// 为什么必须要有它：外置登录（Yggdrasil / Blessing Skin 皮肤站）账号在游戏里
/// 是靠 `-javaagent:authlib-injector.jar=<API 基址>` 把客户端的认证与皮肤请求
/// 重定向到站点。没有它，游戏会去问 Mojang 要皮肤（必然失败），
/// 结果就是「登录成功但进游戏是默认皮肤、也进不了该站的正版服」。
final class AuthLibInjector {

    /// 本地 jar 路径：~/Library/Application Support/GlassMC/authlib-injector.jar
    static var jarURL: URL { Paths.dataRoot.appendingPathComponent("authlib-injector.jar") }

    /// 元数据源：BMCLAPI 镜像优先，官方源兜底。
    private static let metaURLs = [
        "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/latest.json",
        "https://authlib-injector.yushi.moe/artifact/latest.json"
    ]

    /// 确保本地有可用的注入器，返回 jar 路径；失败返回 nil（调用方降级为不注入）。
    static func ensure() async -> URL? {
        if isReady { return jarURL }
        for meta in metaURLs {
            guard let metaURL = URL(string: meta),
                  let (data, resp) = try? await URLSession.shared.data(from: metaURL),
                  let http = resp as? HTTPURLResponse, (200...299).contains(http.statusCode),
                  let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let download = root["download_url"] as? String,
                  let version = root["version"] as? String,
                  let jar = URL(string: download) else { continue }

            guard let (bin, jarResp) = try? await URLSession.shared.data(from: jar),
                  let jhttp = jarResp as? HTTPURLResponse, (200...299).contains(jhttp.statusCode),
                  bin.count > 100_000 else { continue }

            if let sums = root["checksums"] as? [String: Any],
               let expected = (sums["sha256"] as? String)?.lowercased(),
               !expected.isEmpty, sha256(bin) != expected {
                AppState.shared.log("authlib-injector 校验失败，忽略该镜像")
                continue
            }

            try? FileManager.default.createDirectory(at: Paths.dataRoot, withIntermediateDirectories: true)
            do {
                try bin.write(to: jarURL, options: .atomic)
            } catch {
                AppState.shared.log("authlib-injector 写入失败：\(error.localizedDescription)")
                return nil
            }
            UserDefaults.standard.set(version, forKey: "authlibInjectorVersion")
            AppState.shared.log("authlib-injector \(version) 已就绪（外置登录所需的皮肤/认证注入）")
            return jarURL
        }
        AppState.shared.log("authlib-injector 下载失败：外置登录账号将无法同步站点皮肤。")
        return nil
    }

    /// 本地 jar 是否已就绪（大小合理即视为可用）。
    static var isReady: Bool {
        guard FileManager.default.fileExists(atPath: jarURL.path),
              let attrs = try? FileManager.default.attributesOfItem(atPath: jarURL.path),
              let size = attrs[.size] as? Int else { return false }
        return size > 100_000
    }

    /// 已安装版本（展示用）。
    static var version: String? { UserDefaults.standard.string(forKey: "authlibInjectorVersion") }

    /// 删除本地 jar（设置页「重新安装」用）。
    static func removeJar() {
        try? FileManager.default.removeItem(at: jarURL)
        UserDefaults.standard.removeObject(forKey: "authlibInjectorVersion")
    }

    private static func sha256(_ data: Data) -> String {
        var digest = [UInt8](repeating: 0, count: Int(CC_SHA256_DIGEST_LENGTH))
        data.withUnsafeBytes { ptr in
            _ = CC_SHA256(ptr.baseAddress, CC_LONG(data.count), &digest)
        }
        return digest.map { String(format: "%02x", $0) }.joined()
    }
}
