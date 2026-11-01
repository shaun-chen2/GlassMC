import SwiftUI
import AppKit

/// 左侧导航栏，使用玻璃质感按钮并带弹性高亮切换。
struct Sidebar: View {
    @EnvironmentObject var app: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                if let icon = NSImage(named: "AppIcon") {
                    Image(nsImage: icon)
                        .resizable()
                        .frame(width: 34, height: 34)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text("GlassMC").font(.headline)
                    Text("液态玻璃启动器").font(.caption).foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 12).padding(.top, 8).padding(.bottom, 10)

            ForEach(Tab.allCases) { tab in
                Button {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
                        app.selectedTab = tab
                    }
                } label: {
                    HStack(spacing: 12) {
                        Image(systemName: tab.symbol).frame(width: 22)
                        Text(tab.rawValue)
                        Spacer()
                    }
                    .padding(.vertical, 9).padding(.horizontal, 12)
                    .contentShape(RoundedRectangle(cornerRadius: 10))
                }
                .buttonStyle(.plain)
                .background(
                    (app.selectedTab == tab ? Color.accentColor.opacity(0.20) : Color.clear),
                    in: RoundedRectangle(cornerRadius: 10, style: .continuous)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(app.selectedTab == tab ? Color.white.opacity(0.22) : Color.clear, lineWidth: 1)
                )
                .foregroundStyle(app.selectedTab == tab ? Color.accentColor : Color.primary)
            }

            Spacer()

            // 底部激活账号
            if let acc = app.activeAccount {
                Button {
                    app.selectedTab = .accounts
                } label: {
                    HStack(spacing: 10) {
                        SkinHeadView(account: acc, size: 34).frame(width: 34, height: 34)
                        VStack(alignment: .leading, spacing: 1) {
                            Text("当前账号").font(.caption2).foregroundStyle(.secondary)
                            Text(acc.username).font(.system(size: 12, weight: .semibold)).lineLimit(1)
                        }
                        Spacer()
                    }
                    .padding(8)
                }
                .buttonStyle(.plain)
                .glassCard(corner: 11)
            }
        }
        .padding(8)
    }
}
