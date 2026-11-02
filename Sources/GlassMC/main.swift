import SwiftUI
import AppKit

/// GlassMC —— macOS 液态玻璃风格 Minecraft 启动器。
@main
struct GlassMC: App {
    @NSApplicationDelegateAdaptor(GlassAppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(AppState.shared)
                .environmentObject(SettingsStore.shared)
                .frame(minWidth: 940, minHeight: 640)
        }
        .windowStyle(.hiddenTitleBar)

        Settings {
            SettingsView()
                .environmentObject(AppState.shared)
                .environmentObject(SettingsStore.shared)
        }
    }
}

/// 确保应用以常规（前台）策略激活，并允许窗口按需置顶。
final class GlassAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    /// 退出时把联机组件一起收掉，避免留下孤儿进程。
    func applicationWillTerminate(_ notification: Notification) {
        Task { @MainActor in
            TerracottaManager.shared.shutdown()
        }
    }
}
