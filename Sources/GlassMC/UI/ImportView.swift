import SwiftUI

/// ImportView 的本地 UI 状态。
final class ImportUI: ObservableObject {
    @Published var candidates: [DetectedLauncher] = []
    @Published var selected: Set<UUID> = []
    @Published var status = ""
    @Published var busy = false
}

struct ImportView: View {
    @EnvironmentObject var app: AppState
    @StateObject private var ui = ImportUI()

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("导入").font(.largeTitle.bold())
                    Text("可迁移账号（尽力）、游戏版本、模组、存档与资源包。官方 / Prism 的账号令牌在系统钥匙串中，需导入后重新登录。")
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("扫描本机启动器") { scan() }
                    .buttonStyle(GlassButtonStyle(prominent: true))
            }

            if ui.candidates.isEmpty {
                GlassPanel(corner: 16) {
                    Text("尚未扫描。点击「扫描本机启动器」查找 HMCL / 官方 / Prism。")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                GlassPanel(corner: 16, padding: 8) {
                    List {
                        ForEach(ui.candidates) { c in
                            HStack(spacing: 12) {
                                Toggle("", isOn: Binding(
                                    get: { ui.selected.contains(c.id) },
                                    set: { on in
                                        if on { ui.selected.insert(c.id) } else { ui.selected.remove(c.id) }
                                    }))
                                .toggleStyle(.checkbox)
                                .labelsHidden()
                                VStack(alignment: .leading, spacing: 3) {
                                    HStack(spacing: 8) {
                                        Text(c.name).font(.headline)
                                        GlassBadge(text: "\(c.versionCount) 版本", tint: .blue)
                                    }
                                    Text(c.gameDir).font(.caption).foregroundStyle(.secondary)
                                    Text("账号 \(c.accountCount)")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                            }
                            .padding(.vertical, 7)
                            .listRowBackground(Color.clear)
                        }
                    }
                    .listStyle(.inset)
                    .glassList()
                }

                Button(ui.busy ? "导入中…" : "导入所选 (\(ui.selected.count))") { Task { await doImport() } }
                    .buttonStyle(GlassButtonStyle(prominent: true))
                    .disabled(ui.busy || ui.selected.isEmpty)
            }

            if !ui.status.isEmpty {
                Text(ui.status).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(20)
        .onAppear { if ui.candidates.isEmpty { scan() } }
    }

    func scan() {
        ui.candidates = LauncherImporter.detect()
        ui.selected = Set(ui.candidates.map { $0.id })
        app.log("扫描到 \(ui.candidates.count) 个启动器")
    }

    func doImport() async {
        await MainActor.run { ui.busy = true }
        for c in ui.candidates where ui.selected.contains(c.id) {
            do {
                let vs = try LauncherImporter.import(c) { msg in
                    Task { @MainActor in ui.status = msg }
                }
                _ = vs
            } catch {
                Task { @MainActor in
                    ui.status = "导入 \(c.name) 失败：\(error.localizedDescription)"
                    app.log(ui.status)
                }
            }
        }
        let list = LocalVersionScanner().scanInstalled()
        await MainActor.run {
            app.versions = list
            app.saveVersions()
            ui.status = "导入完成，共 \(list.count) 个版本"
            ui.busy = false
        }
    }
}
