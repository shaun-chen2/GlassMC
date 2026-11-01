import SwiftUI
import AppKit

// MARK: - 玻璃卡片容器

/// 一整块圆角液态玻璃面板：毛玻璃底 + 顶部反光 + 边缘高光 + 柔和投影。
struct GlassPanel<Content: View>: View {
    var corner: CGFloat
    var padding: CGFloat
    var content: () -> Content

    init(corner: CGFloat = 16, padding: CGFloat = 16, @ViewBuilder content: @escaping () -> Content) {
        self.corner = corner
        self.padding = padding
        self.content = content
    }

    var body: some View {
        content()
            .padding(padding)
            .background(
                ZStack {
                    RoundedRectangle(cornerRadius: corner, style: .continuous)
                        .fill(.ultraThinMaterial)
                    RoundedRectangle(cornerRadius: corner, style: .continuous)
                        .fill(LinearGradient(
                            colors: [Color.white.opacity(0.20), Color.white.opacity(0.02)],
                            startPoint: .topLeading, endPoint: .bottomTrailing))
                    RoundedRectangle(cornerRadius: corner, style: .continuous)
                        .strokeBorder(Color.white.opacity(0.24), lineWidth: 1)
                }
            )
            .shadow(color: Color.black.opacity(0.16), radius: 12, x: 0, y: 6)
    }
}

// MARK: - 卡片修饰器

struct GlassCardModifier: ViewModifier {
    var corner: CGFloat = 14
    func body(content: Content) -> some View {
        content
            .background(
                ZStack {
                    RoundedRectangle(cornerRadius: corner, style: .continuous).fill(.ultraThinMaterial)
                    RoundedRectangle(cornerRadius: corner, style: .continuous).fill(
                        LinearGradient(colors: [Color.white.opacity(0.16), Color.white.opacity(0.02)],
                                       startPoint: .top, endPoint: .bottom))
                }
            )
            .overlay(
                RoundedRectangle(cornerRadius: corner, style: .continuous)
                    .strokeBorder(Color.white.opacity(0.20), lineWidth: 1)
            )
            .shadow(color: Color.black.opacity(0.12), radius: 8, x: 0, y: 4)
    }
}

extension View {
    /// 给任意视图套上一层液态玻璃卡片外观。
    func glassCard(corner: CGFloat = 14) -> some View {
        modifier(GlassCardModifier(corner: corner))
    }

    /// 让 List 背景透明，从而透出统一窗口玻璃底。
    func glassList() -> some View {
        self.scrollContentBackground(.hidden)
            .background(Color.clear)
    }
}

// MARK: - 玻璃按钮

struct GlassButtonStyle: ButtonStyle {
    var prominent: Bool = false
    var destructive: Bool = false
    var dense: Bool = false

    private var tint: Color { destructive ? Color.red : Color.accentColor }

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: dense ? 12 : 13, weight: .semibold))
            .foregroundStyle(prominent ? Color.white : (destructive ? Color.red : Color.primary))
            .padding(.horizontal, dense ? 10 : 14)
            .padding(.vertical, dense ? 5 : 7)
            .background(
                ZStack {
                    RoundedRectangle(cornerRadius: dense ? 9 : 10, style: .continuous)
                        .fill(.ultraThinMaterial)
                    if prominent {
                        RoundedRectangle(cornerRadius: dense ? 9 : 10, style: .continuous)
                            .fill(tint.opacity(configuration.isPressed ? 0.72 : 0.88))
                    }
                    RoundedRectangle(cornerRadius: dense ? 9 : 10, style: .continuous)
                        .fill(LinearGradient(
                            colors: [Color.white.opacity(prominent ? 0.34 : (configuration.isPressed ? 0.20 : 0.26)),
                                     Color.white.opacity(0.03)],
                            startPoint: .top, endPoint: .bottom))
                    RoundedRectangle(cornerRadius: dense ? 9 : 10, style: .continuous)
                        .strokeBorder(Color.white.opacity(prominent ? 0.45 : 0.28), lineWidth: 1)
                }
            )
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .animation(.spring(response: 0.24, dampingFraction: 0.7), value: configuration.isPressed)
    }
}

// MARK: - 玻璃输入框

struct GlassTextField: View {
    let placeholder: String
    let text: Binding<String>
    var secure: Bool = false
    var width: CGFloat? = nil

    var body: some View {
        Group {
            if secure {
                SecureField(placeholder, text: text)
            } else {
                TextField(placeholder, text: text)
            }
        }
        .textFieldStyle(.plain)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(width: width, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous).fill(.ultraThinMaterial)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Color.white.opacity(0.22), lineWidth: 1)
        )
    }
}

// MARK: - 玻璃分段控件

struct GlassSegmented: View {
    let options: [(title: String, value: String)]
    let selection: Binding<String>

    var body: some View {
        HStack(spacing: 4) {
            ForEach(options.indices, id: \.self) { i in
                let opt = options[i]
                Button {
                    selection.wrappedValue = opt.value
                } label: {
                    Text(opt.title)
                        .font(.system(size: 12, weight: .semibold))
                        .padding(.horizontal, 14).padding(.vertical, 6)
                        .frame(maxWidth: .infinity)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(selection.wrappedValue == opt.value
                                      ? Color.white.opacity(0.30)
                                      : Color.clear)
                        )
                        .foregroundStyle(selection.wrappedValue == opt.value ? Color.primary : Color.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(5)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous).fill(.ultraThinMaterial)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .strokeBorder(Color.white.opacity(0.22), lineWidth: 1)
        )
    }
}

// MARK: - 玻璃徽章

struct GlassBadge: View {
    let text: String
    var tint: Color = Color.accentColor
    var body: some View {
        Text(text)
            .font(.system(size: 11, weight: .semibold))
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(Capsule().fill(tint.opacity(0.18)))
            .overlay(Capsule().strokeBorder(tint.opacity(0.4), lineWidth: 0.8))
            .foregroundStyle(tint)
    }
}

// MARK: - ObservableObject -> Binding 便捷转换（本 SDK 无 @State/@Binding 宏）

extension ObservableObject {
    func bind<T>(_ keyPath: ReferenceWritableKeyPath<Self, T>) -> Binding<T> {
        Binding(get: { self[keyPath: keyPath] },
                set: { self[keyPath: keyPath] = $0 })
    }
}
