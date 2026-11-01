import SwiftUI
import AppKit

// MARK: - 安装页本地 UI 状态

/// 「安装」页的全部可变状态。
/// 本机 SDK 缺少 SwiftUIMacros 插件，不能使用 @State，故统一收敛到一个 ObservableObject。
final class InstallUI: ObservableObject {
    /// 子页：game = 游戏版本，loader = 模组加载器
    @Published var mode: String = "game"

    // —— 游戏版本 ——
    @Published var includeSnapshots: Bool = false
    @Published var remoteVersions: [RemoteVersion] = []
    @Published var installedIDs: Set<String> = []
    @Published var loadingRemote: Bool = false
    @Published var installingVersion: String?

    // —— 模组加载器 ——
    @Published var baseGame: String = ""
    @Published var loaderKind: LoaderKind = .fabric
    @Published var loaderVersions: [String] = []
    @Published var loaderVersion: String = ""
    @Published var fetchingLoader: Bool = false
    @Published var installingLoader: Bool = false
    @Published var loaderResult: String = ""

    // —— 进度与日志 ——
    @Published var progress: Double = 0
    @Published var progressText: String = ""
    @Published var logs: [String] = []
    @Published var error: String?

    /// 是否有安装任务正在执行（用于禁用按钮）。
    var busy: Bool { installingVersion != nil || installingLoader }

    /// 追加一行日志，最多保留 200 行。
    func appendLog(_ line: String) {
        logs.append(line)
        if logs.count > 200 { logs.removeFirst(logs.count - 200) }
    }
}

// MARK: - 安装页

/// 安装页：左侧切换「游戏版本」与「模组加载器」两块功能。
struct InstallView: View {
    @EnvironmentObject var app: AppState
    @StateObject private var ui = InstallUI()

