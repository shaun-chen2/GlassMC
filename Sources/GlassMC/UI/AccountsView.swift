import SwiftUI
import AppKit
import UniformTypeIdentifiers

/// AccountsView 的本地 UI 状态。
final class AccountsUI: ObservableObject {
    @Published var showAdd = false
    @Published var showLibrary = false
    @Published var selectedID: UUID?
    @Published var busyID: UUID?
    @Published var status = ""
    /// 本次运行是否已尝试过补齐历史账号缺失的站点皮肤。
    var autoSynced = false
}

/// 头部头像加载模型（代替 @State）。
final class SkinHeadModel: ObservableObject {
    @Published var image: NSImage?
    @Published var note = ""
}

struct AccountsView: View {
    @EnvironmentObject var app: AppState
    @EnvironmentObject var settings: SettingsStore
    @StateObject private var ui = AccountsUI()

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("账号").font(.largeTitle.bold())
                    Text("支持离线账号与 Yggdrasil 认证服务器（外置登录）。皮肤在登录后自动获取并缓存。")
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("皮肤库") { ui.showLibrary = true }
                    .buttonStyle(GlassButtonStyle())
                Button("添加账号") { ui.showAdd = true }
                    .buttonStyle(GlassButtonStyle(prominent: true))
            }

            HStack(alignment: .top, spacing: 18) {
                GlassPanel(corner: 16, padding: 8) {
                    List {
                        ForEach(app.accounts) { acc in
                            accountRow(acc)
                                .listRowBackground(Color.clear)
                        }
                    }
                    .listStyle(.inset)
                    .glassList()
                    .frame(minWidth: 360, minHeight: 320)
                }

                SkinPreview(account: app.accounts.first { $0.id == ui.selectedID })
            }

            if !ui.status.isEmpty {
                Text(ui.status).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(20)
        .sheet(isPresented: $ui.showAdd) {
            AddAccountSheet(app: app, settings: settings, onClose: { ui.showAdd = false })
        }
        .sheet(isPresented: $ui.showLibrary) {
            SkinLibrarySheet(account: libraryTarget,
                             onApply: { acc, item in applyLibrarySkin(acc, item) },
                             onClose: { ui.showLibrary = false })
        }
        .onAppear {
            if !ui.autoSynced {
                ui.autoSynced = true
                autoSyncMissingSkins()
            }
        }
    }

    /// 老版本登录进来的外置账号没有贴图地址，进页面时自动补齐一次站点皮肤。
    func autoSyncMissingSkins() {
        let targets = app.accounts.filter {
            $0.type == "yggdrasil" && $0.skinURL == nil && $0.skinLocalPath == nil
        }
        guard !targets.isEmpty else { return }
        Task {
            for acc in targets {
                if let updated = await AuthManager.syncSkin(acc) {
                    app.replaceAccount(updated)
                    app.log("已从站点同步皮肤：\(updated.username)")
                }
            }
        }
    }

    /// 皮肤库套用目标：优先当前选中账号，其次激活账号。
    private var libraryTarget: MCAccount? {
        app.accounts.first { $0.id == ui.selectedID } ?? app.activeAccount
    }

    /// 把皮肤库条目套用到账号（写入本地皮肤缓存，并同步模型类型）。
    func applyLibrarySkin(_ acc: MCAccount, _ item: SkinLibraryItem) {
        let path = SkinLibraryClient.shared.cacheFile(tid: item.tid).path
        guard FileManager.default.fileExists(atPath: path) else {
            ui.status = "皮肤文件缺失，请重试。"
            return
        }
        if let i = app.accounts.firstIndex(where: { $0.id == acc.id }) {
            app.accounts[i].skinLocalPath = path
            app.accounts[i].skinVariant = item.isAlex ? "alex" : "steve"
            app.saveAccounts()
        }
        ui.selectedID = acc.id
        ui.status = "已套用皮肤「\(item.name)」→ \(acc.username)（\(item.modelLabel) 模型）"
        app.log("皮肤库套用：\(item.name)（tid \(item.tid)）→ \(acc.username)")
    }

    @ViewBuilder
    private func accountRow(_ acc: MCAccount) -> some View {
        HStack(spacing: 12) {
            SkinHeadView(account: acc).frame(width: 42, height: 42)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(acc.username).font(.headline)
                    if acc.id == app.activeAccountID {
                        GlassBadge(text: "使用中", tint: .green)
                    }
                }
                Text("\(acc.type) · 来源 \(acc.source)" + (acc.needsReauth ? " · 需重新登录" : " · 令牌可用"))
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button("设为激活") { app.setActiveAccount(acc) }
                .buttonStyle(GlassButtonStyle(dense: true))
                .disabled(acc.id == app.activeAccountID)
            Button(acc.id == ui.selectedID ? "已选" : "预览") { ui.selectedID = acc.id }
                .buttonStyle(GlassButtonStyle(dense: true))
            if acc.needsReauth {
                Button("重新登录") { reauth(acc) }
                    .buttonStyle(GlassButtonStyle(dense: true))
            }
            Button("删除") { app.removeAccount(acc) }
                .buttonStyle(GlassButtonStyle(destructive: true, dense: true))
        }
        .padding(.vertical, 7)
        .contentShape(Rectangle())
        .onTapGesture { ui.selectedID = acc.id }
    }

    func reauth(_ acc: MCAccount) {
        guard let serverId = acc.authServerId,
              settings.authServers.contains(where: { $0.id.uuidString == serverId }) else {
            app.log("该账号关联的认证服务器不可用，无法自动重新登录。")
            ui.status = "无法自动重新登录该账号。"
            return
        }
        ui.busyID = acc.id
        Task {
            do {
                let updated = try await AuthManager.refresh(acc)
                app.replaceAccount(updated)
                await MainActor.run {
                    ui.status = "已刷新登录：\(updated.username)"
                    ui.busyID = nil
                }
            } catch {
                await MainActor.run {
                    ui.status = "重新登录失败：\(error.localizedDescription)"
                    ui.busyID = nil
                }
            }
        }
    }
}

