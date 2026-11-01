import Foundation

// MARK: - 设备码

/// 微软 OAuth 2.0 设备码流程返回的一次性登录凭据。
///
/// 用户需要在浏览器打开 `verificationURI` 并输入 `userCode` 完成授权，
/// 应用侧则按 `interval` 秒轮询换取令牌。
struct MSDeviceCode {
    /// 设备码（内部用，轮询时提交，不要展示给用户）。
    let deviceCode: String
    /// 用户码（展示给用户输入）。
    let userCode: String
    /// 授权页面地址（一般是 https://www.microsoft.com/link）。
    let verificationURI: String
    /// 设备码有效期（秒）。
    let expiresIn: Int
    /// 建议轮询间隔（秒）。
    let interval: Int
}

// MARK: - 错误

/// 微软正版登录过程中可能出现的错误，全部提供中文描述。
enum MicrosoftAuthError: LocalizedError {
    /// 微软 OAuth 接口返回的 error / error_description。
    case oauth(String)
    /// 设备码已过期（用户未在有效期内完成授权）。
    case codeExpired
    /// 用户在授权页面点击了拒绝。
    case declined
    /// Xbox / XSTS 授权失败，附带 XErr 错误码。
    case xsts(Int64?, String)
    /// 该微软账号没有购买 Minecraft: Java Edition。
    case noGameOwnership
    /// 本地没有可用刷新令牌，必须重新走一次设备码登录。
    case needsRelogin
    /// 服务端返回内容不符合预期（字段缺失、HTTP 异常等）。
    case badResponse(String)

    var errorDescription: String? {
        switch self {
        case .oauth(let message):
            return "微软账号授权失败：\(message)"
        case .codeExpired:
            return "登录代码已过期，请重新发起微软账号登录。"
        case .declined:
            return "你已在授权页面拒绝了本次登录。"
        case .xsts(let code, let message):
            if let code {
                return "\(message)（XErr=\(code)）"
            }
            return message
        case .noGameOwnership:
            return "该微软账号没有购买 Minecraft: Java Edition，无法用于正版登录。"
        case .needsRelogin:
            return "登录凭据已失效，请重新添加微软账号。"
        case .badResponse(let message):
            return "服务端返回异常：\(message)"
        }
    }
}

// MARK: - 微软登录

/// 微软正版账号登录（OAuth 2.0 Device Code 流程）。
///
/// 完整链路：设备码 → 轮询换取微软令牌 → Xbox Live（XBL）→ XSTS → Minecraft 服务 → 游戏档案。
/// 仅依赖 Foundation / URLSession，无需第三方库，也不需要客户端密钥（公共客户端）。
enum MicrosoftAuth {

    // MARK: 常量

    /// 旧版 Xbox Live 公共客户端 id。
    /// 实测该 id 在 v2.0 consumers 端点已不可用（AADSTS700016），
    /// 所以真正的可用 id 必须由用户/开发者自行在 Azure 注册（免费，需勾选「允许公共客户端流」）。
    /// 这里仅作占位，实际优先读设置里的 `msClientID`。
    static let defaultClientID = "00000000402b5328"

    /// 生效的客户端 id：优先用设置页填写的，其次回退占位值。
    static var effectiveClientID: String {
        let manual = SettingsStore.shared.msClientID.trimmingCharacters(in: .whitespacesAndNewlines)
        return manual.isEmpty ? defaultClientID : manual
    }

