import Foundation

// MARK: - 陶瓦联机（Terracotta）数据模型
//
// 这些类型对应 Terracotta 本地 HTTP 服务 /state 的返回。
// 协议文档：https://burningtnt.github.io/Terracotta/http-api/

/// 房间内的一名玩家。
struct TCProfile: Decodable, Identifiable, Hashable {
    var machineID: String
    var name: String
    var vendor: String
    var kind: String

    enum CodingKeys: String, CodingKey {
        case machineID = "machine_id"
        case name, vendor, kind
    }

    var id: String { machineID }

    /// 房主 / 成员 / 本机。
    var roleText: String {
        switch kind {
        case "HOST": return "房主"
        case "GUEST": return "成员"
        default: return "本机"
        }
    }

    /// 成员列表里优先展示「房主」图标。
    var isHost: Bool { kind == "HOST" }
}

/// /state 的一次快照。字段与状态相关，因此大多为可选。
struct TCStatus: Decodable, Equatable {
    var state: String
    var index: Int
    var room: String?
    var url: String?
    var difficulty: String?
    var profiles: [TCProfile]?
    /// 仅 exception 状态存在。
    var type: Int?

    static let idle = TCStatus(state: "waiting", index: -1,
                               room: nil, url: nil, difficulty: nil,
                               profiles: nil, type: nil)

    /// 状态是否属于「房主链路」。
    var isHosting: Bool { state.hasPrefix("host-") }
    /// 状态是否属于「加入链路」。
    var isGuesting: Bool { state.hasPrefix("guest-") }

    /// NAT 连接难度的人话说明。
    var difficultyText: String? {
        switch difficulty {
        case "EASIEST": return "网络很好，几乎秒连"
        case "SIMPLE": return "网络良好，通常几秒内连上"
        case "MEDIUM": return "网络一般，可能要等十几秒"
        case "TOUGH": return "网络较严格，可能需要多试几次"
        case "UNKNOWN": return "正在评估网络环境"
        default: return nil
        }
    }

    /// 异常类型的可读说明。
    var exceptionText: String? {
        guard state == "exception" else { return nil }
        switch type {
        case 0: return "无法连接到房主：房间可能已关闭，或对方网络不稳定。"
        case 1: return "与房主的连接中断了。"
        case 2: return "本机的隧道组件崩溃了，请重试一次。"
        case 3: return "房主那边的隧道组件崩溃了，请让房主重新创建房间。"
        case 4: return "房主的游戏服务器已关闭（可能是退出了存档），房间自动解散。"
        case 5: return "房间协调协议出错，请稍后重试或反馈给开发者。"
        default: return "出现了未知错误。"
        }
    }
}
