import SwiftUI
import AppKit

/// 皮肤库界面状态。
final class SkinLibraryModel: ObservableObject {
    @Published var items: [SkinLibraryItem] = []
    @Published var page = 1
    @Published var lastPage = 1
    @Published var total = 0
    @Published var loading = false
    @Published var error = ""
    @Published var query = ""
    @Published var selected: SkinLibraryItem?
    @Published var applying = false
    @Published var status = ""

    var filtered: [SkinLibraryItem] {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return items }
        return items.filter { $0.name.localizedCaseInsensitiveContains(q) || $0.author.localizedCaseInsensitiveContains(q) }
    }

    func load(page target: Int) async {
        await MainActor.run { loading = true; error = "" }
        do {
            let p = try await SkinLibraryClient.shared.fetchPage(target)
            await MainActor.run {
                items = p.items
                page = p.page
                lastPage = max(1, p.lastPage)
                total = p.total
                loading = false
            }
        } catch {
            await MainActor.run {
                self.error = error.localizedDescription
                loading = false
            }
        }
    }
}

/// 单个条目头像的异步加载器。
final class LibraryHeadModel: ObservableObject {
    @Published var image: NSImage?
    @Published var failed = false
}

/// 皮肤库条目头像（从站点取原始皮肤后本地等距渲染）。
struct LibraryHead: View {
    let item: SkinLibraryItem
    var size: CGFloat = 64
    @StateObject private var model = LibraryHeadModel()

    var body: some View {
        Group {
            if let img = model.image {
                Image(nsImage: img).resizable().frame(width: size, height: size)
            } else if model.failed {
                Image(systemName: "exclamationmark.triangle").resizable()
                    .frame(width: size * 0.4, height: size * 0.4)
                    .foregroundStyle(.secondary)
                    .frame(width: size, height: size)
            } else {
                ProgressView().controlSize(.small).frame(width: size, height: size)
            }
        }
        .onAppear { load() }
    }

    func load() {
        if let cached = SkinHeadCache.shared.image(for: item.tid, size: size) {
            model.image = cached
            return
        }
        Task {
            do {
                let file = try await SkinLibraryClient.shared.localTexture(for: item)
                let data = try? Data(contentsOf: file)
                let img = SkinRenderer.renderHead(skinData: data, size: size)
                SkinHeadCache.shared.store(img, for: item.tid, size: size)
                await MainActor.run { model.image = img }
            } catch {
                await MainActor.run { model.failed = true }
            }
        }
    }
}

/// 已渲染头像的内存缓存（避免翻页/滚动时重复下载与渲染）。
final class SkinHeadCache {
    static let shared = SkinHeadCache()
    private var store: [String: NSImage] = [:]
    private let lock = NSLock()

    private func key(_ tid: Int, _ size: CGFloat) -> String { "\(tid)@\(Int(size))" }

    func image(for tid: Int, size: CGFloat) -> NSImage? {
        lock.lock(); defer { lock.unlock() }
        return store[key(tid, size)]
    }

    func store(_ image: NSImage, for tid: Int, size: CGFloat) {
        lock.lock(); defer { lock.unlock() }
        store[key(tid, size)] = image
    }
}

/// 皮肤库弹窗：浏览站点皮肤库，一键套用到某个账号的本地皮肤缓存。
struct SkinLibrarySheet: View {
    /// 应用目标账号（nil 表示只浏览）。
    let account: MCAccount?
    let onApply: (MCAccount, SkinLibraryItem) -> Void
    let onClose: () -> Void
    @StateObject private var model = SkinLibraryModel()

