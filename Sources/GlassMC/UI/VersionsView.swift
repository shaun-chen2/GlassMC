import SwiftUI
import AppKit

/// VersionsView 的本地 UI 状态。
final class VersionsUI: ObservableObject {
    @Published var javaInstalls: [JavaInstall] = []
    @Published var selectedJava: JavaInstall?
    @Published var status = ""
    @Published var detail = ""
    @Published var failed = false
    @Published var busy = false
    @Published var query = ""
    @Published var runningVersion: String?

    func rescanJava(required: Int) {
        javaInstalls = JavaManager.installed()
        if selectedJava == nil || !(selectedJava?.isUsable ?? false) {
            selectedJava = JavaManager.bestMatch(required: required) ?? javaInstalls.first
        }
    }
}

struct VersionsView: View {
    @EnvironmentObject var app: AppState
    @StateObject private var ui = VersionsUI()

    private var filtered: [GameVersion] {
        let q = ui.query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return app.versions }
        return app.versions.filter { $0.title.lowercased().contains(q) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("版本").font(.largeTitle.bold())
                Spacer()
                GlassTextField(placeholder: "搜索版本…", text: ui.bind(\.query), width: 180)
                Button("刷新") { Task { await refresh() } }
                    .buttonStyle(GlassButtonStyle())
            }

            GlassPanel(corner: 14, padding: 12) {
                HStack(spacing: 16) {
                    if let acc = app.activeAccount {
                        HStack(spacing: 8) {
                            SkinHeadView(account: acc, size: 30).frame(width: 28, height: 28)
                            VStack(alignment: .leading, spacing: 1) {
                                Text("激活账号").font(.caption2).foregroundStyle(.secondary)
                                Text(acc.username).font(.system(size: 13, weight: .semibold))
                            }
                        }
                    } else {
                        Text("尚未添加账号").foregroundStyle(.secondary)
                    }
                    Divider().frame(height: 26).opacity(0.3)
                    HStack(spacing: 6) {
                        Text("Java").font(.caption).foregroundStyle(.secondary)
                        Picker("", selection: ui.bind(\.selectedJava)) {
                            ForEach(ui.javaInstalls) { j in
                                Text("Java \(j.version)\(j.managed ? "" : " · 系统")")
                                    .tag(j as JavaInstall?)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 170)
                        Button {
                            ui.rescanJava(required: 0)
                        } label: {
                            Image(systemName: "arrow.clockwise")
                        }
                        .buttonStyle(GlassButtonStyle(dense: true))
                    }
                    Spacer()
                    if !ui.javaInstalls.isEmpty {
                        GlassBadge(text: "\(ui.javaInstalls.count) 个可用",
                                   tint: .green)
                    } else {
                        Text("未检测到 Java，请到「设置」安装")
                            .font(.caption).foregroundStyle(.orange)
                    }
                }
            }

            GlassPanel(corner: 16, padding: 8) {
                List {
                    ForEach(filtered) { v in
                        HStack(spacing: 12) {
                            VStack(alignment: .leading, spacing: 3) {
                                HStack(spacing: 8) {
                                    Text(v.title).font(.headline)
                                    GlassBadge(text: v.releaseType,
                                               tint: v.releaseType == "release" ? .green : .orange)
                                    if let loader = v.modLoader { GlassBadge(text: loader, tint: .purple) }
                                    if LaunchEngine.isIsolatedInstance(v.versionId) {
                                        GlassBadge(text: "隔离实例", tint: .blue)
                                    }
                                }
                                Text("模组 \(v.modCount) · 存档 \(v.saveCount)")
                                    .font(.caption).foregroundStyle(.secondary)
                                if ui.runningVersion == v.versionId {
                                    Text("运行中…").font(.caption).foregroundStyle(.green)
                                }
                            }
                            Spacer()
                            Button {
                                NSWorkspace.shared.open(LaunchEngine.gameDirectory(versionId: v.versionId))
                            } label: {
                                Image(systemName: "folder")
                            }
                            .buttonStyle(GlassButtonStyle(dense: true))
                            .help("打开该版本的游戏目录")

                            Button(ui.busy ? "启动中…" : "启动") { Task { await launch(v) } }
                                .buttonStyle(GlassButtonStyle(prominent: true))
                                .disabled(ui.busy)
                        }
                        .padding(.vertical, 7)
                        .listRowBackground(Color.clear)
                    }
                }
                .listStyle(.inset)
                .glassList()
            }

            if !ui.status.isEmpty {
                GlassPanel(corner: 10, padding: 10) {
                    HStack(alignment: .top, spacing: 8) {
                        Image(systemName: ui.failed ? "exclamationmark.triangle.fill" : "info.circle")
                            .foregroundStyle(ui.failed ? Color.orange : Color.secondary)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(ui.status)
                                .font(.caption)
                                .foregroundStyle(ui.failed ? Color.orange : Color.secondary)
                            if !ui.detail.isEmpty {
                                Text(ui.detail)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        if ui.failed {
                            Button("查看日志") { app.selectedTab = .home }
                                .buttonStyle(GlassButtonStyle(dense: true))
                        }
                    }
                }
            }
            Spacer()
        }
        .padding(20)
        .onAppear {
            ui.rescanJava(required: 0)
        }
    }

