import SwiftUI
import AppKit

/// SettingsView 的本地 UI 状态。
final class SettingsUI: ObservableObject {
    @Published var javaInstalls: [JavaInstall] = []
    @Published var javaStatus: [Int: String] = [:]
    @Published var busyJava: Int?
    @Published var javaError = ""
    @Published var serverName = ""
    @Published var serverURL = ""
    @Published var serverError = ""
    @Published var injectorBusy = false
    @Published var injectorNote = ""
    @Published var scriptVersion = ""
    @Published var scriptNote = ""
}

struct SettingsView: View {
    @EnvironmentObject var app: AppState
    @EnvironmentObject var settings: SettingsStore
    @StateObject private var ui = SettingsUI()

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("设置").font(.largeTitle.bold())

                GlassPanel(corner: 16) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("外观").font(.title3.bold())
                        HStack {
                            Text("窗口玻璃透明度")
                            Spacer()
                            Text(String(format: "%.0f%%", settings.glassOpacity * 100))
                                .foregroundStyle(.secondary)
                        }
                        Slider(value: settings.bind(\.glassOpacity), in: 0.35...1.0)
                        Text("调低后整个窗口会渲染为一整块通透的液态玻璃。")
                            .font(.caption).foregroundStyle(.secondary)
                        GlassSegmented(options: [("跟随系统", "system"), ("浅色", "light"), ("深色", "dark")],
                                       selection: settings.bind(\.themeRaw))
                            .frame(width: 320)
                    }
                }

                GlassPanel(corner: 16) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("性能").font(.title3.bold())
                        HStack {
                            Text("游戏内存分配")
                            Spacer()
                            Text(String(format: "%.0f GB", settings.memoryGB))
                                .foregroundStyle(.secondary)
                        }
                        Slider(value: settings.bind(\.memoryGB), in: 1...32, step: 1)
                        Text("启动时以 -Xmx 传递给 Java。大型整合包建议 6 GB 以上；若实例自带 HMCL 配置里内存更大，会取两者较大值。")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }

                GlassPanel(corner: 16) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("窗口与启动参数").font(.title3.bold())

                        HStack(spacing: 10) {
                            Text("游戏窗口")
                            GlassTextField(placeholder: "默认", text: Binding(
                                get: { settings.windowWidth > 0 ? String(settings.windowWidth) : "" },
                                set: { settings.windowWidth = Int($0.trimmingCharacters(in: .whitespaces)) ?? 0 }
                            ), width: 76)
                            Text("×").foregroundStyle(.secondary)
                            GlassTextField(placeholder: "默认", text: Binding(
                                get: { settings.windowHeight > 0 ? String(settings.windowHeight) : "" },
                                set: { settings.windowHeight = Int($0.trimmingCharacters(in: .whitespaces)) ?? 0 }
                            ), width: 76)
                            Text("像素（留空 = 用游戏默认）").font(.caption).foregroundStyle(.secondary)
                            Spacer()
                            GlassSegmented(options: [("默认", "0"), ("1280×720", "1"), ("1600×900", "2"), ("1920×1080", "3")],
                                           selection: Binding(
                                            get: {
                                                switch (settings.windowWidth, settings.windowHeight) {
                                                case (1280, 720): return "1"
                                                case (1600, 900): return "2"
                                                case (1920, 1080): return "3"
                                                default: return "0"
                                                }
                                            },
                                            set: { v in
                                                switch v {
                                                case "1": settings.windowWidth = 1280; settings.windowHeight = 720
                                                case "2": settings.windowWidth = 1600; settings.windowHeight = 900
                                                case "3": settings.windowWidth = 1920; settings.windowHeight = 1080
                                                default: settings.windowWidth = 0; settings.windowHeight = 0
                                                }
                                            }
                                           ))
                            .frame(width: 300)
                        }

                        HStack(spacing: 10) {
                            Text("自定义 JVM 参数")
                            GlassTextField(placeholder: "-XX:+UseG1GC -XX:MaxGCPauseMillis=50", text: settings.bind(\.extraJVMArgs))
                            Button("清空") { settings.extraJVMArgs = "" }
                                .buttonStyle(GlassButtonStyle(dense: true))
                        }
                        Text("以空格分隔，追加在版本自带参数之后、主类之前。例如 -XX:+UseZGC、-Dfile.encoding=UTF-8。")
                            .font(.caption).foregroundStyle(.secondary)

                        Divider().opacity(0.25)

                        HStack(spacing: 10) {
                            Text("启动脚本")
                            Picker("", selection: ui.bind(\.scriptVersion)) {
                                ForEach(app.versions) { v in
                                    Text(v.title).tag(v.versionId)
                                }
                            }
                            .labelsHidden().frame(width: 200)
                            Button("导出 .command…") { exportScript() }
                                .buttonStyle(GlassButtonStyle(dense: true))
                                .disabled(app.versions.isEmpty)
                            Spacer()
                        }
                        Text(ui.scriptNote.isEmpty
                             ? "导出完整 java 命令行（含本次的 JVM 参数与账号参数），双击即可启动，便于排查。"
                             : ui.scriptNote)
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }

                GlassPanel(corner: 16) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("微软正版登录").font(.title3.bold())
                        HStack(spacing: 10) {
                            Text("Azure 应用 ID")
                            GlassTextField(placeholder: "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
                                           text: settings.bind(\.msClientID))
                            GlassBadge(text: MicrosoftAuth.isConfigured ? "已配置" : "未配置",
                                       tint: MicrosoftAuth.isConfigured ? .green : .orange)
                            Button("打开注册页") {
                                NSWorkspace.shared.open(URL(string: "https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade")!)
                            }
                            .buttonStyle(GlassButtonStyle(dense: true))
                        }
                        Text("微软 v2.0 设备码流程要求使用**自己注册的公共客户端**（免费）：在 Azure 门户注册应用 → 账户类型选「个人 Microsoft 账户」→ 身份验证里开启「允许公共客户端流」→ 把应用 ID 填到这里。旧的 Xbox 公共 ID 已被微软废弃，不填会报 AADSTS700016。")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }

                GlassPanel(corner: 16) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("认证服务器").font(.title3.bold())
                        Text("外置登录（Yggdrasil）服务器，供账号页登录使用。内置预设不可删除。")
                            .font(.caption).foregroundStyle(.secondary)

                        ForEach(settings.authServers) { server in
                            HStack(spacing: 10) {
                                GlassBadge(text: server.isPreset ? "预设" : "自定义",
                                           tint: server.isPreset ? .gray : Color.accentColor)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(server.name).font(.headline)
                                    Text(server.baseURL).font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                if !server.isPreset {
                                    Button("删除") {
                                        settings.authServers.removeAll { $0.id == server.id }
                                    }
                                    .buttonStyle(GlassButtonStyle(destructive: true, dense: true))
                                }
                            }
                            .padding(8)
                            .glassCard(corner: 10)
                        }

                        HStack(spacing: 10) {
                            GlassTextField(placeholder: "名称（如 MyAuth）", text: ui.bind(\.serverName), width: 150)
                            GlassTextField(placeholder: "API 基址（https://…/api/yggdrasil）", text: ui.bind(\.serverURL))
                            Button("添加") { addServer() }
                                .buttonStyle(GlassButtonStyle(prominent: true))
                                .disabled(ui.serverName.isEmpty || ui.serverURL.isEmpty)
                        }
                        if !ui.serverError.isEmpty {
                            Text(ui.serverError).font(.caption).foregroundStyle(.red)
                        }

                        Divider().opacity(0.25)
                        HStack(spacing: 10) {
                            GlassBadge(text: AuthLibInjector.isReady ? "已就绪" : "未安装",
                                       tint: AuthLibInjector.isReady ? .green : .orange)
                            VStack(alignment: .leading, spacing: 2) {
                                Text("authlib-injector").font(.headline)
                                Text(AuthLibInjector.version.map { "版本 \($0) · 启动时以 -javaagent 注入" }
                                     ?? "外置登录账号必须在游戏内注入它，才能同步站点皮肤并进入站点正版服")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button(ui.injectorBusy ? "安装中…" : (AuthLibInjector.isReady ? "重装" : "安装")) {
                                installInjector()
                            }
                            .buttonStyle(GlassButtonStyle(dense: true))
                            .disabled(ui.injectorBusy)
                        }
                        .padding(8)
                        .glassCard(corner: 10)
                        if !ui.injectorNote.isEmpty {
                            Text(ui.injectorNote).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }

                GlassPanel(corner: 16) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("下载源").font(.title3.bold())
                        HStack {
                            Text("游戏文件").frame(width: 90, alignment: .leading)
                            GlassTextField(placeholder: "BMCLAPI", text: settings.bind(\.downloadBase))
                        }
                        HStack {
                            Text("Java").frame(width: 90, alignment: .leading)
                            GlassTextField(placeholder: "清华 Adoptium 镜像", text: settings.bind(\.javaMirror))
                        }
                        Text("游戏文件默认 BMCLAPI（HMCL 镜像）；Java 默认清华大学 Adoptium 镜像。")
                            .font(.caption).foregroundStyle(.secondary)
                        Divider().opacity(0.25)
                        HStack {
                            Text("皮肤库").frame(width: 90, alignment: .leading)
                            GlassTextField(placeholder: "https://www.mcpeau.com", text: settings.bind(\.skinLibraryBase))
                        }
                        Text("皮肤库支持 Blessing Skin 6 站点（默认 MiniSkins / mcpeau.com），账号页「皮肤库」按钮会读取该站点。")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }

                GlassPanel(corner: 16) {
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Text("Java 运行环境").font(.title3.bold())
                            Spacer()
                            Button("重新扫描") {
                                ui.javaInstalls = JavaManager.installed()
                                ui.javaError = ""
                            }
                            .buttonStyle(GlassButtonStyle(dense: true))
                        }
                        ForEach(JavaManager.availableFeatureVersions, id: \.self) { v in
                            HStack {
                                Text("Java \(v)").frame(width: 70, alignment: .leading)
                                let installed = ui.javaInstalls.filter { $0.version == v }
                                if let first = installed.first {
                                    GlassBadge(text: first.managed ? "已安装" : "系统已有", tint: .green)
                                } else {
                                    GlassBadge(text: "未安装", tint: .gray)
                                }
                                Spacer()
                                if let s = ui.javaStatus[v] {
                                    Text(s).font(.caption).foregroundStyle(.secondary)
                                        .lineLimit(1).truncationMode(.middle)
                                        .frame(maxWidth: 260, alignment: .trailing)
                                }
                                if ui.busyJava == v {
                                    ProgressView().scaleEffect(0.6)
                                } else {
                                    Button(installed.isEmpty ? "安装" : "重装") {
                                        Task { await installJava(v) }
                                    }
                                    .buttonStyle(GlassButtonStyle(dense: true))
                                }
                            }
                            .padding(6)
                            .glassCard(corner: 9)
                        }

                        if !ui.javaError.isEmpty {
                            HStack(alignment: .top, spacing: 6) {
                                Image(systemName: "exclamationmark.triangle.fill")
                                    .foregroundStyle(Color.orange)
                                Text(ui.javaError).font(.caption).foregroundStyle(Color.orange)
                            }
                        }

                        Divider().opacity(0.3)
                        Text("已检测到的 Java").font(.caption.bold()).foregroundStyle(.secondary)
                        if ui.javaInstalls.isEmpty {
                            Text("未检测到任何可用 Java。可点上方「安装」从清华源下载，或自行安装 JDK 后点「重新扫描」。")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        ForEach(ui.javaInstalls) { j in
                            HStack(spacing: 8) {
                                GlassBadge(text: "Java \(j.version)", tint: j.managed ? Color.accentColor : .gray)
                                Text(j.homePath).font(.caption2).foregroundStyle(.secondary)
                                    .lineLimit(1).truncationMode(.middle)
                                Spacer()
                                if j.managed {
                                    Button("删除") { removeJava(j) }
                                        .buttonStyle(GlassButtonStyle(destructive: true, dense: true))
                                }
                            }
                            .padding(5)
                        }
                        Text("数据目录：\(Paths.dataRoot.path)")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .padding(22)
        }
        .onAppear { ui.javaInstalls = JavaManager.installed() }
    }

    func addServer() {
        let url = ui.serverURL.trimmingCharacters(in: .whitespaces)
        guard url.hasPrefix("http") else {
            ui.serverError = "API 基址需以 http(s):// 开头"
            return
        }
        settings.authServers.append(AuthServer(name: ui.serverName, baseURL: url))
        ui.serverName = ""
        ui.serverURL = ""
        ui.serverError = ""
    }

    /// 下载/更新 authlib-injector（外置登录的皮肤与认证注入）。
    func installInjector() {
        ui.injectorBusy = true
        ui.injectorNote = "正在下载 authlib-injector…"
        Task {
            AuthLibInjector.removeJar()
            let jar = await AuthLibInjector.ensure()
            await MainActor.run {
                ui.injectorBusy = false
                if let jar {
                    ui.injectorNote = "已安装：\(jar.path)"
                } else {
                    ui.injectorNote = "安装失败，请检查网络（BMCLAPI / 官方源均不可达）。"
                }
            }
        }
    }

    func installJava(_ v: Int) async {
        ui.busyJava = v
        ui.javaError = ""
        ui.javaStatus[v] = "准备中…"
        do {
            _ = try await JavaManager.install(feature: v) { p, msg in
                Task { @MainActor in ui.javaStatus[v] = msg ?? "\(Int(p * 100))%" }
            }
            await MainActor.run {
                ui.javaInstalls = JavaManager.installed()
                ui.javaStatus[v] = "完成"
                ui.busyJava = nil
            }
        } catch {
            let msg = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            await MainActor.run {
                ui.javaStatus[v] = "失败"
                ui.javaError = "Java \(v) 安装失败：\(msg)"
                ui.busyJava = nil
                app.log("Java \(v) 安装失败：\(msg)")
            }
        }
    }

    func removeJava(_ j: JavaInstall) {
        JavaManager.uninstall(j)
        ui.javaInstalls = JavaManager.installed()
        ui.javaStatus[j.version] = nil
        app.log("已删除 Java \(j.version)")
    }

    /// 导出某个版本的完整启动脚本（.command）。
    func exportScript() {
        if ui.scriptVersion.isEmpty { ui.scriptVersion = app.versions.first?.versionId ?? "" }
        let versionId = ui.scriptVersion
        guard !versionId.isEmpty else { ui.scriptNote = "请先选择一个版本"; return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "启动 \(versionId).command"
        panel.title = "导出启动脚本"
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        ui.scriptNote = "正在生成…"
        Task {
            do {
                let out = try await LaunchEngine.exportLaunchScript(versionId: versionId, to: url)
                await MainActor.run { ui.scriptNote = "已导出：\(out.lastPathComponent)（双击即可启动）" }
            } catch {
                let msg = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                await MainActor.run { ui.scriptNote = "导出失败：\(msg)" }
            }
        }
    }
}