    /// 是否已配置可用的客户端 id（未配置时 UI 应给出引导）。
    static var isConfigured: Bool {
        !SettingsStore.shared.msClientID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private static let userAgent = "GlassMC/1.0 (macOS)"
    // 注意：租户必须是 consumers（复数），写成 consumer 会返回 AADSTS900023。
    private static let deviceCodeURL = URL(string: "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")!
    private static let tokenURL = URL(string: "https://login.microsoftonline.com/consumers/oauth2/v2.0/token")!
    private static let xblURL = URL(string: "https://user.auth.xboxlive.com/user/authenticate")!
    private static let xstsURL = URL(string: "https://xsts.auth.xboxlive.com/xsts/authorize")!
    private static let mcLoginURL = URL(string: "https://api.minecraftservices.com/authentication/login_with_xbox")!
    private static let profileURL = URL(string: "https://api.minecraftservices.com/minecraft/profile")!
    private static let scope = "XboxLive.signin offline_access"

    // MARK: 设备码

    /// 申请设备码。调用方应把 `userCode` 与 `verificationURI` 展示给用户。
    static func requestDeviceCode(clientID: String = MicrosoftAuth.effectiveClientID) async throws -> MSDeviceCode {
        try Task.checkCancellation()

        let (data, status) = try await postForm(deviceCodeURL, [
            "client_id": clientID,
            "scope": scope
        ])

        guard status == 200 else {
            let payload = try? JSONDecoder().decode(OAuthFailure.self, from: data)
            let reason = payload?.errorDescription ?? payload?.error ?? "HTTP \(status)"
            throw MicrosoftAuthError.oauth(reason)
        }

        guard let payload = try? JSONDecoder().decode(DeviceCodeResponse.self, from: data),
              let deviceCode = payload.deviceCode,
              let userCode = payload.userCode,
              let verificationURI = payload.verificationURI else {
            throw MicrosoftAuthError.badResponse("设备码响应缺少必要字段。")
        }

        return MSDeviceCode(
            deviceCode: deviceCode,
            userCode: userCode,
            verificationURI: verificationURI,
            expiresIn: payload.expiresIn ?? 900,
            interval: max(payload.interval ?? 5, 1)
        )
    }

    // MARK: 轮询并登录

    /// 轮询令牌接口，成功后再走 Xbox Live → XSTS → Minecraft → 档案，返回可用的正版账号。
    ///
    /// - Parameters:
    ///   - clientID: 客户端 id，默认使用 Xbox Live 公共客户端。
    ///   - code: `requestDeviceCode` 返回的设备码。
    ///   - onStatus: 阶段性中文提示，供 UI 展示进度。
    static func pollForAccount(clientID: String = MicrosoftAuth.effectiveClientID,
                               code: MSDeviceCode,
                               onStatus: @escaping (String) -> Void) async throws -> MCAccount {
        let deadline = Date().addingTimeInterval(TimeInterval(max(code.expiresIn, 1)))
        var interval = max(code.interval, 1)
        var msAccessToken: String?
        var msRefreshToken: String?

        onStatus("正在等待浏览器中的微软账号授权…")

        polling: while true {
            try Task.checkCancellation()
            if Date() >= deadline { throw MicrosoftAuthError.codeExpired }

            let (data, status) = try await postForm(tokenURL, [
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "client_id": clientID,
                "device_code": code.deviceCode
            ])

            let payload = try? JSONDecoder().decode(TokenResponse.self, from: data)

            if status == 200, let token = payload?.accessToken {
                msAccessToken = token
                msRefreshToken = payload?.refreshToken
                break polling
            }

            switch payload?.error {
            case "authorization_pending":
                break                       // 用户还没完成授权，继续等
            case "slow_down":
                interval += 5               // 官方要求降速
            case "expired_token":
                throw MicrosoftAuthError.codeExpired
            case "authorization_declined":
                throw MicrosoftAuthError.declined
            default:
                throw MicrosoftAuthError.oauth(payload?.errorDescription ?? payload?.error ?? "HTTP \(status)")
            }

            let remaining = deadline.timeIntervalSinceNow
            if remaining <= 0 { throw MicrosoftAuthError.codeExpired }
            let wait = min(TimeInterval(interval), remaining)
            try await Task.sleep(nanoseconds: UInt64(wait * 1_000_000_000))
        }

        guard let accessToken = msAccessToken else { throw MicrosoftAuthError.codeExpired }

        onStatus("微软账号已授权，正在验证 Xbox Live…")
        let profile = try await completeLogin(msAccessToken: accessToken, onStatus: onStatus)

        let accountID = UUID()
        if let refreshToken = msRefreshToken {
            saveRefreshToken(refreshToken, for: accountID)
        }

        onStatus("登录完成：\(profile.name)")
        return MCAccount(
            id: accountID,
            type: "microsoft",
            username: profile.name,
            uuid: profile.uuid,
            accessToken: profile.accessToken,
            source: "glassmc",
            needsReauth: false,
            skinURL: profile.skinURL,
            skinVariant: profile.skinVariant
        )
    }

    // MARK: 刷新

    /// 用本地保存的刷新令牌续期，并重新换取 Minecraft 令牌。
    ///
    /// `MCAccount` 本身不保存刷新令牌（避免账号文件泄露），刷新令牌单独存放在 UserDefaults。
    /// 没有可用刷新令牌时抛 `needsRelogin`，调用方应引导用户重新登录。
    static func refresh(_ account: MCAccount, clientID: String = MicrosoftAuth.effectiveClientID) async throws -> MCAccount {
        try Task.checkCancellation()

        guard let refreshToken = refreshToken(for: account.id) else {
            throw MicrosoftAuthError.needsRelogin
        }

        let (data, status) = try await postForm(tokenURL, [
            "client_id": clientID,
            "grant_type": "refresh_token",
            "refresh_token": refreshToken,
            "scope": scope
        ])

        let payload = try? JSONDecoder().decode(TokenResponse.self, from: data)

        guard status == 200, let msAccessToken = payload?.accessToken else {
            // invalid_grant 表示刷新令牌已作废（改密码、撤销授权、长期未用等）。
            if status == 400 || status == 401 || payload?.error == "invalid_grant" {
                throw MicrosoftAuthError.needsRelogin
            }
            throw MicrosoftAuthError.oauth(payload?.errorDescription ?? payload?.error ?? "HTTP \(status)")
        }

        if let newRefreshToken = payload?.refreshToken {
            saveRefreshToken(newRefreshToken, for: account.id)
        }

        let profile = try await completeLogin(msAccessToken: msAccessToken, onStatus: { _ in })

        var updated = account
        updated.type = "microsoft"
        updated.username = profile.name
        updated.uuid = profile.uuid
        updated.accessToken = profile.accessToken
        updated.source = "glassmc"
        updated.needsReauth = false
        updated.skinURL = profile.skinURL ?? account.skinURL
        updated.skinVariant = profile.skinVariant ?? account.skinVariant
        return updated
    }

    // MARK: 刷新令牌存取

    /// 保存刷新令牌（按账号 id 隔离）。
    static func saveRefreshToken(_ token: String, for accountID: UUID) {
        UserDefaults.standard.set(token, forKey: refreshKey(accountID))
    }

    /// 读取刷新令牌。
    static func refreshToken(for accountID: UUID) -> String? {
        UserDefaults.standard.string(forKey: refreshKey(accountID))
    }

    /// 退出登录：删除该账号保存的刷新令牌。
    static func signOut(_ account: MCAccount) {
        UserDefaults.standard.removeObject(forKey: refreshKey(account.id))
    }

    private static func refreshKey(_ accountID: UUID) -> String {
        "msRefresh-\(accountID.uuidString)"
    }

    // MARK: - 登录链路（XBL → XSTS → Minecraft → 档案）

    /// 已解析出的游戏档案。
    private struct GameProfile {
        let name: String
        let uuid: String
        let accessToken: String
        let skinURL: String?
        let skinVariant: String?
    }

    /// 用微软 access token 依次换取 XBL / XSTS / Minecraft 令牌并读取档案。
    private static func completeLogin(msAccessToken: String,
                                      onStatus: (String) -> Void) async throws -> GameProfile {
        // 1) Xbox Live 用户认证
        onStatus("正在验证 Xbox Live 账号…")
        let xblBody: [String: Any] = [
            "Properties": [
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": "d=\(msAccessToken)"
            ],
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        ]
        let (xblData, xblStatus) = try await postJSON(xblURL, xblBody)
        guard xblStatus == 200 else {
            throw MicrosoftAuthError.badResponse("Xbox Live 认证失败（HTTP \(xblStatus)）。")
        }
        let xbl = try decodeXboxToken(xblData, stage: "Xbox Live")

        // 2) XSTS 授权
        onStatus("正在获取 XSTS 授权…")
        let xstsBody: [String: Any] = [
            "Properties": [
                "SandboxId": "RETAIL",
                "UserTokens": [xbl.token]
            ],
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        ]
        let (xstsData, xstsStatus) = try await postJSON(xstsURL, xstsBody)
        if xstsStatus == 401 {
            throw xstsError(from: xstsData)
        }
        guard xstsStatus == 200 else {
            throw MicrosoftAuthError.badResponse("XSTS 授权失败（HTTP \(xstsStatus)）。")
        }
        let xsts = try decodeXboxToken(xstsData, stage: "XSTS")

        // 3) 换取 Minecraft 访问令牌
        onStatus("正在登录 Minecraft 服务…")
        let (mcData, mcStatus) = try await postJSON(mcLoginURL, [
            "identityToken": "XBL3.0 x=\(xsts.uhs);\(xsts.token)"
        ])
        guard mcStatus == 200,
              let mcPayload = try? JSONDecoder().decode(MCLoginResponse.self, from: mcData),
              let mcToken = mcPayload.accessToken else {
            throw MicrosoftAuthError.badResponse("Minecraft 登录失败（HTTP \(mcStatus)）。")
        }

        // 4) 读取游戏档案
        onStatus("正在读取 Minecraft 档案…")
        let (profileData, profileStatus) = try await getJSON(profileURL, bearer: mcToken)
        if profileStatus == 404 {
            throw MicrosoftAuthError.noGameOwnership
        }
        guard profileStatus == 200,
              let profile = try? JSONDecoder().decode(MCProfileResponse.self, from: profileData) else {
            throw MicrosoftAuthError.badResponse("读取 Minecraft 档案失败（HTTP \(profileStatus)）。")
        }

        let skin = profile.skins?.first
        return GameProfile(
            name: profile.name,
            uuid: profile.id,
            accessToken: mcToken,
            skinURL: skin?.url,
            skinVariant: skin?.variant
        )
    }

    private struct XboxToken {
        let token: String
        let uhs: String
    }

    private static func decodeXboxToken(_ data: Data, stage: String) throws -> XboxToken {
        guard let payload = try? JSONDecoder().decode(XboxTokenResponse.self, from: data) else {
            throw MicrosoftAuthError.badResponse("\(stage) 返回内容无法解析。")
        }
        return XboxToken(token: payload.token, uhs: payload.uhs)
    }

    /// 把 XSTS 的 XErr 翻译成可读的中文说明。
    private static func xstsError(from data: Data) -> MicrosoftAuthError {
        let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        let code = (object["XErr"] as? NSNumber)?.int64Value
        let serverMessage = (object["Message"] as? String) ?? ""

        if let code {
            switch code {
            case 2148916233:
                return .xsts(code, "该微软账号还没有 Xbox 档案，请先在 xbox.com 创建档案后重试。")
            case 2148916235:
                return .xsts(code, "该账号所在地区不支持 Xbox Live，请更换账号或调整网络环境后重试。")
            case 2148916238:
                return .xsts(code, "该账号为未成年账号，需要家长在 Microsoft 家庭组中同意后才能登录。")
            default:
                let detail = serverMessage.isEmpty ? "" : "：\(serverMessage)"
                return .xsts(code, "Xbox 授权被拒绝\(detail)")
            }
        }
        return .xsts(nil, serverMessage.isEmpty ? "Xbox 授权被拒绝。" : serverMessage)
    }

    // MARK: - 网络辅助

    /// `application/x-www-form-urlencoded` POST。
    private static func postForm(_ url: URL, _ params: [String: String]) async throws -> (Data, Int) {
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 30
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.httpBody = formBody(params)
        return try await send(request)
    }

    /// JSON POST，可选 Bearer 令牌。
    private static func postJSON(_ url: URL,
                                 _ body: [String: Any],
                                 bearer: String? = nil) async throws -> (Data, Int) {
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 30
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        if let bearer {
            request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        }
        do {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        } catch {
            throw MicrosoftAuthError.badResponse("请求体构造失败。")
        }
        return try await send(request)
    }

    /// JSON GET，可选 Bearer 令牌。
    private static func getJSON(_ url: URL, bearer: String? = nil) async throws -> (Data, Int) {
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.timeoutInterval = 30
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        if let bearer {
            request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        }
        return try await send(request)
    }

    private static func send(_ request: URLRequest) async throws -> (Data, Int) {
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        return (data, status)
    }

    /// 严格 percent-encode（空格 → %20，冒号 → %3A），符合 x-www-form-urlencoded。
    private static func formBody(_ params: [String: String]) -> Data {
        let unreserved = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")
        let pairs = params.map { key, value -> String in
            let k = key.addingPercentEncoding(withAllowedCharacters: unreserved) ?? key
            let v = value.addingPercentEncoding(withAllowedCharacters: unreserved) ?? value
            return "\(k)=\(v)"
        }
        return Data(pairs.joined(separator: "&").utf8)
    }

    // MARK: - 响应模型

    private struct DeviceCodeResponse: Decodable {
        let deviceCode: String?
        let userCode: String?
        let verificationURI: String?
        let expiresIn: Int?
        let interval: Int?

        enum CodingKeys: String, CodingKey {
            case deviceCode = "device_code"
            case userCode = "user_code"
            case verificationURI = "verification_uri"
            case expiresIn = "expires_in"
            case interval
        }
    }

    private struct TokenResponse: Decodable {
        let accessToken: String?
        let refreshToken: String?
        let error: String?
        let errorDescription: String?

        enum CodingKeys: String, CodingKey {
            case accessToken = "access_token"
            case refreshToken = "refresh_token"
            case error
            case errorDescription = "error_description"
        }
    }

    private struct OAuthFailure: Decodable {
        let error: String?
        let errorDescription: String?

        enum CodingKeys: String, CodingKey {
            case error
            case errorDescription = "error_description"
        }
    }

    private struct XboxTokenResponse: Decodable {
        let token: String
        let uhs: String

        private struct Claims: Decodable {
            let xui: [Entry]
        }

        private struct Entry: Decodable {
            let uhs: String
        }

        private enum CodingKeys: String, CodingKey {
            case token = "Token"
            case displayClaims = "DisplayClaims"
        }

        init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            token = try container.decode(String.self, forKey: .token)
            let claims = try container.decode(Claims.self, forKey: .displayClaims)
            guard let first = claims.xui.first else {
                throw MicrosoftAuthError.badResponse("Xbox 认证响应缺少用户哈希（uhs）。")
            }
            uhs = first.uhs
        }
    }

    private struct MCLoginResponse: Decodable {
        let accessToken: String?

        enum CodingKeys: String, CodingKey {
            case accessToken = "access_token"
        }
    }

    private struct MCProfileResponse: Decodable {
        let id: String
        let name: String
        let skins: [Skin]?

        struct Skin: Decodable {
            let url: String
            let variant: String?
        }
    }
}
