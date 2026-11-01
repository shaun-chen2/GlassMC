import Foundation

/// Yggdrasil（外置登录 / 统一通行证）认证管理器。
///
/// 兼容不同服务器对认证路径的差异：
/// - 标准 Mojang：`{root}/authenticate`
/// - Blessing Skin（LittleSkin 等）：`{root}/authserver/authenticate`
/// - Ely.by：`{root}/auth/authenticate`
/// 登录时自动探测可用基址（404 视为路径不存在，继续尝试）。
final class AuthManager {

    // MARK: - 响应结构

    private struct YggResponse: Decodable {
        let accessToken: String
        let clientToken: String
        let availableProfiles: [YggProfile]?
        let selectedProfile: YggProfile?
        let user: YggUser?
    }

    private struct YggProfile: Decodable {
        let id: String
        let name: String
        let properties: [YggProperty]?
    }

    private struct YggProperty: Decodable {
        let name: String
        let value: String
        let signature: String?
    }

    private struct YggUser: Decodable {
        let id: String
        let properties: [YggProperty]?
    }

    struct YggError: LocalizedError {
        let reason: String
        var statusCode: Int? = nil
        var errorDescription: String? { reason }
    }

    // MARK: - 基址候选

    /// 生成认证基址候选（按可能性排序）。若用户已填到 /authserver 或 /auth，则优先它本身。
    static func authCandidates(_ server: AuthServer) -> [String] {
        let base = server.normalizedBase
        var list = [base]
        if !base.hasSuffix("/authserver") && !base.hasSuffix("/auth") {
            list.append(base + "/authserver")
            list.append(base + "/auth")
        }
        return list
    }

    /// 生成 sessionserver 基址候选（贴图查询用）。
    /// Blessing Skin 等站点的 `/sessionserver/...` 挂在 yggdrasil 基址下，
    /// 所以用户即便填的是 `.../authserver` 也要回退到父路径。
    static func sessionBases(_ server: AuthServer) -> [String] {
        var out: [String] = []
        for base in authCandidates(server) {
            var b = base
            if b.hasSuffix("/authserver") {
                b = String(b.dropLast("/authserver".count))
            } else if b.hasSuffix("/auth") {
                b = String(b.dropLast("/auth".count))
            }
            if !out.contains(b) { out.append(b) }
        }
        return out
    }

    // MARK: - 认证

    /// 用账号密码登录到指定认证服务器（自动探测认证基址）。
    static func authenticate(server: AuthServer, username: String, password: String) async throws -> MCAccount {
        let clientToken = UUID().uuidString
        var lastError: Error?
        for base in authCandidates(server) {
            do {
                return try await authenticateAt(base: base, server: server,
                                                username: username, password: password,
                                                clientToken: clientToken)
            } catch let e as YggError where e.statusCode == 404 {
                lastError = e
                continue
            } catch {
                throw error
            }
        }
        throw lastError ?? YggError(reason: "未找到可用的认证接口，请确认服务器 API 基址。")
    }

    private static func authenticateAt(base: String, server: AuthServer,
                                       username: String, password: String,
                                       clientToken: String) async throws -> MCAccount {
        let url = URL(string: "\(base)/authenticate")!
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.timeoutInterval = 30
        let payload: [String: Any] = [
            "agent": ["name": "Minecraft", "version": 1],
            "username": username,
            "password": password,
            "clientToken": clientToken,
            "requestUser": true
        ]
        req.httpBody = try JSONSerialization.data(withJSONObject: payload)

        let (data, resp) = try await URLSession.shared.data(for: req)
        try checkError(data: data, response: resp)
        let r = try JSONDecoder().decode(YggResponse.self, from: data)
        let profile = r.selectedProfile ?? r.availableProfiles?.first
        var (skinURL, variant) = extractSkin(from: profile?.properties ?? r.user?.properties)
        // Blessing Skin 等皮肤站的 authenticate 响应里通常不带 properties，
        // 贴图必须再查一次 sessionserver 才能拿到（否则账号永远显示默认皮肤）。
        if skinURL == nil, let pid = profile?.id {
            let fetched = await profileTextures(base: base, uuid: pid)
            skinURL = fetched.url
            if let v = fetched.variant { variant = v }
        }

        return MCAccount(
            type: "yggdrasil",
            username: profile?.name ?? username,
            uuid: profile?.id,
            accessToken: r.accessToken,
            clientToken: r.clientToken,
            authServerId: server.id.uuidString,
            source: "glassmc",
            needsReauth: false,
            skinURL: skinURL,
            skinVariant: variant
        )
    }

    /// 用已有令牌刷新。
    static func refresh(_ account: MCAccount) async throws -> MCAccount {
        guard let serverId = account.authServerId,
              let server = SettingsStore.shared.authServers.first(where: { $0.id.uuidString == serverId }),
              let token = account.accessToken else {
            throw YggError(reason: "该账号没有可刷新的外置登录令牌。")
        }
        var lastError: Error?
        for base in authCandidates(server) {
            do {
                return try await refreshAt(base: base, account: account, token: token)
            } catch let e as YggError where e.statusCode == 404 {
                lastError = e
                continue
            } catch {
                throw error
            }
        }
        throw lastError ?? YggError(reason: "未找到可用的刷新接口。")
    }

