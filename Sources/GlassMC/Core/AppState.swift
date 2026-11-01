import Foundation
import SwiftUI
import AppKit

/// 应用级共享状态：账号、版本、导入候选、日志、主窗口引用。
final class AppState: ObservableObject {
    static let shared = AppState()

    @Published var selectedTab: Tab = .home
    @Published var accounts: [MCAccount] = []
    @Published var versions: [GameVersion] = []
    @Published var importCandidates: [ImportCandidate] = []
    @Published var logLines: [String] = []
    @Published var isBusy: Bool = false
    @Published var launchOutput: String = ""
    /// 当前激活账号（启动游戏时使用）。
    @Published var activeAccountID: UUID? {
        didSet { UserDefaults.standard.set(activeAccountID?.uuidString, forKey: "activeAccountID") }
    }

    /// 主窗口引用，用于联动透明度（alphaValue）。
    weak var mainWindow: NSWindow?

    let settings = SettingsStore.shared

    private init() {
        Paths.ensureDirectories()
        loadAccounts()
        if let s = UserDefaults.standard.string(forKey: "activeAccountID"), let u = UUID(uuidString: s) {
            activeAccountID = u
        }
        if activeAccountID == nil { activeAccountID = accounts.first?.id }
    }

    /// 当前用于启动的账号：优先激活账号，其次第一个账号。
    var activeAccount: MCAccount? {
        if let id = activeAccountID, let acc = accounts.first(where: { $0.id == id }) { return acc }
        return accounts.first
    }

    func setActiveAccount(_ account: MCAccount) {
        activeAccountID = account.id
        log("已切换激活账号：\(account.username)")
    }

    /// 用刷新后的账号替换原记录。
    func replaceAccount(_ account: MCAccount) {
        if let i = accounts.firstIndex(where: { $0.id == account.id }) {
            accounts[i] = account
            saveAccounts()
        }
    }

    // MARK: - 日志

    func log(_ message: String) {
        let ts = DateFormatter.localizedString(from: Date(), dateStyle: .none, timeStyle: .medium)
        DispatchQueue.main.async {
            self.logLines.append("[\(ts)] \(message)")
            if self.logLines.count > 500 { self.logLines.removeFirst() }
        }
    }

    // MARK: - 账号持久化

    func loadAccounts() {
        guard let data = try? Data(contentsOf: Paths.accountsFile) else { return }
        accounts = (try? JSONDecoder().decode([MCAccount].self, from: data)) ?? []
    }

    func saveAccounts() {
        guard let data = try? JSONEncoder().encode(accounts) else { return }
        try? data.write(to: Paths.accountsFile, options: .atomic)
    }

    func addAccount(_ account: MCAccount) {
        accounts.append(account)
        saveAccounts()
    }

    func removeAccount(_ account: MCAccount) {
        accounts.removeAll { $0.id == account.id }
        saveAccounts()
    }

    // MARK: - 版本持久化（导入结果缓存，便于重启后保留）

    func saveVersions() {
        guard let data = try? JSONEncoder().encode(versions) else { return }
        try? data.write(to: Paths.versionsCacheFile, options: .atomic)
    }

    func loadVersionsCache() {
        guard let data = try? Data(contentsOf: Paths.versionsCacheFile) else { return }
        versions = (try? JSONDecoder().decode([GameVersion].self, from: data)) ?? []
    }
}