/// 右侧皮肤预览面板。
struct SkinPreview: View {
    let account: MCAccount?
    @StateObject private var model = SkinHeadModel()

    init(account: MCAccount?) {
        self.account = account
    }

    var body: some View {
        VStack(spacing: 9) {
            Text("皮肤预览").font(.headline)
            if let acc = account {
                Group {
                    if let img = model.image {
                        Image(nsImage: img).resizable().frame(width: 118, height: 118)
                    } else {
                        Image(systemName: "person.crop.square").resizable()
                            .frame(width: 118, height: 118).foregroundStyle(.secondary)
                    }
                }
                Text(acc.username).font(.headline)
                if let v = acc.skinVariant {
                    Text(v == "alex" ? "模型：Alex（细臂）" : "模型：Steve（粗臂）")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Text(skinSourceText(acc))
                    .font(.caption2).foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                if acc.type == "yggdrasil" {
                    Button("同步站点皮肤") { sync(acc) }
                        .buttonStyle(GlassButtonStyle(dense: true))
                } else {
                    Button("刷新皮肤") { refresh(acc) }
                        .buttonStyle(GlassButtonStyle(dense: true))
                }
                HStack(spacing: 6) {
                    Button("本地皮肤") { chooseLocal(acc) }
                        .buttonStyle(GlassButtonStyle(dense: true))
                    if acc.skinLocalPath != nil {
                        Button("恢复站点") { clearLocal(acc) }
                            .buttonStyle(GlassButtonStyle(dense: true))
                    }
                }
                if !model.note.isEmpty {
                    Text(model.note)
                        .font(.caption2).foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                }
            } else {
                Text("选择左侧账号预览皮肤").foregroundStyle(.secondary)
            }
            Spacer()
        }
        .frame(width: 224, height: 380)
        .padding(14)
        .glassCard(corner: 16)
        .onAppear { if let acc = account { load(acc) } }
        .onChange(of: account?.id) { _, _ in if let acc = account { load(acc) } }
        .onChange(of: account?.skinURL ?? "") { _, _ in if let acc = account { load(acc) } }
        .onChange(of: account?.skinLocalPath ?? "") { _, _ in if let acc = account { load(acc) } }
    }

    /// 当前头像用的是哪一层皮肤。
    func skinSourceText(_ acc: MCAccount) -> String {
        if acc.skinLocalPath != nil { return "本地皮肤（覆盖站点）" }
        if acc.skinURL != nil { return acc.type == "yggdrasil" ? "站点皮肤" : "已缓存皮肤" }
        if acc.type == "yggdrasil" { return "站点未设置皮肤" }
        return "默认皮肤"
    }

    func load(_ acc: MCAccount) {
        Task {
            let file = await SkinManager.ensure(acc)
            let data = file.flatMap { try? Data(contentsOf: $0) }
            let img = SkinRenderer.renderHead(skinData: data, size: 118)
            await MainActor.run {
                model.image = img
                model.note = ""
            }
        }
    }

    /// 从认证服务器（皮肤站）重新拉取贴图地址并重下皮肤。
    func sync(_ acc: MCAccount) {
        model.note = "正在从站点同步…"
        Task {
            var target = acc
            if let updated = await AuthManager.syncSkin(acc) {
                AppState.shared.replaceAccount(updated)
                target = updated
            }
            let file = await SkinManager.refresh(target)
            await MainActor.run {
                load(target)
                if let f = file {
                    model.note = "已同步：\(f.lastPathComponent)"
                } else if target.skinURL == nil {
                    model.note = "站点未返回皮肤（请先在网站上传皮肤）"
                } else {
                    model.note = "同步失败：\(target.skinURL ?? "")"
                }
            }
        }
    }

    func refresh(_ acc: MCAccount) {
        Task {
            _ = await SkinManager.refresh(acc)
            await MainActor.run { load(acc) }
        }
    }

    func clearLocal(_ acc: MCAccount) {
        SkinManager.clearLocalSkin(acc.id)
        model.note = "已恢复站点皮肤"
        if let updated = AppState.shared.accounts.first(where: { $0.id == acc.id }) {
            load(updated)
        }
    }

    func chooseLocal(_ acc: MCAccount) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.png]
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            SkinManager.setLocalSkin(acc.id, path: url.path)
            model.note = "已使用本地皮肤"
            if let updated = AppState.shared.accounts.first(where: { $0.id == acc.id }) {
                load(updated)
            }
        }
    }
}