    private static func refreshAt(base: String, account: MCAccount, token: String) async throws -> MCAccount {
        let url = URL(string: "\(base)/refresh")!
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.timeoutInterval = 30
        let payload: [String: Any] = [
            "accessToken": token,
            "clientToken": account.clientToken ?? UUID().uuidString,
            "requestUser": true
        ]
        req.httpBody = try JSONSerialization.data(withJSONObject: payload)

        let (data, resp) = try await URLSession.shared.data(for: req)
        try checkError(data: data, response: resp)
        let r = try JSONDecoder().decode(YggResponse.self, from: data)
        let profile = r.selectedProfile ?? r.availableProfiles?.first
        var (skinURL, variant) = extractSkin(from: profile?.properties ?? r.user?.properties)
        if skinURL == nil, let pid = profile?.id ?? account.uuid {
            let fetched = await profileTextures(base: base, uuid: pid)
            skinURL = fetched.url
            if let v = fetched.variant { variant = v }
        }

        var updated = account
        updated.accessToken = r.accessToken
        updated.clientToken = r.clientToken
        updated.needsReauth = false
        if let p = profile { updated.username = p.name; updated.uuid = p.id }
        if let s = skinURL { updated.skinURL = s }
        if let v = variant { updated.skinVariant = v }
        return updated
    }

    // MARK: - 皮肤解析

    /// 主动向 sessionserver 查询账号当前贴图（皮肤站点换皮肤后用它同步）。
    /// 返回 nil 表示该服务器没有可用的 sessionserver 或账号查不到。
    static func syncSkin(_ account: MCAccount) async -> MCAccount? {
        guard account.type == "yggdrasil",
              let serverId = account.authServerId,
              let server = SettingsStore.shared.authServers.first(where: { $0.id.uuidString == serverId }),
              let uuid = account.uuid, !uuid.isEmpty else { return nil }

        for base in sessionBases(server) {
            let fetched = await profileTextures(base: base, uuid: uuid)
            if let url = fetched.url {
                var updated = account
                updated.skinURL = url
                updated.skinVariant = fetched.variant ?? account.skinVariant
                return updated
            }
        }
        return nil
    }

    /// 查询 `{base}/sessionserver/session/minecraft/profile/{uuid}` 并解析贴图。
    private static func profileTextures(base: String, uuid: String) async -> (url: String?, variant: String?) {
        let id = uuid.replacingOccurrences(of: "-", with: "")
        guard let url = URL(string: "\(base)/sessionserver/session/minecraft/profile/\(id)?unsigned=false") else {
            return (nil, nil)
        }
        var req = URLRequest(url: url)
        req.timeoutInterval = 20
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        guard let (data, resp) = try? await URLSession.shared.data(for: req) else { return (nil, nil) }
        if let http = resp as? HTTPURLResponse, !(200...299).contains(http.statusCode) { return (nil, nil) }
        guard !data.isEmpty,
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let props = root["properties"] as? [[String: Any]] else { return (nil, nil) }
        let list: [YggProperty] = props.compactMap { p in
            guard let n = p["name"] as? String, let v = p["value"] as? String else { return nil }
            return YggProperty(name: n, value: v, signature: p["signature"] as? String)
        }
        return extractSkin(from: list)
    }

    /// 部分站点（尤其 Blessing Skin 旧版）返回 `http://` 贴图地址，
    /// 统一升级为 https，避免传输层直接拒绝明文请求。
    static func normalizeSkinURL(_ raw: String) -> String {
        let s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.hasPrefix("http://") { return "https://" + s.dropFirst("http://".count) }
        return s
    }

    /// 从 Yggdrasil 的 properties 中提取 textures 贴图地址与模型。
    private static func extractSkin(from properties: [YggProperty]?) -> (url: String?, variant: String?) {
        guard let properties else { return (nil, nil) }
        for prop in properties where prop.name == "textures" {
            guard let data = Data(base64Encoded: prop.value),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let textures = json["textures"] as? [String: Any],
                  let skin = textures["SKIN"] as? [String: Any],
                  let url = skin["url"] as? String, !url.isEmpty else { continue }
            let variant = (skin["metadata"] as? [String: Any])?["model"] as? String
            return (normalizeSkinURL(url), variant == "slim" ? "alex" : "steve")
        }
        return (nil, nil)
    }

    // MARK: - 错误处理

    private static func checkError(data: Data, response: URLResponse) throws {
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            let code = http.statusCode
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let err = (json["errorMessage"] as? String) ?? (json["error"] as? String) {
                throw YggError(reason: "认证失败：\(err)", statusCode: code)
            }
            throw YggError(reason: "认证失败（HTTP \(code)）", statusCode: code)
        }
    }
}
