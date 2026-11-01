import SwiftUI
import AppKit
import UniformTypeIdentifiers

// MARK: - 模组页本地 UI 状态

/// 「模组」页的全部可变状态（已装模组 / 在线搜索 / 整合包）。
final class ModsUI: ObservableObject {
    /// 子页：local = 已装模组，search = 在线搜索，pack = 整合包
    @Published var mode: String = "local"

    // —— 已装模组 ——
    @Published var targetVersion: String = ""
    @Published var mods: [InstalledMod] = []
    @Published var modsDirPath: String = ""
    /// 正在后台扫描 mods 目录（283 个 jar 每个都要解压读 toml，必须离开主线程）。
    @Published var scanning: Bool = false

    // —— 在线搜索 ——
    @Published var query: String = ""
    @Published var hits: [MRSearchHit] = []
    @Published var total: Int = 0
    @Published var searching: Bool = false
    @Published var selectedHit: MRSearchHit?
    @Published var versions: [MRVersion] = []
    @Published var loadingVersions: Bool = false
    @Published var installingVersionId: String?
    @Published var dependencyHint: String = ""

    // —— 整合包 ——
    @Published var packs: [ImportedModpack] = []
    @Published var importingPack: Bool = false

    // —— 通用 ——
    @Published var progress: Double = 0
    @Published var progressText: String = ""
    @Published var error: String?

    /// 已启用模组数量。
    var enabledCount: Int { mods.filter { $0.enabled }.count }
}

// MARK: - 模组页

/// 模组页：本地管理、Modrinth 在线安装、整合包导入。
struct ModsView: View {
    @EnvironmentObject var app: AppState
    @StateObject private var ui = ModsUI()

