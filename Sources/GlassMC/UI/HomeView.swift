import SwiftUI

struct HomeView: View {
    @EnvironmentObject var app: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("欢迎使用 GlassMC").font(.largeTitle.bold())
                Text("原生 macOS 液态玻璃启动器 · 支持外置登录与皮肤，游戏文件走 HMCL 镜像，Java 走清华大学开源软件镜像站。")
                    .foregroundStyle(.secondary)

                // 激活账号概览
                GlassPanel(corner: 18, padding: 18) {
                    HStack(spacing: 16) {
                        if let acc = app.activeAccount {
                            SkinHeadView(account: acc, size: 64).frame(width: 64, height: 64)
                            VStack(alignment: .leading, spacing: 4) {
                                Text("当前账号").font(.caption).foregroundStyle(.secondary)
                                Text(acc.username).font(.title2.bold())
                                HStack(spacing: 6) {
                                    GlassBadge(text: acc.type, tint: .blue)
                                    GlassBadge(text: acc.needsReauth ? "需重新登录" : "就绪",
                                               tint: acc.needsReauth ? .orange : .green)
                                }
                            }
                        } else {
                            Image(systemName: "person.crop.circle.badge.questionmark")
                                .font(.system(size: 44)).foregroundStyle(.secondary)
                            VStack(alignment: .leading, spacing: 4) {
                                Text("还没有账号").font(.title3.bold())
                                Text("添加离线账号或登录认证服务器即可开始").foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        Button("管理账号") { app.selectedTab = .accounts }
                            .buttonStyle(GlassButtonStyle(prominent: true))
                    }
                }

                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 14) {
                    HomeCard(icon: "arrow.down.doc.fill", title: "导入游戏数据", sub: "从其他启动器迁移账号与版本") { app.selectedTab = .import }
                    HomeCard(icon: "square.stack.3d.up.fill", title: "已安装版本", sub: "查看并启动已有版本") { app.selectedTab = .versions }
                    HomeCard(icon: "person.crop.circle.fill", title: "账号与皮肤", sub: "离线 / 认证服务器登录") { app.selectedTab = .accounts }
                    HomeCard(icon: "cpu.fill", title: "管理 Java", sub: "从清华源安装运行环境") { app.selectedTab = .settings }
                }

                if !app.logLines.isEmpty {
                    LogPanel()
                }
            }
            .padding(20)
        }
    }
}

struct HomeCard: View {
    let icon: String
    let title: String
    let sub: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: icon)
                    .font(.title2)
                    .foregroundStyle(Color.accentColor)
                    .frame(width: 40, height: 40)
                    .background(RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Color.accentColor.opacity(0.16)))
                VStack(alignment: .leading, spacing: 4) {
                    Text(title).font(.headline)
                    Text(sub).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassCard(corner: 14)
        }
        .buttonStyle(.plain)
    }
}

struct LogPanel: View {
    @EnvironmentObject var app: AppState
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("运行日志").font(.headline)
                Spacer()
                Button("清空") { app.logLines.removeAll() }
                    .buttonStyle(GlassButtonStyle(dense: true))
            }
            ScrollViewReader { proxy in
                ScrollView {
                    Text(app.logLines.joined(separator: "\n"))
                        .font(.system(.caption, design: .monospaced))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .id("log-bottom")
                }
                .frame(height: 160)
                .onChange(of: app.logLines.count) { _, _ in
                    withAnimation(.easeOut(duration: 0.15)) {
                        proxy.scrollTo("log-bottom", anchor: .bottom)
                    }
                }
            }
        }
        .padding(14)
        .glassCard(corner: 14)
    }
}
