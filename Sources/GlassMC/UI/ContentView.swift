import SwiftUI

/// 根视图：整块液态玻璃背景 + 侧栏 + 主区域，并联动窗口透明度。
struct ContentView: View {
    @EnvironmentObject var app: AppState
    @EnvironmentObject var settings: SettingsStore

    private var scheme: ColorScheme? {
        switch settings.themeRaw {
        case "light": return .light
        case "dark": return .dark
        default: return nil
        }
    }

    var body: some View {
        ZStack {
            GlassBackground(material: app.selectedTab == .settings ? .sidebar : .hudWindow)
                .ignoresSafeArea()
            GlassSheen()
            HStack(spacing: 0) {
                Sidebar().frame(width: 212)
                Divider().opacity(0.22)
                mainArea.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .padding(14)
        }
        .background(WindowConfigurator())
        .preferredColorScheme(scheme)
        .onChange(of: settings.glassOpacity) { _, newValue in
            AppState.shared.mainWindow?.alphaValue = newValue
        }
        .onAppear {
            Paths.ensureDirectories()
            app.loadVersionsCache()
            if app.versions.isEmpty { Task { await refreshVersions() } }
        }
    }

    @ViewBuilder
    private var mainArea: some View {
        switch app.selectedTab {
        case .home: HomeView()
        case .accounts: AccountsView()
        case .versions: VersionsView()
        case .install: InstallView()
        case .mods: ModsView()
        case .multiplayer: MultiplayerView()
        case .import: ImportView()
        case .settings: SettingsView()
        }
    }

    func refreshVersions() async {
        let list = LocalVersionScanner().scanInstalled()
        await MainActor.run { app.versions = list; app.saveVersions() }
    }
}