    /// 含 @StateObject 的 View 合成 init 为 private，必须显式声明 internal init。
    init() {}

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                Text("模组").font(.largeTitle.bold())
                Spacer()
                GlassSegmented(options: [(title: "已装模组", value: "local"),
                                         (title: "在线搜索", value: "search"),
                                         (title: "整合包", value: "pack")],
                               selection: ui.bind(\.mode))
                    .frame(width: 330)
            }

            if ui.mode == "local" {
                localPane
            } else if ui.mode == "search" {
                searchPane
            } else {
                packPane
            }

            if let err = ui.error {
                Text(err).font(.caption).foregroundStyle(.red)
            }

            if !ui.progressText.isEmpty && (ui.installingVersionId != nil || ui.importingPack) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(ui.progressText).font(.caption).foregroundStyle(.secondary)
                    ProgressView(value: min(max(ui.progress, 0), 1))
                }
            }

            Spacer(minLength: 0)
        }
        .padding(20)
        .onAppear(perform: handleAppear)
    }

    // MARK: - 已装模组

    private var localPane: some View {
        VStack(alignment: .leading, spacing: 12) {
            GlassPanel(corner: 14, padding: 12) {
                HStack(spacing: 14) {
                    Text("目标版本").font(.caption).foregroundStyle(.secondary)
                    if app.versions.isEmpty {
                        Text("未扫描到已安装版本").font(.caption).foregroundStyle(.orange)
                        Spacer()
                        Button("刷新") { app.loadVersionsCache() }
                            .buttonStyle(GlassButtonStyle(dense: true))
                    } else {
                        Picker("", selection: ui.bind(\.targetVersion)) {
                            ForEach(app.versions) { v in
                                Text(v.title).tag(v.versionId)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 220)

                        if ui.scanning {
                            HStack(spacing: 5) {
                                ProgressView().controlSize(.small)
                                Text("扫描中…").font(.caption).foregroundStyle(.secondary)
                            }
                        } else {
                            Text("共 \(ui.mods.count) 个，启用 \(ui.enabledCount) 个")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button { reloadMods() } label: {
                            Image(systemName: "arrow.clockwise")
                        }
                        .buttonStyle(GlassButtonStyle(dense: true))
                        .help("重新扫描模组目录")
                    }
                }
            }

            GlassPanel(corner: 16, padding: 8) {
                modsList
            }
        }
        .onChange(of: ui.targetVersion) { _, _ in reloadMods() }
    }

    @ViewBuilder
    private var modsList: some View {
        if ui.mods.isEmpty {
            VStack(spacing: 6) {
                Text(ui.scanning ? "正在扫描 mods 目录…" : "该版本暂无模组").foregroundStyle(.secondary)
                if !ui.modsDirPath.isEmpty {
                    Text(ui.modsDirPath).font(.caption2).foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, minHeight: 140)
        } else {
            List {
                ForEach(ui.mods) { m in
                    HStack(spacing: 12) {
                        VStack(alignment: .leading, spacing: 3) {
                            HStack(spacing: 8) {
                                Text(m.name).font(.headline)
                                if !m.enabled { GlassBadge(text: "已禁用", tint: .orange) }
                            }
                            Text("\(m.modId) · v\(m.version)")
                                .font(.caption).foregroundStyle(.secondary)
                            Text(m.fileName)
                                .font(.caption2).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text(sizeText(m.size))
                            .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                        Button(m.enabled ? "禁用" : "启用") { toggle(m) }
                            .buttonStyle(GlassButtonStyle(dense: true))
                        Button("移除") { remove(m) }
                            .buttonStyle(GlassButtonStyle(destructive: true, dense: true))
                    }
                    .padding(.vertical, 6)
                    .listRowBackground(Color.clear)
                }
            }
            .listStyle(.inset)
            .glassList()
        }
    }

    // MARK: - 在线搜索

    private var searchPane: some View {
        VStack(alignment: .leading, spacing: 12) {
            GlassPanel(corner: 14, padding: 12) {
                HStack(spacing: 10) {
                    GlassTextField(placeholder: "搜索 Modrinth 模组…",
                                   text: ui.bind(\.query),
                                   width: 260)
                    Button(ui.searching ? "搜索中…" : "搜索") { Task { await runSearch() } }
                        .buttonStyle(GlassButtonStyle(prominent: true))
                        .disabled(ui.searching)
                    if ui.selectedHit != nil {
                        Button("返回结果") { backToResults() }
                            .buttonStyle(GlassButtonStyle(dense: true))
                    } else if ui.total > 0 {
                        Text("共 \(ui.total) 个结果")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text("目标：\(searchTargetText)")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }

            GlassPanel(corner: 16, padding: 8) {
                if ui.selectedHit != nil {
                    versionList
                } else {
                    hitList
                }
            }

            if !ui.dependencyHint.isEmpty {
                Text(ui.dependencyHint).font(.caption).foregroundStyle(.orange)
            }
        }
    }

    @ViewBuilder
    private var hitList: some View {
        if ui.hits.isEmpty {
            Text(ui.searching ? "正在搜索…" : "输入关键词后点击「搜索」。")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: 140)
        } else {
            List {
                ForEach(ui.hits) { h in
                    Button {
                        Task { await loadVersions(h) }
                    } label: {
                        HStack(spacing: 12) {
                            VStack(alignment: .leading, spacing: 3) {
                                HStack(spacing: 8) {
                                    Text(h.title).font(.headline)
                                    GlassBadge(text: "\(downloadText(h.downloads)) 下载", tint: .blue)
                                }
                                Text(h.description)
                                    .font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                Text("作者：\(h.author)")
                                    .font(.caption2).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Image(systemName: "chevron.right").foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 6)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .listRowBackground(Color.clear)
                }
            }
            .listStyle(.inset)
            .glassList()
        }
    }

    @ViewBuilder
    private var versionList: some View {
        if ui.loadingVersions {
            Text("正在获取可用版本…")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: 140)
        } else if ui.versions.isEmpty {
            Text("该模组没有匹配当前游戏版本 / 加载器的文件。")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: 140)
        } else {
            List {
                ForEach(ui.versions) { v in
                    HStack(spacing: 12) {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack(spacing: 8) {
                                Text(v.versionNumber).font(.headline)
                                ForEach(v.loaders, id: \.self) { l in
                                    GlassBadge(text: l, tint: .purple)
                                }
                            }
                            Text("游戏版本：\(gameVersionText(v.gameVersions))")
                                .font(.caption).foregroundStyle(.secondary)
                            if let f = v.files.first {
                                Text("\(f.filename) · \(sizeText(Int64(f.size)))")
                                    .font(.caption2).foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        if ui.installingVersionId == v.id {
                            ProgressView(value: min(max(ui.progress, 0), 1))
                                .frame(width: 90)
                        }
                        Button("安装") { Task { await install(v) } }
                            .buttonStyle(GlassButtonStyle(prominent: true, dense: true))
                            .disabled(ui.installingVersionId != nil)
                    }
                    .padding(.vertical, 6)
                    .listRowBackground(Color.clear)
                }
            }
            .listStyle(.inset)
            .glassList()
        }
    }

    // MARK: - 整合包

    private var packPane: some View {
        VStack(alignment: .leading, spacing: 12) {
            GlassPanel(corner: 14, padding: 12) {
                HStack(spacing: 10) {
                    Button(ui.importingPack ? "导入中…" : "导入 .mrpack…") { choosePack() }
                        .buttonStyle(GlassButtonStyle(prominent: true))
                        .disabled(ui.importingPack)
                    Button("查看已导入") { ui.packs = ModpackManager.listImportedPacks() }
                        .buttonStyle(GlassButtonStyle(dense: true))
                    if ui.importingPack {
                        ProgressView(value: min(max(ui.progress, 0), 1))
                            .frame(width: 140)
                    }
                    Spacer()
                    Text("共 \(ui.packs.count) 个整合包实例")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }

            GlassPanel(corner: 16, padding: 8) {
                if ui.packs.isEmpty {
                    Text("尚未导入整合包。点击「导入 .mrpack…」选择文件。")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 140)
                } else {
                    List {
                        ForEach(ui.packs) { p in
                            HStack(spacing: 12) {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(p.name).font(.headline)
                                    Text("\(p.gameVersion) · \(p.loader) \(p.loaderVersion)")
                                        .font(.caption).foregroundStyle(.secondary)
                                    Text(p.instanceId)
                                        .font(.caption2).foregroundStyle(.secondary)
                                }
                                Spacer()
                                GlassBadge(text: "\(p.modCount) 模组", tint: .blue)
                            }
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

    // MARK: - 派生信息

    /// 当前搜索使用的基础游戏版本。
    private var searchGame: String { ui.targetVersion }

    /// 当前搜索使用的加载器（取自目标版本的 modLoader，缺省 fabric）。
    private var searchLoader: String {
        let raw = (app.versions.first { $0.versionId == ui.targetVersion }?.modLoader ?? "")
            .replacingOccurrences(of: " ", with: "")
            .lowercased()
        switch raw {
        case "fabric", "quilt", "neoforge", "forge":
            return raw
        default:
            return "fabric"
        }
    }

    private var searchTargetText: String {
        guard !ui.targetVersion.isEmpty else { return "未选择版本" }
        return "\(ui.targetVersion) · \(searchLoader)"
    }

    // MARK: - 交互

    private func handleAppear() {
        if app.versions.isEmpty { app.loadVersionsCache() }
        if ui.targetVersion.isEmpty {
            ui.targetVersion = app.versions.first?.versionId ?? ""
        }
        if ui.mods.isEmpty { reloadMods() }
        if ui.packs.isEmpty { ui.packs = ModpackManager.listImportedPacks() }
    }

    /// 重新读取目标版本的 mods 目录。
    /// 注意：扫描要逐个解压 jar 读 neoforge.mods.toml，几百个模组时同步执行会卡死首帧，
    /// 所以必须丢到后台线程，回来再更新 UI。
    private func reloadMods() {
        guard !ui.targetVersion.isEmpty else {
            ui.mods = []
            ui.modsDirPath = ""
            return
        }
        guard let dir = LocalMods.modsDirectory(versionId: ui.targetVersion) else {
            ui.mods = []
            ui.modsDirPath = "未找到该版本的 mods 目录"
            return
        }
        ui.modsDirPath = dir.path
        ui.scanning = true
        let version = ui.targetVersion
        Task.detached(priority: .userInitiated) {
            let scanned = LocalMods.scan(dir)
            await MainActor.run {
                // 扫描期间用户可能切了版本，过期结果直接丢弃。
                guard ui.targetVersion == version else { return }
                ui.mods = scanned
                ui.scanning = false
            }
        }
    }

    private func toggle(_ mod: InstalledMod) {
        do {
            try LocalMods.setEnabled(mod, enabled: !mod.enabled)
            ui.error = nil
            reloadMods()
        } catch {
            ui.error = (mod.enabled ? "禁用失败：" : "启用失败：") + error.localizedDescription
        }
    }

    private func remove(_ mod: InstalledMod) {
        do {
            try LocalMods.remove(mod)
            ui.error = nil
            ui.dependencyHint = ""
            reloadMods()
        } catch {
            ui.error = "移除 \(mod.name) 失败：\(error.localizedDescription)"
        }
    }

    private func runSearch() async {
        let keyword = ui.query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !keyword.isEmpty else {
            await MainActor.run { ui.error = "请输入搜索关键词。" }
            return
        }
        guard !ui.targetVersion.isEmpty else {
            await MainActor.run { ui.error = "请先在「已装模组」中选择目标版本。" }
            return
        }
        let game = searchGame
        let loader = searchLoader
        await MainActor.run {
            ui.searching = true
            ui.error = nil
            ui.dependencyHint = ""
            ui.selectedHit = nil
        }
        do {
            let result = try await ModrinthClient.search(query: keyword,
                                                         game: game,
                                                         loader: loader,
                                                         offset: 0,
                                                         limit: 20)
            await MainActor.run {
                ui.hits = result.hits
                ui.total = result.total
                ui.searching = false
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.searching = false
                ui.error = "搜索失败：\(message)"
            }
        }
    }

    private func loadVersions(_ hit: MRSearchHit) async {
        let game = searchGame
        let loader = searchLoader
        await MainActor.run {
            ui.selectedHit = hit
            ui.versions = []
            ui.loadingVersions = true
            ui.error = nil
            ui.dependencyHint = ""
        }
        do {
            let list = try await ModrinthClient.versions(projectId: hit.projectId,
                                                         game: game,
                                                         loader: loader)
            await MainActor.run {
                ui.versions = list
                ui.loadingVersions = false
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.loadingVersions = false
                ui.error = "获取版本失败：\(message)"
            }
        }
    }

    private func backToResults() {
        ui.selectedHit = nil
        ui.versions = []
        ui.dependencyHint = ""
    }

    private func install(_ v: MRVersion) async {
        guard let dir = LocalMods.modsDirectory(versionId: ui.targetVersion) else {
            await MainActor.run { ui.error = "未找到目标版本的 mods 目录。" }
            return
        }
        guard let file = v.files.first else {
            await MainActor.run { ui.error = "该模组版本没有可下载的文件。" }
            return
        }
        await MainActor.run {
            ui.installingVersionId = v.id
            ui.progress = 0
            ui.progressText = "准备下载 \(file.filename)"
            ui.error = nil
            ui.dependencyHint = ""
        }
        do {
            _ = try await ModrinthClient.download(file, toDirectory: dir) { p, msg in
                Task { @MainActor in
                    ui.progress = p
                    ui.progressText = msg ?? "下载中 \(Int(p * 100))%"
                }
            }
            // 依赖检查复用已扫描结果，避免在主线程再解压几百个 jar。
            let installedIDs = Set(ui.mods.map { $0.modId })
            let missing = LocalMods.missingDependencies(forMRVersion: v, installed: installedIDs)
            let hint = missing.isEmpty
                ? ""
                : "需要额外依赖：" + missing.map { $0.projectId ?? $0.versionId ?? "未知" }.joined(separator: "、")
            await MainActor.run {
                reloadMods()
                ui.installingVersionId = nil
                ui.progress = 1
                ui.progressText = "已安装 \(file.filename)"
                ui.dependencyHint = hint
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.installingVersionId = nil
                ui.progressText = ""
                ui.error = "安装 \(v.versionNumber) 失败：\(message)"
            }
        }
    }

    private func choosePack() {
        let panel = NSOpenPanel()
        panel.title = "选择整合包文件"
        panel.prompt = "导入"
        if let type = UTType(filenameExtension: "mrpack") {
            panel.allowedContentTypes = [type]
        } else {
            panel.allowedContentTypes = [.data]
        }
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await importPack(url) }
    }

    private func importPack(_ url: URL) async {
        await MainActor.run {
            ui.importingPack = true
            ui.progress = 0
            ui.progressText = "正在解析 \(url.lastPathComponent)"
            ui.error = nil
        }
        do {
            let pack = try await ModpackManager.importPack(url) { p, msg in
                Task { @MainActor in
                    ui.progress = p
                    ui.progressText = msg ?? "导入中 \(Int(p * 100))%"
                }
            }
            await MainActor.run {
                ui.importingPack = false
                ui.progress = 1
                ui.progressText = "整合包《\(pack.name)》导入完成"
                ui.packs = ModpackManager.listImportedPacks()
                app.loadVersionsCache()
            }
        } catch {
            let message = error.localizedDescription
            await MainActor.run {
                ui.importingPack = false
                ui.progressText = ""
                ui.error = "导入整合包失败：\(message)"
            }
        }
    }

    // MARK: - 格式化

    private func sizeText(_ bytes: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: max(bytes, 0), countStyle: .file)
    }

    private func downloadText(_ count: Int) -> String {
        if count >= 10_000 {
            return String(format: "%.1f 万", Double(count) / 10_000.0)
        }
        return "\(count)"
    }

    private func gameVersionText(_ list: [String]) -> String {
        guard list.count > 4 else { return list.joined(separator: ", ") }
        return list.prefix(4).joined(separator: ", ") + " …"
    }
}