/// 单个账号的头部头像（异步加载并渲染）。
struct SkinHeadView: View {
    let account: MCAccount
    var size: CGFloat = 42
    @StateObject private var model = SkinHeadModel()

    init(account: MCAccount, size: CGFloat = 42) {
        self.account = account
        self.size = size
    }

    var body: some View {
        Group {
            if let img = model.image {
                Image(nsImage: img).resizable()
            } else {
                Image(systemName: "person.crop.square").resizable().foregroundStyle(.secondary)
            }
        }
        .onAppear { load() }
        .onChange(of: account.skinURL ?? "") { _, _ in load() }
        .onChange(of: account.skinLocalPath ?? "") { _, _ in load() }
    }

    func load() {
        Task {
            let file = await SkinManager.ensure(account)
            let data = file.flatMap { try? Data(contentsOf: $0) }
            let img = SkinRenderer.renderHead(skinData: data, size: size)
            await MainActor.run { model.image = img }
        }
    }
}

/// 添加账号表单状态。
final class AddAccountForm: ObservableObject {
    @Published var mode = "offline"      // offline / microsoft / yggdrasil
    @Published var username = ""
    @Published var password = ""
    @Published var selectedServer: AuthServer?
    @Published var busy = false
    @Published var errorMsg = ""
    /// 微软设备码登录：显示的代码与链接、以及当前步骤提示。
    @Published var msUserCode = ""
    @Published var msURL = ""
    @Published var msStatus = ""
}

/// 添加账号弹窗：离线账号、微软正版（设备码）、Yggdrasil 认证服务器登录。
struct AddAccountSheet: View {
    let app: AppState
    let settings: SettingsStore
    let onClose: () -> Void
    @StateObject private var form = AddAccountForm()

    init(app: AppState, settings: SettingsStore, onClose: @escaping () -> Void) {
        self.app = app
        self.settings = settings
        self.onClose = onClose
    }

