import Foundation

/// 下载错误。
enum DownloadError: LocalizedError {
    case badStatus(Int, URL)
    case emptyBody(URL)

    var errorDescription: String? {
        switch self {
        case .badStatus(let code, let url):
            return "HTTP \(code)：\(url.absoluteString)"
        case .emptyBody(let url):
            return "响应为空：\(url.absoluteString)"
        }
    }
}

/// 基于 async/await 的下载管理器，带实时进度回调与失败重试。
/// 仅依赖 Foundation/URLSession（原生，无第三方库）。
final class DownloadManager {

    /// 下载一个文件到目标路径，progress 回调进度 0~1 与当前速度文案。
    /// 会校验 HTTP 状态码（404 页面不会被当成文件写盘），并对网络错误重试。
    static func download(_ url: URL,
                         to destination: URL,
                         retries: Int = 2,
                         progress: ((Double, String?) -> Void)? = nil) async throws {
        var lastError: Error?
        for attempt in 0...max(0, retries) {
            do {
                try await downloadOnce(url, to: destination, progress: progress)
                return
            } catch {
                lastError = error
                // HTTP 4xx 不必重试
                if case DownloadError.badStatus(let code, _) = error, (400..<500).contains(code) {
                    throw error
                }
                if attempt < retries {
                    try? await Task.sleep(nanoseconds: UInt64(400_000_000 * (attempt + 1)))
                }
            }
        }
        throw lastError ?? DownloadError.emptyBody(url)
    }

    private static func downloadOnce(_ url: URL,
                                     to destination: URL,
                                     progress: ((Double, String?) -> Void)?) async throws {
        var request = URLRequest(url: url, timeoutInterval: 120)
        request.setValue("GlassMC/1.0 (macOS; Minecraft Launcher)", forHTTPHeaderField: "User-Agent")
        request.setValue("*/*", forHTTPHeaderField: "Accept")

        let (bytes, response) = try await URLSession.shared.bytes(for: request)
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            throw DownloadError.badStatus(http.statusCode, url)
        }
        let expected = response.expectedContentLength

        let fm = FileManager.default
        try fm.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        // 先写临时文件，完成后再移动，避免半成品。
        let tmp = destination.appendingPathExtension("part")
        try? fm.removeItem(at: tmp)
        // 关键：FileHandle(forWritingTo:) 不会自己创建文件，必须先 createFile，否则每次下载都抛
        // “The file …part doesn't exist”，所有下载静默失败（这是之前无法启动/Java 装不上的根因）。
        guard fm.createFile(atPath: tmp.path, contents: nil) else {
            throw CocoaError(.fileWriteUnknown)
        }
        let handle = try FileHandle(forWritingTo: tmp)

        let byteFormatter = ByteCountFormatter()
        var buffer = Data()
        var received: Int64 = 0
        let started = Date()
        var lastReport = Date.distantPast
        do {
            for try await byte in bytes {
                buffer.append(byte)
                received += 1
                if buffer.count >= 65536 {
                    try handle.write(contentsOf: buffer)
                    buffer.removeAll(keepingCapacity: true)
                }
                if let progress, expected > 0 {
                    let now = Date()
                    if now.timeIntervalSince(lastReport) > 0.1 {
                        lastReport = now
                        let frac = Double(received) / Double(expected)
                        let elapsed = now.timeIntervalSince(started)
                        let speed = elapsed > 0 ? Double(received) / elapsed : 0
                        let msg = "\(byteFormatter.string(fromByteCount: received)) / \(byteFormatter.string(fromByteCount: expected))  (\(speed.humanSpeed))"
                        progress(min(frac, 1), msg)
                    }
                }
            }
            if !buffer.isEmpty { try handle.write(contentsOf: buffer) }
        } catch {
            try? handle.close()
            try? fm.removeItem(at: tmp)
            throw error
        }
        try handle.close()

        guard received > 0 else {
            try? fm.removeItem(at: tmp)
            throw DownloadError.emptyBody(url)
        }

        try? fm.removeItem(at: destination)
        try fm.moveItem(at: tmp, to: destination)
        progress?(1, "完成")
    }

    /// 简单的 GET JSON。
    static func getJSON<T: Decodable>(_ url: URL) async throws -> T {
        var request = URLRequest(url: url, timeoutInterval: 60)
        request.setValue("GlassMC/1.0 (macOS; Minecraft Launcher)", forHTTPHeaderField: "User-Agent")
        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            throw DownloadError.badStatus(http.statusCode, url)
        }
        return try JSONDecoder().decode(T.self, from: data)
    }
}

private extension Double {
    var humanSpeed: String {
        let b = self
        if b > 1024 * 1024 { return String(format: "%.1f MB/s", b / 1024 / 1024) }
        if b > 1024 { return String(format: "%.0f KB/s", b / 1024) }
        return String(format: "%.0f B/s", b)
    }
}
