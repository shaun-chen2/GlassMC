import SwiftUI
import AppKit

/// 在 ContentView 背景放置一个空 NSView，借机抓取 NSWindow 并把它配置成
/// 透明、无边框、可整窗拖拽的液态玻璃容器。
struct WindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            configure(window)
            AppState.shared.mainWindow = window
            // 初始透明度同步
            window.alphaValue = SettingsStore.shared.glassOpacity
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}

    private func configure(_ window: NSWindow) {
        window.isOpaque = false
        window.backgroundColor = .clear
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.styleMask.insert(.fullSizeContentView)
        window.isMovableByWindowBackground = true
        window.hasShadow = true
        window.level = .normal
        window.collectionBehavior = [.fullScreenAuxiliary]

        if let contentView = window.contentView {
            contentView.wantsLayer = true
            contentView.layer?.cornerRadius = 20
            contentView.layer?.masksToBounds = true
        }
    }
}

/// 全窗口的 NSVisualEffectView，作为整块连贯玻璃的底层。
/// behindWindow 混合模式会让其折射桌面背景，实现“通透”的液态玻璃质感。
struct GlassBackground: NSViewRepresentable {
    var material: NSVisualEffectView.Material = .hudWindow
    var blending: NSVisualEffectView.BlendingMode = .behindWindow
    var state: NSVisualEffectView.State = .active

    func makeNSView(context: Context) -> NSVisualEffectView {
        let vev = NSVisualEffectView()
        vev.material = material
        vev.blendingMode = blending
        vev.state = state
        vev.wantsLayer = true
        vev.layer?.cornerRadius = 20
        return vev
    }

    func updateNSView(_ vev: NSVisualEffectView, context: Context) {
        vev.material = material
        vev.blendingMode = blending
        vev.state = state
    }
}

/// 液态玻璃高光叠层：顶部反光 + 边缘高光描边 + 柔和对角光泽。
/// 全部用 SwiftUI 形状绘制（原生，无 Web 技术）。
struct GlassSheen: View {
    var body: some View {
        ZStack {
            // 顶部反光
            LinearGradient(
                colors: [Color.white.opacity(0.22), Color.white.opacity(0)],
                startPoint: .top,
                endPoint: .init(x: 0.5, y: 0.18)
            )
            .allowsHitTesting(false)

            // 对角柔光
            LinearGradient(
                colors: [Color.white.opacity(0.10), Color.clear, Color.white.opacity(0.06)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .allowsHitTesting(false)

            // 边缘高光描边
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .strokeBorder(Color.white.opacity(0.28), lineWidth: 1.2)
                .allowsHitTesting(false)
        }
        .ignoresSafeArea()
        .compositingGroup()
    }
}