    /// 含 @StateObject 的 View 合成 init 为 private，必须显式声明 internal init。
    init() {}

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                Text("安装").font(.largeTitle.bold())
                Spacer()
                GlassSegmented(options: [(title: "游戏版本", value: "game"),
                                         (title: "模组加载器", value: "loader")],
                               selection: ui.bind(\.mode))
                    .frame(width: 250)
            }

            if ui.mode == "game" {
                gamePane
            } else {
                loaderPane
            }

            if let err = ui.error {
                Text(err).font(.caption).foregroundStyle(.red)
            }

            if ui.busy || !ui.progressText.isEmpty {
                progressPane
            }

            if !ui.logs.isEmpty {
                logPane
            }

            Spacer(minLength: 0)
        }
        .padding(20)
        .onAppear(perform: handleAppear)
    }

    // MARK: - 游戏版本

    private var gamePane: some View {
        VStack(alignment: .leading, spacing: 12) {
            GlassPanel(corner: 14, padding: 12) {
                HStack(spacing: 14) {
                    Toggle("显示快照版", isOn: ui.bind(\.includeSnapshots))
                        .toggleStyle(.checkbox)
                        .onChange(of: ui.includeSnapshots) { _, _ in
                            Task { await loadRemote() }
                        }
                    if ui.loadingRemote {
                        ProgressView().controlSize(.small)
                    }
                    Text("\(ui.remoteVersions.count) 个可用版本")
                        .font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    Button("刷新列表") { Task { await loadRemote() } }
                        .buttonStyle(GlassButtonStyle(dense: true))
                        .disabled(ui.loadingRemote || ui.busy)
                }
            }

            GlassPanel(corner: 16, padding: 8) {
                if ui.remoteVersions.isEmpty {
                    Text(ui.loadingRemote ? "正在获取版本列表…" : "暂无版本数据，点击「刷新列表」重试。")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 140)
                } else {
                    List {
                        ForEach(ui.remoteVersions) { v in
                            remoteRow(v)
                                .padding(.vertical, 6)
                                .listRowBackground(Color.clear)
                        }
                    }
                    .listStyle(.inset)
                    .glassList()
                }
            }
        }
    }

    @ViewBuilder
    private func remoteRow(_ v: RemoteVersion) -> some View {
        let installed = ui.installedIDs.contains(v.versionId)
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(v.versionId).font(.headline)
                    GlassBadge(text: releaseLabel(v.type),
                               tint: v.type == "release" ? .green : .orange)
                    if installed { GlassBadge(text: "已安装", tint: .green) }
                }
                Text("发布于 \(shortDate(v.releaseTime))")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button(installed ? "重新安装" : "安装") {
                Task { await installRemote(v) }
            }
            .buttonStyle(GlassButtonStyle(prominent: !installed, dense: true))
            .disabled(ui.busy)
        }
    }

    // MARK: - 模组加载器

    private var loaderPane: some View {
        GlassPanel(corner: 14, padding: 14) {
            VStack(alignment: .leading, spacing: 14) {
                Text("为一个已安装的 Minecraft 版本附加模组加载器；安装后会在版本列表中出现一个新的组合版本。")
                    .font(.caption).foregroundStyle(.secondary)

                HStack(spacing: 14) {
                    Text("基础版本").font(.caption).foregroundStyle(.secondary)
                    if app.versions.isEmpty {
                        Text("未扫描到已安装版本，请先到「版本」页刷新。")
                            .font(.caption).foregroundStyle(.orange)
                    } else {
                        Picker("", selection: ui.bind(\.baseGame)) {
                            ForEach(app.versions) { v in
                                Text(v.title).tag(v.versionId)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 220)
                        .onChange(of: ui.baseGame) { _, _ in
                            ui.loaderVersions = []
                            ui.loaderVersion = ""
                            ui.loaderResult = ""
                        }
                    }
                    Spacer()
                }

                HStack(spacing: 14) {
                    Text("加载器").font(.caption).foregroundStyle(.secondary)
                    Picker("", selection: ui.bind(\.loaderKind)) {
                        ForEach(LoaderKind.allCases) { k in
                            Text(k.displayName).tag(k)
                        }
                    }
                    .labelsHidden()
                    .frame(width: 220)

                    Button("获取版本列表") { Task { await fetchLoaderVersions() } }
                        .buttonStyle(GlassButtonStyle(dense: true))
                        .disabled(ui.fetchingLoader || ui.baseGame.isEmpty)

                    if ui.fetchingLoader {
                        ProgressView().controlSize(.small)
                    }
                    Spacer()
                }

                HStack(spacing: 14) {
                    Text("加载器版本").font(.caption).foregroundStyle(.secondary)
                    if ui.loaderVersions.isEmpty {
                        Text("先点击「获取版本列表」").font(.caption).foregroundStyle(.secondary)
                    } else {
                        Picker("", selection: ui.bind(\.loaderVersion)) {
                            ForEach(ui.loaderVersions, id: \.self) { ver in
                                Text(ver).tag(ver)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 220)
                    }
                    Spacer()
                    Button(ui.installingLoader ? "安装中…" : "安装加载器") {
                        Task { await installLoader() }
                    }
                    .buttonStyle(GlassButtonStyle(prominent: true))
                    .disabled(ui.installingLoader || ui.loaderVersion.isEmpty || ui.baseGame.isEmpty)
                }

                if !ui.loaderResult.isEmpty {
                    Text(ui.loaderResult).font(.caption).foregroundStyle(.green)
                }
            }
        }
    }

    // MARK: - 进度与日志

    private var progressPane: some View {
        GlassPanel(corner: 12, padding: 12) {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(ui.progressText.isEmpty ? "处理中…" : ui.progressText)
                        .font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    Text("\(Int(ui.progress * 100))%")
                        .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
                ProgressView(value: min(max(ui.progress, 0), 1))
            }
        }
    }

    private var logPane: some View {
        GlassPanel(corner: 12, padding: 12) {
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(ui.logs.suffix(8).enumerated()), id: \.offset) { _, line in
                        Text(line)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .frame(maxHeight: 110)
        }
    }

    // MARK: - 交互

    private func handleAppear() {
        if app.versions.isEmpty { app.loadVersionsCache() }
        if ui.baseGame.isEmpty { ui.baseGame = app.versions.first?.versionId ?? "" }
        if ui.remoteVersions.isEmpty && !ui.loadingRemote {
            Task { await loadRemote() }
        }
    }

    private func loadRemote() async {
        await MainActor.run {
            ui.loadingRemote = true
            ui.error = nil
        }
        do {
            let list = try await VersionInstaller.fetchAll(includeSnapshots: ui.includeSnapshots)
            var installed = Set<String>()
            for v in list where VersionInstaller.isInstalled(v.versionId) {
                installed.insert(v.versionId)
            }
            await MainActor.run {
                ui.remoteVersions = list
                ui.installedIDs = installed
                ui.loadingRemote = false
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.error = "获取版本列表失败：\(message)"
                ui.loadingRemote = false
            }
        }
    }

    private func installRemote(_ v: RemoteVersion) async {
        await MainActor.run {
            ui.installingVersion = v.versionId
            ui.progress = 0
            ui.progressText = "准备安装 \(v.versionId)"
            ui.error = nil
            ui.appendLog("开始安装 \(v.versionId)")
        }
        do {
            try await VersionInstaller.install(v.versionId) { p, msg in
                Task { @MainActor in
                    ui.progress = p
                    ui.progressText = msg ?? "下载中 \(Int(p * 100))%"
                }
            }
            await MainActor.run {
                ui.installedIDs.insert(v.versionId)
                ui.installingVersion = nil
                ui.progress = 1
                ui.progressText = "\(v.versionId) 安装完成"
                ui.appendLog("完成：\(v.versionId)")
                app.loadVersionsCache()
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.installingVersion = nil
                ui.error = "安装 \(v.versionId) 失败：\(message)"
                ui.appendLog("失败：\(v.versionId) — \(message)")
            }
        }
    }

    private func fetchLoaderVersions() async {
        let game = ui.baseGame
        guard !game.isEmpty else { return }
        await MainActor.run {
            ui.fetchingLoader = true
            ui.error = nil
            ui.loaderResult = ""
        }
        do {
            let list = try await LoaderInstaller.versions(kind: ui.loaderKind, game: game)
            await MainActor.run {
                ui.loaderVersions = list
                ui.loaderVersion = list.first ?? ""
                ui.fetchingLoader = false
                if list.isEmpty {
                    ui.error = "\(game) 没有可用的 \(ui.loaderKind.displayName) 版本。"
                }
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.fetchingLoader = false
                ui.error = "获取加载器版本失败：\(message)"
            }
        }
    }

    private func installLoader() async {
        let game = ui.baseGame
        let loaderVersion = ui.loaderVersion
        guard !game.isEmpty, !loaderVersion.isEmpty else { return }
        let kind = ui.loaderKind
        await MainActor.run {
            ui.installingLoader = true
            ui.progress = 0
            ui.progressText = "准备安装 \(kind.displayName) \(loaderVersion)"
            ui.error = nil
            ui.loaderResult = ""
            ui.appendLog("开始为 \(game) 安装 \(kind.displayName) \(loaderVersion)")
        }
        do {
            let newId = try await LoaderInstaller.install(kind: kind,
                                                          game: game,
                                                          loaderVersion: loaderVersion) { p, msg in
                Task { @MainActor in
                    ui.progress = p
                    ui.progressText = msg ?? "处理中 \(Int(p * 100))%"
                }
            }
            await MainActor.run {
                ui.installingLoader = false
                ui.progress = 1
                ui.progressText = "加载器安装完成"
                ui.loaderResult = "已创建版本：\(newId)"
                ui.appendLog("完成：\(newId)")
                ui.baseGame = game
                app.loadVersionsCache()
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.installingLoader = false
                ui.error = "安装加载器失败：\(message)"
                ui.appendLog("失败：\(kind.displayName) \(loaderVersion) — \(message)")
            }
        }
    }
}

// MARK: - 小工具

/// 版本类型的中文名称。
private func releaseLabel(_ type: String) -> String {
    switch type {
    case "release": return "正式版"
    case "snapshot": return "快照"
    case "old_beta", "old_alpha": return "旧版"
    default: return type
    }
}

/// 把 ISO8601 发布时间格式化成 yyyy-MM-dd。
private func shortDate(_ raw: String) -> String {
    guard !raw.isEmpty else { return "未知时间" }
    let plain = ISO8601DateFormatter()
    plain.formatOptions = [.withInternetDateTime]
    let fraction = ISO8601DateFormatter()
    fraction.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    guard let date = plain.date(from: raw) ?? fraction.date(from: raw) else { return raw }
    let out = DateFormatter()
    out.dateFormat = "yyyy-MM-dd"
    return out.string(from: date)
}
