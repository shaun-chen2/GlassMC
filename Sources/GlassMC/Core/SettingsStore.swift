import Foundation
import SwiftUI

/// 全局设置，使用 UserDefaults 持久化，并通过 ObservableObject 驱动 UI 刷新。
final class SettingsStore: ObservableObject {
    static let shared = SettingsStore()

    /// 窗口玻璃透明度（0.35 ~ 1.0）。联动 NSWindow.alphaValue。
    @Published var glassOpacity: Double {
        didSet { UserDefaults.standard.set(glassOpacity, forKey: "glassOpacity") }
    }
    /// system / light / dark
    @Published var themeRaw: String {
        didSet { UserDefaults.standard.set(themeRaw, forKey: "themeRaw") }
    }
    /// 游戏文件下载源（HMCL 镜像 BMCLAPI）
    @Published var downloadBase: String {
        didSet { UserDefaults.standard.set(downloadBase, forKey: "downloadBase") }
    }
    /// Java 运行环境下载源（清华大学开源软件镜像站 Adoptium）
    @Published var javaMirror: String {
        didSet { UserDefaults.standard.set(javaMirror, forKey: "javaMirror") }
    }
    /// 已配置的 Yggdrasil 认证服务器列表（外置登录）。
    @Published var authServers: [AuthServer] {
        didSet { saveAuthServers() }
    }
    /// 分配给游戏的堆内存（GB）。
    @Published var memoryGB: Double {
        didSet { UserDefaults.standard.set(memoryGB, forKey: "memoryGB") }
    }
    /// 皮肤库站点基址（Blessing Skin 6 站点，默认 MiniSkins）。
    @Published var skinLibraryBase: String {
        didSet { UserDefaults.standard.set(skinLibraryBase, forKey: "skinLibraryBase") }
    }
    /// 游戏窗口宽度（0 表示用游戏默认值 / 保留版本 JSON 里的设置）。
    @Published var windowWidth: Int {
        didSet { UserDefaults.standard.set(windowWidth, forKey: "windowWidth") }
    }
    /// 游戏窗口高度（0 表示默认）。
    @Published var windowHeight: Int {
        didSet { UserDefaults.standard.set(windowHeight, forKey: "windowHeight") }
    }
    /// 自定义 JVM 参数（空格分隔），追加在版本自带参数之后、主类之前。
    @Published var extraJVMArgs: String {
        didSet { UserDefaults.standard.set(extraJVMArgs, forKey: "extraJVMArgs") }
    }
    /// 微软登录用的 Azure 应用（公共客户端）id。
    /// v2.0 设备码流程要求应用已注册并开启「允许公共客户端流」，旧的 Xbox 公共 id 已失效。
    @Published var msClientID: String {
        didSet { UserDefaults.standard.set(msClientID, forKey: "msClientID") }
    }

    init() {
        let d = UserDefaults.standard
        self.glassOpacity = d.object(forKey: "glassOpacity") as? Double ?? 1.0
        self.themeRaw = d.string(forKey: "themeRaw") ?? "system"
        // BMCLAPI 是 HMCL 默认使用的国内镜像，覆盖 Mojang 原版资源。
        self.downloadBase = d.string(forKey: "downloadBase") ?? "https://bmclapi2.bangbang93.com"
        // 清华源 Adoptium（Temurin）JDK 镜像。
        self.javaMirror = d.string(forKey: "javaMirror") ?? "https://mirrors.tuna.tsinghua.edu.cn/Adoptium"
        // 认证服务器：首次启动写入内置预设，之后以用户配置为准。
        if let data = d.data(forKey: "authServers"),
           let list = try? JSONDecoder().decode([AuthServer].self, from: data) {
            self.authServers = list
        } else {
            self.authServers = AuthServer.presets
        }
        self.memoryGB = d.object(forKey: "memoryGB") as? Double ?? 2.0
        self.skinLibraryBase = d.string(forKey: "skinLibraryBase") ?? "https://www.mcpeau.com"
        // 窗口大小：0 = 不干预，交给游戏自己决定。
        self.windowWidth = d.object(forKey: "windowWidth") as? Int ?? 0
        self.windowHeight = d.object(forKey: "windowHeight") as? Int ?? 0
        self.extraJVMArgs = d.string(forKey: "extraJVMArgs") ?? ""
        self.msClientID = d.string(forKey: "msClientID") ?? ""

        // 老版本保存的服务器列表里可能缺少后续新增的内置预设，这里做一次补齐。
        let known = Set(authServers.map { $0.normalizedBase.lowercased() })
        let missing = AuthServer.presets.filter { !known.contains($0.normalizedBase.lowercased()) }
        if !missing.isEmpty {
            authServers.append(contentsOf: missing)
        }
    }

    private func saveAuthServers() {
        guard let data = try? JSONEncoder().encode(authServers) else { return }
        UserDefaults.standard.set(data, forKey: "authServers")
    }
}
