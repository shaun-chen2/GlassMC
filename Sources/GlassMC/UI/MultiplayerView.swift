import SwiftUI
import AppKit

// MARK: - 联机页本地 UI 状态

/// 本机 SDK 缺 SwiftUIMacros，不能用 @State，故把可变状态收进 ObservableObject。
final class MultiplayerUI: ObservableObject {
    @Published var joinCode: String = ""
    @Published var playerName: String = ""
    @Published var copied: Bool = false
    @Published var busy: Bool = false
}

// MARK: - 联机页

/// 联机页：基于 Terracotta（陶瓦联机）的房间创建 / 加入。
/// 隧道与协议由 Terracotta 组件负责，本页只驱动它的本地 HTTP 接口。
struct MultiplayerView: View {
    @EnvironmentObject var app: AppState
    @ObservedObject private var tc = TerracottaManager.shared
    @StateObject private var ui = MultiplayerUI()

    init() {}

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header

            if let err = tc.error {
                errorBar(err)
            }

            if !tc.ready {
                onboarding
            } else {
                content
                helpPane
            }

            Spacer(minLength: 0)
            footer
        }
        .padding(20)
        .onAppear(perform: handleAppear)
    }

    // MARK: 顶部

    private var header: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text("联机").font(.largeTitle.bold())
            statusBadge
            Spacer()
            if tc.ready {
                Button {
                    Task { await tc.refresh() }
                } label: {
                    Label("刷新", systemImage: "arrow.clockwise")
                }
                .buttonStyle(GlassButtonStyle(dense: true))
            }
        }
    }

    private var statusBadge: some View {
        let (text, color) = statusText
        return Text(text)
            .font(.system(size: 11, weight: .bold))
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Capsule().fill(color.opacity(0.92)))
            .overlay(Capsule().strokeBorder(Color.white.opacity(0.35), lineWidth: 1))
            .foregroundStyle(.white)
    }

    private var statusText: (String, Color) {
        switch tc.status.state {
        case "waiting": return ("待机", .secondary)
        case "host-scanning": return ("等待游戏开放", .orange)
        case "host-starting": return ("正在创建房间", .blue)
        case "host-ok": return ("房间已就绪", .green)
        case "guest-connecting": return ("正在连接房主", .blue)
        case "guest-starting": return ("正在建立隧道", .blue)
        case "guest-ok": return ("已加入房间", .green)
        case "exception": return ("出错", .red)
        default: return (tc.status.state, .secondary)
        }
    }

    // MARK: 首次使用

    @ViewBuilder
    private var onboarding: some View {
        GlassPanel(corner: 16, padding: 18) {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    Image(systemName: "person.3.fill").font(.system(size: 20))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("联机需要先准备一次组件").font(.headline)
                        Text("基于陶瓦联机（Terracotta）协议，可与陶瓦用户互相加入房间")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }

                Text("组件是陶瓦官方的原版二进制（约 12 MB），只在第一次使用时下载，之后常驻本地。拉起来后由它负责打洞与隧道，本启动器只负责界面。")
                    .font(.system(size: 12.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                if tc.preparing {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text(tc.prepareText.isEmpty ? "正在准备…" : tc.prepareText)
                            .font(.caption).foregroundStyle(.secondary)
                    }
                } else {
                    Button {
                        Task { await tc.bootstrap() }
                    } label: {
                        Label("下载并启用联机", systemImage: "arrow.down.circle.fill")
                    }
                    .buttonStyle(GlassButtonStyle(prominent: true))
                }

                Text("首次联机时如果系统询问是否允许「查找本地网络中的设备」，请选择允许，否则找不到房间。")
                    .font(.caption2).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: 主内容

    @ViewBuilder
    private var content: some View {
        switch tc.status.state {
        case "waiting":
            waitingPane
        case "host-scanning":
            hostScanningPane
        case "host-starting":
            startingPane(title: "正在创建房间…", detail: "正在和协调服务器协商房间，稍等几秒。")
        case "host-ok":
            hostRoomPane
        case "guest-connecting", "guest-starting":
            guestConnectingPane
        case "guest-ok":
            guestJoinedPane
        case "exception":
            exceptionPane
        default:
            waitingPane
        }
    }

    // MARK: 待机

    private var waitingPane: some View {
        HStack(alignment: .top, spacing: 12) {
            // 创建房间
            GlassPanel(corner: 16, padding: 16) {
                VStack(alignment: .leading, spacing: 10) {
                    Label("创建房间", systemImage: "crown.fill").font(.headline)
                    Text("先在游戏里进入存档，按 Esc → **对局域网开放** → 创建局域网世界，再回来点这里。")
                        .font(.system(size: 12.5)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)

                    GlassTextField(placeholder: "你的显示名", text: ui.bind(\.playerName))
                        .frame(maxWidth: .infinity)

                    Button {
                        Task {
                            await tc.createRoom(player: playerNameOrDefault)
                        }
                    } label: {
                        Label("创建房间", systemImage: "plus.circle.fill")
                    }
                    .buttonStyle(GlassButtonStyle(prominent: true))
                    .disabled(ui.busy)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            // 加入房间
            GlassPanel(corner: 16, padding: 16) {
                VStack(alignment: .leading, spacing: 10) {
                    Label("加入房间", systemImage: "arrow.right.circle.fill").font(.headline)
                    Text("把朋友发来的邀请码粘进来。形如 `U/XXXX-XXXX-XXXX-XXXX`。")
                        .font(.system(size: 12.5)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)

                    GlassTextField(placeholder: "U/XXXX-XXXX-XXXX-XXXX", text: ui.bind(\.joinCode))
                        .frame(maxWidth: .infinity)

                    Button {
                        Task { await tc.join(room: ui.joinCode, player: playerNameOrDefault) }
                    } label: {
                        Label("加入", systemImage: "arrow.right")
                    }
                    .buttonStyle(GlassButtonStyle(prominent: true))
                    .disabled(ui.joinCode.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    // MARK: 房主：扫描中

    private func startingPane(title: String, detail: String) -> some View {
        GlassPanel(corner: 16, padding: 18) {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text(title).font(.headline)
                }
                Text(detail).font(.system(size: 12.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let room = tc.status.room {
                    Text(room).font(.system(size: 12.5, design: .monospaced))
                        .foregroundStyle(.secondary)
                }
                Button("取消") { Task { await tc.leave() } }
                    .buttonStyle(GlassButtonStyle())
            }
        }
    }

    private var hostScanningPane: some View {
        GlassPanel(corner: 16, padding: 18) {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text("正在等待游戏开放局域网世界…").font(.headline)
                }
                Text("请启动游戏，进入存档后按 Esc → **对局域网开放**。开放成功后房间会自动建好，这里会显示邀请码。")
                    .font(.system(size: 12.5)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                HStack {
                    Button("取消") { Task { await tc.leave() } }
                        .buttonStyle(GlassButtonStyle())
                    Spacer()
                }
            }
        }
    }

    // MARK: 房主：房间就绪

    private var hostRoomPane: some View {
        VStack(alignment: .leading, spacing: 12) {
            GlassPanel(corner: 16, padding: 18) {
                VStack(alignment: .leading, spacing: 12) {
                    Label("房间已就绪", systemImage: "checkmark.seal.fill")
                        .font(.headline).foregroundStyle(.green)
                    Text("把下面的邀请码发给朋友，他们加入后会在游戏「多人游戏」列表里看到你的世界。")
                        .font(.system(size: 12.5)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)

                    HStack(spacing: 10) {
                        Text(tc.status.room ?? "—")
                            .font(.system(size: 20, weight: .semibold, design: .monospaced))
                            .textSelection(.enabled)
                            .padding(.horizontal, 12).padding(.vertical, 8)
                            .background(RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .fill(Color.black.opacity(0.16)))
                        Button {
                            copy(tc.status.room ?? "")
                        } label: {
                            Label(ui.copied ? "已复制" : "复制", systemImage: ui.copied ? "checkmark" : "doc.on.doc")
                        }
                        .buttonStyle(GlassButtonStyle())
                        Spacer()
                    }

                    HStack(spacing: 10) {
                        Button("解散房间") { Task { await tc.leave() } }
                            .buttonStyle(GlassButtonStyle(destructive: true))
                        Spacer()
                    }
                }
            }
            membersPane
        }
    }

    // MARK: 加入方

    private var guestConnectingPane: some View {
        GlassPanel(corner: 16, padding: 18) {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text(tc.status.state == "guest-connecting" ? "正在连接房主…" : "正在建立隧道…")
                        .font(.headline)
                }
                if let room = tc.status.room {
                    Text("房间 \(room)").font(.system(size: 12.5, design: .monospaced))
                        .foregroundStyle(.secondary)
                }
                if let d = tc.status.difficultyText {
                    Text(d).font(.system(size: 12.5)).foregroundStyle(.secondary)
                }
                Button("取消") { Task { await tc.leave() } }
                    .buttonStyle(GlassButtonStyle())
            }
        }
    }

    private var guestJoinedPane: some View {
        VStack(alignment: .leading, spacing: 12) {
            GlassPanel(corner: 16, padding: 18) {
                VStack(alignment: .leading, spacing: 12) {
                    Label("已加入房间", systemImage: "checkmark.seal.fill")
                        .font(.headline).foregroundStyle(.green)
                    Text("回到游戏 → **多人游戏**，列表里会出现「陶瓦联机大厅」，双击进入即可。如果没看到，点一下「刷新」再等几秒。")
                        .font(.system(size: 12.5)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)

                    if let url = tc.status.url {
                        HStack(spacing: 8) {
                            Text("备用直连地址").font(.caption).foregroundStyle(.secondary)
                            Text(url).font(.system(size: 12.5, design: .monospaced))
                            Button {
                                copy(url)
                            } label: {
                                Image(systemName: ui.copied ? "checkmark" : "doc.on.doc")
                            }
                            .buttonStyle(GlassButtonStyle(dense: true))
                        }
                    }

                    Button("离开房间") { Task { await tc.leave() } }
                        .buttonStyle(GlassButtonStyle(destructive: true))
                }
            }
            membersPane
        }
    }

    // MARK: 成员

    @ViewBuilder
    private var membersPane: some View {
        if let profiles = tc.status.profiles, !profiles.isEmpty {
            GlassPanel(corner: 16, padding: 16) {
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Text("房间成员").font(.headline)
                        Spacer()
                        Text("\(profiles.count) 人").font(.caption).foregroundStyle(.secondary)
                    }
                    ForEach(profiles) { p in
                        HStack(spacing: 10) {
                            Image(systemName: p.isHost ? "crown.fill" : "person.fill")
                                .foregroundStyle(p.isHost ? Color.yellow : Color.secondary)
                                .frame(width: 18)
                            Text(p.name).font(.system(size: 13))
                            Spacer()
                            GlassBadge(text: p.roleText,
                                       tint: p.isHost ? .yellow : .secondary)
                        }
                    }
                }
            }
        }
    }

    // MARK: 使用说明

    private var helpPane: some View {
        GlassPanel(corner: 16, padding: 16) {
            VStack(alignment: .leading, spacing: 10) {
                Text("怎么联机").font(.headline)

                row(index: "1", title: "房主先开放世界",
                    detail: "启动游戏进入存档 → 按 Esc → 对局域网开放 → 创建局域网世界。")
                row(index: "2", title: "回到这里创建房间",
                    detail: "点「创建房间」，几秒后会出现一串邀请码，发给朋友即可。")
                row(index: "3", title: "朋友粘贴邀请码加入",
                    detail: "对方加入成功后，回游戏 → 多人游戏，列表里会多出「陶瓦联机大厅」。")

                Divider().opacity(0.25)

                HStack(spacing: 6) {
                    Image(systemName: "arrow.triangle.2.circlepath").font(.system(size: 10))
                    Text("用的是陶瓦联机协议，对方的陶瓦客户端也能直接加入，反之亦然。")
                        .font(.system(size: 11.5))
                }
                .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func row(index: String, title: String, detail: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Text(index)
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .frame(width: 18, height: 18)
                .background(Circle().fill(Color.accentColor.opacity(0.22)))
                .foregroundStyle(Color.accentColor)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.system(size: 12.5, weight: .semibold))
                Text(detail).font(.system(size: 11.5)).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
    }

    // MARK: 异常

    private var exceptionPane: some View {
        GlassPanel(corner: 16, padding: 18) {
            VStack(alignment: .leading, spacing: 12) {
                Label("联机中断", systemImage: "exclamationmark.triangle.fill")
                    .font(.headline).foregroundStyle(.red)
                Text(tc.status.exceptionText ?? "出现了未知错误。")
                    .font(.system(size: 13)).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Button("返回") { Task { await tc.leave() } }
                    .buttonStyle(GlassButtonStyle(prominent: true))
            }
        }
    }

    // MARK: 错误条

    private func errorBar(_ text: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.red)
            Text(text).font(.system(size: 12.5)).foregroundStyle(.red)
            Spacer()
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.red.opacity(0.10)))
    }

    // MARK: 页脚

    private var footer: some View {
        HStack(spacing: 6) {
            Text("联机能力由 Terracotta 提供")
            Text("·").foregroundStyle(.tertiary)
            Text(tc.serverVersion.isEmpty ? "v\(TerracottaManager.version)" : "v\(tc.serverVersion)")
            if !tc.easytierVersion.isEmpty {
                Text("· EasyTier \(tc.easytierVersion)")
            }
            Spacer()
            Link("项目主页", destination: URL(string: "https://github.com/burningtnt/Terracotta")!)
        }
        .font(.system(size: 11))
        .foregroundStyle(.secondary)
        .padding(.top, 6)
    }

    // MARK: 行为

    private var playerNameOrDefault: String {
        let n = ui.playerName.trimmingCharacters(in: .whitespaces)
        if !n.isEmpty { return n }
        return app.activeAccount?.username ?? "GlassMC"
    }

    private func copy(_ s: String) {
        guard !s.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(s, forType: .string)
        ui.copied = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.6) { ui.copied = false }
    }

    private func handleAppear() {
        if ui.playerName.isEmpty, let name = app.activeAccount?.username {
            ui.playerName = name
        }
        if tc.binaryInstalled, !tc.ready, !tc.preparing {
            Task { await tc.bootstrap() }
        }
    }
}