    init(account: MCAccount?,
         onApply: @escaping (MCAccount, SkinLibraryItem) -> Void,
         onClose: @escaping () -> Void) {
        self.account = account
        self.onApply = onApply
        self.onClose = onClose
    }

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 10), count: 7)

    var body: some View {
        GlassPanel(corner: 18, padding: 20) {
            VStack(alignment: .leading, spacing: 14) {
                header
                toolbar
                content
                footer
            }
            .frame(width: 700, height: 520)
        }
        .onAppear { Task { await model.load(page: 1) } }
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 3) {
                Text("皮肤库").font(.title2.bold())
                Text(SkinLibraryClient.shared.base + " · 共 \(model.total) 款")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if let acc = account {
                VStack(alignment: .trailing, spacing: 3) {
                    Text("目标账号").font(.caption2).foregroundStyle(.secondary)
                    Text(acc.username).font(.callout.bold())
                    Text(acc.type == "offline" ? "离线账号：直接生效" : "外置登录：仅覆盖本地预览")
                        .font(.caption2).foregroundStyle(.secondary)
                }
            } else {
                Text("未选择账号（仅浏览）").font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var toolbar: some View {
        HStack(spacing: 10) {
            GlassTextField(placeholder: "在当前页中筛选名称 / 作者", text: model.bind(\.query))
            Button("刷新") { Task { await model.load(page: model.page) } }
                .buttonStyle(GlassButtonStyle(dense: true))
                .disabled(model.loading)
        }
    }

    @ViewBuilder
    private var content: some View {
        if model.loading && model.items.isEmpty {
            VStack(spacing: 10) {
                ProgressView()
                Text("正在读取皮肤库…").font(.caption).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, minHeight: 320)
        } else if !model.error.isEmpty && model.items.isEmpty {
            VStack(spacing: 8) {
                Image(systemName: "wifi.exclamationmark").font(.title2).foregroundStyle(.secondary)
                Text(model.error).font(.caption).foregroundStyle(.secondary)
                Button("重试") { Task { await model.load(page: 1) } }
                    .buttonStyle(GlassButtonStyle(dense: true))
            }
            .frame(maxWidth: .infinity, minHeight: 320)
        } else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 12) {
                    ForEach(model.filtered) { item in
                        cell(item)
                    }
                }
                .padding(4)
            }
            .frame(height: 350)
        }
    }

    private func cell(_ item: SkinLibraryItem) -> some View {
        let isSel = model.selected?.tid == item.tid
        return VStack(spacing: 5) {
            LibraryHead(item: item, size: 62)
                .padding(4)
                .background(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(Color.white.opacity(isSel ? 0.28 : 0.10))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(isSel ? Color.accentColor.opacity(0.85) : Color.white.opacity(0.18),
                                      lineWidth: isSel ? 1.6 : 1)
                )
            Text(item.name)
                .font(.system(size: 10))
                .lineLimit(1)
                .foregroundStyle(.primary)
            HStack(spacing: 4) {
                Text(item.modelLabel).font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.secondary)
                if item.likes > 0 {
                    Text("♥ \(item.likes)").font(.system(size: 9)).foregroundStyle(.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity)
        .contentShape(Rectangle())
        .onTapGesture { model.selected = item }
    }

    private var footer: some View {
        HStack(spacing: 10) {
            Button("上一页") { Task { await model.load(page: max(1, model.page - 1)) } }
                .buttonStyle(GlassButtonStyle(dense: true))
                .disabled(model.loading || model.page <= 1)
            Text("第 \(model.page) / \(model.lastPage) 页").font(.caption).foregroundStyle(.secondary)
            Button("下一页") { Task { await model.load(page: min(model.lastPage, model.page + 1)) } }
                .buttonStyle(GlassButtonStyle(dense: true))
                .disabled(model.loading || model.page >= model.lastPage)

            Spacer()

            if !model.status.isEmpty {
                Text(model.status).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Button("关闭") { onClose() }
                .buttonStyle(GlassButtonStyle())
            Button(model.applying ? "应用中…" : "套用到账号") {
                guard let item = model.selected else {
                    model.status = "请先选择一款皮肤。"
                    return
                }
                guard let acc = account else {
                    model.status = "请先在账号列表里选择一个账号。"
                    return
                }
                model.applying = true
                apply(item, to: acc)
            }
            .buttonStyle(GlassButtonStyle(prominent: true))
            .disabled(model.applying || model.selected == nil || account == nil)
        }
    }

    private func apply(_ item: SkinLibraryItem, to acc: MCAccount) {
        Task {
            do {
                let file = try await SkinLibraryClient.shared.localTexture(for: item)
                await MainActor.run {
                    onApply(acc, item)
                    model.applying = false
                    model.status = "已套用「\(item.name)」→ \(acc.username)（\(file.lastPathComponent)）"
                }
            } catch {
                await MainActor.run {
                    model.applying = false
                    model.status = "下载失败：\(error.localizedDescription)"
                }
            }
        }
    }
}