    var body: some View {
        GlassPanel(corner: 18, padding: 22) {
            VStack(alignment: .leading, spacing: 16) {
                Text("添加账号").font(.title2.bold())

                GlassSegmented(options: [("离线账号", "offline"), ("微软正版", "microsoft"), ("认证服务器", "yggdrasil")],
                               selection: form.bind(\.mode))

                if form.mode == "offline" {
                    GlassTextField(placeholder: "用户名（任意）", text: form.bind(\.username))
                    Text("离线账号无需联网，使用本地 UUID 与默认皮肤，可立即启动游戏。")
                        .font(.caption).foregroundStyle(.secondary)
                } else if form.mode == "microsoft" {
                    if !MicrosoftAuth.isConfigured {
                        Text("尚未配置 Azure 应用 ID：请先到「设置 → 微软正版登录」填入。微软已废弃旧的公共 ID，不填会报 AADSTS700016。")
                            .font(.caption).foregroundStyle(.orange)
                    }
                    Text("点击下方按钮后，浏览器会打开微软登录页；把显示的代码填进去并授权即可。登录令牌仅保存在本机。")
                        .font(.caption).foregroundStyle(.secondary)
                    if !form.msUserCode.isEmpty {
                        HStack(spacing: 10) {
                            Text(form.msUserCode)
                                .font(.system(size: 22, weight: .bold, design: .monospaced))
                                .padding(.horizontal, 14).padding(.vertical, 8)
                                .background(RoundedRectangle(cornerRadius: 10).fill(.ultraThinMaterial))
                            VStack(alignment: .leading, spacing: 3) {
                                Text("代码已复制到剪贴板").font(.caption).foregroundStyle(.secondary)
                                if let u = URL(string: form.msURL) {
                                    Button("重新打开登录页") { NSWorkspace.shared.open(u) }
                                        .buttonStyle(GlassButtonStyle(dense: true))
                                }
                            }
                            Spacer()
                        }
                    }
                    if !form.msStatus.isEmpty {
                        Text(form.msStatus).font(.caption).foregroundStyle(.secondary)
                    }
                } else {
                    if settings.authServers.isEmpty {
                        Text("尚未配置认证服务器，请到「设置 → 认证服务器」添加。")
                            .foregroundStyle(.secondary)
                    }
                    HStack {
                        Text("服务器").frame(width: 64, alignment: .leading)
                        Picker("", selection: form.bind(\.selectedServer)) {
                            ForEach(settings.authServers) { s in
                                Text(s.name).tag(s as AuthServer?)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 240)
                    }
                    GlassTextField(placeholder: "用户名 / 邮箱", text: form.bind(\.username))
                    GlassTextField(placeholder: "密码", text: form.bind(\.password), secure: true)
                }

                if !form.errorMsg.isEmpty {
                    Text(form.errorMsg).foregroundStyle(.red).font(.caption)
                }

                HStack {
                    Spacer()
                    Button("取消") { onClose() }
                        .buttonStyle(GlassButtonStyle())
                    Button(buttonTitle) { Task { await submit() } }
                        .buttonStyle(GlassButtonStyle(prominent: true))
                        .disabled(form.busy || !canSubmit)
                }
            }
            .frame(width: 420)
        }
        .onAppear { form.selectedServer = settings.authServers.first }
    }

    private var buttonTitle: String {
        switch form.mode {
        case "offline": return "添加"
        case "microsoft": return form.busy ? "等待授权…" : "微软登录"
        default: return "登录"
        }
    }

    private var canSubmit: Bool {
        switch form.mode {
        case "offline": return !form.username.isEmpty
        case "microsoft": return MicrosoftAuth.isConfigured && !form.busy
        default: return !form.username.isEmpty && !form.password.isEmpty && form.selectedServer != nil
        }
    }

    func submit() async {
        form.busy = true
        form.errorMsg = ""
        defer { form.busy = false }
        if form.mode == "offline" {
            let acc = MCAccount(type: "offline", username: form.username, uuid: nil,
                                accessToken: form.username, source: "glassmc", needsReauth: false)
            app.addAccount(acc)
            app.setActiveAccount(acc)
            app.log("已添加离线账号：\(form.username)")
            onClose()
        } else if form.mode == "microsoft" {
            do {
                let code = try await MicrosoftAuth.requestDeviceCode()
                await MainActor.run {
                    form.msUserCode = code.userCode
                    form.msURL = code.verificationURI
                    form.msStatus = "等待你在浏览器中完成授权…"
                }
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(code.userCode, forType: .string)
                if let u = URL(string: code.verificationURI) { NSWorkspace.shared.open(u) }
                let acc = try await MicrosoftAuth.pollForAccount(code: code) { status in
                    Task { @MainActor in form.msStatus = status }
                }
                app.addAccount(acc)
                app.setActiveAccount(acc)
                app.log("已通过微软账号登录：\(acc.username)")
                onClose()
            } catch {
                await MainActor.run { form.errorMsg = error.localizedDescription }
            }
        } else {
            guard let server = form.selectedServer else { return }
            do {
                let acc = try await AuthManager.authenticate(server: server, username: form.username, password: form.password)
                app.addAccount(acc)
                app.setActiveAccount(acc)
                app.log("已通过 \(server.name) 登录：\(acc.username)")
                onClose()
            } catch {
                form.errorMsg = error.localizedDescription
            }
        }
    }
}