    func refresh() async {
        let list = LocalVersionScanner().scanInstalled()
        await MainActor.run { app.versions = list; app.saveVersions() }
        ui.rescanJava(required: 0)
    }

    func launch(_ v: GameVersion) async {
        guard let acc = app.activeAccount else {
            await MainActor.run {
                ui.failed = true
                ui.status = "请先到「账号」页添加并激活一个账号。"
                ui.detail = ""
            }
            return
        }
        await MainActor.run {
            ui.busy = true
            ui.failed = false
            ui.detail = ""
            ui.status = "准备依赖：\(v.versionId)"
        }
        do {
            // 1) 该版本要求的 Java 大版本
            let merged = try await LaunchEngine.mergedVersion(v.versionId)
            let required = ((merged["javaVersion"] as? [String: Any])?["majorVersion"] as? Int) ?? 0

            // 2) 选 Java：优先实例里 HMCL 指定的 Java，其次当前选中，最后自动挑最合适的
            let instanceJava = LaunchEngine.instanceJavaExecutable(v.versionId)
                .flatMap { JavaManager.installFromExecutable($0) }
            let java: JavaInstall
            if let ij = instanceJava, ij.version >= required {
                java = ij
            } else if let sel = ui.selectedJava, sel.isUsable, sel.version >= required {
                java = sel
            } else if let best = JavaManager.bestMatch(required: required) {
                java = best
            } else {
                throw LaunchError.noJava(required: required)
            }
            await MainActor.run {
                ui.selectedJava = java
                ui.status = "准备依赖：\(v.versionId)（Java \(java.version)）"
            }

            // 3) 下载依赖
            try await LaunchEngine.prepare(versionId: v.versionId) { p, msg in
                let m = msg ?? "准备中 \(Int(p * 100))%"
                Task { @MainActor in ui.status = m }
            }

            // 4) 启动
            await MainActor.run { ui.status = "正在启动 \(v.versionId)…" }
            _ = try await LaunchEngine.launch(versionId: v.versionId, account: acc, java: java)
            await MainActor.run {
                ui.status = "已启动 \(v.versionId)，游戏日志会输出到首页「运行日志」。"
                ui.runningVersion = v.versionId
            }
        } catch {
            let message = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            await MainActor.run {
                ui.failed = true
                ui.status = "启动失败：\(message)"
                ui.detail = "游戏目录：\(LaunchEngine.gameDirectory(versionId: v.versionId).path)"
                app.log("启动失败[\(v.versionId)]：\(message)")
            }
        }
        await MainActor.run { ui.busy = false }
    }
}
