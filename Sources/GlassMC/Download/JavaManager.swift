import Foundation
import Darwin

/// 一个已安装的 Java 运行环境。
struct JavaInstall: Identifiable, Hashable {
    let id: UUID
    let version: Int
    let homePath: String
    /// true = GlassMC 自己下载管理的；false = 系统里已有的 JDK。
    var managed: Bool

    init(version: Int, homePath: String, managed: Bool = false) {
        self.id = UUID()
        self.version = version
        self.homePath = homePath
        self.managed = managed
    }

    var executable: String { "\(homePath)/bin/java" }
    var isUsable: Bool { FileManager.default.isExecutableFile(atPath: executable) }

    static func == (lhs: JavaInstall, rhs: JavaInstall) -> Bool { lhs.homePath == rhs.homePath }
    func hash(into hasher: inout Hasher) { hasher.combine(homePath) }
}

/// Java 运行环境管理：下载源使用清华大学开源软件镜像站（Adoptium/Temurin 镜像）。
final class JavaManager {
    /// 提供下载的 LTS 特性版本。
    static let availableFeatureVersions = [8, 11, 17, 21]

    /// 当前机器架构：arm64 -> aarch64，x86_64 -> x64。
    static var arch: String {
        LaunchEngine.cpuArch == "arm64" ? "aarch64" : "x64"
    }

    // MARK: - 扫描

    /// 扫描可用的 Java：GlassMC 管理的 + 系统已装的。
    /// 注意：只有真正存在 `bin/java` 的才算可用，否则会把「下了一半的空目录」当成 Java 21，
    /// 启动时直接报文件不存在。
    static func installed() -> [JavaInstall] {
        var result: [JavaInstall] = []
        var seen = Set<String>()

        func append(home: String, version: Int, managed: Bool) {
            let resolved = URL(fileURLWithPath: home).resolvingSymlinksInPath().path
            var v = version
            if v <= 0 {
                v = majorVersion(at: resolved, fallbackName: (resolved as NSString).lastPathComponent)
            }
            // 版本号识别不出（多半是 /opt/homebrew/opt/openjdk 这类重复软链）就丢弃，避免选到 Java 0。
            guard v > 0 else { return }
            guard FileManager.default.isExecutableFile(atPath: home + "/bin/java") else { return }
            guard !seen.contains(resolved) else { return }
            seen.insert(resolved)
            result.append(JavaInstall(version: v, homePath: home, managed: managed))
        }

        // 1) GlassMC 下载目录：java/<feature>/
        let jd = Paths.javaDir
        if Paths.isDirectory(jd.path) {
            let items = (try? FileManager.default.contentsOfDirectory(at: jd, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
            for url in items where LauncherImporter.isDir(url) {
                let name = url.lastPathComponent
                guard let feature = Int(name) else { continue }
                let home = javaHome(in: url.path)
                append(home: home, version: feature, managed: true)
            }
        }

        // 2) 系统 JDK（用户可能已经装过，避免重复下载）
        var roots: [String] = [
            "/Library/Java/JavaVirtualMachines",
            "\(NSHomeDirectory())/Library/Java/JavaVirtualMachines",
            "/System/Library/Java/JavaVirtualMachines"
        ]
        roots.append(contentsOf: ["/opt/homebrew/opt", "/usr/local/opt"])
        for root in roots {
            guard let entries = try? FileManager.default.contentsOfDirectory(atPath: root) else { continue }
            for name in entries {
                let base = "\(root)/\(name)"
                var candidates: [String] = []
                if name.hasSuffix(".jdk") || Paths.isDirectory("\(base)/Contents/Home") {
                    candidates.append("\(base)/Contents/Home")
                }
                if FileManager.default.fileExists(atPath: "\(base)/bin/java") {
                    candidates.append(base)
                } else if let subs = try? FileManager.default.contentsOfDirectory(atPath: base) {
                    for sub in subs where sub.hasPrefix("openjdk") || sub.hasPrefix("jdk") {
                        if FileManager.default.fileExists(atPath: "\(base)/\(sub)/bin/java") {
                            candidates.append("\(base)/\(sub)")
                        }
                        if FileManager.default.fileExists(atPath: "\(base)/\(sub)/Contents/Home/bin/java") {
                            candidates.append("\(base)/\(sub)/Contents/Home")
                        }
                    }
                }
                for home in candidates {
                    append(home: home, version: majorVersion(at: home, fallbackName: name), managed: false)
                }
            }
        }

        return result.sorted { $0.version > $1.version }
    }

    /// 选择最适合该版本的 Java：优先满足最低版本要求，且优先用 GlassMC 自己管理的。
    static func bestMatch(required: Int) -> JavaInstall? {
        let list = installed()
        guard !list.isEmpty else { return nil }
        if required <= 0 { return list.first }
        let ok = list.filter { $0.version >= required }
        if ok.isEmpty { return list.first }
        // 满足要求里取最小版本（避免 Java 24 跑老 mod），相同则优先 managed
        return ok.sorted {
            if $0.version != $1.version { return $0.version < $1.version }
            return $0.managed && !$1.managed
        }.first
    }

    /// 读取 release 文件解析主版本号。
    static func majorVersion(at home: String, fallbackName: String = "") -> Int {
        if let text = try? String(contentsOfFile: "\(home)/release", encoding: .utf8) {
            for line in text.split(separator: "\n") where line.hasPrefix("JAVA_VERSION=") {
                let v = line.replacingOccurrences(of: "JAVA_VERSION=", with: "")
                    .trimmingCharacters(in: CharacterSet(charactersIn: "\"' "))
                if v.hasPrefix("1.") {
                    let rest = v.dropFirst(2).prefix(while: { $0.isNumber })
                    return Int(rest) ?? 8
                }
                let major = v.prefix(while: { $0.isNumber })
                if let m = Int(major), m > 0 { return m }
            }
        }
        // 退化：目录名里第一个数字串（如 jdk-21.0.11+10 / openjdk@21）
        var digits = ""
        for ch in fallbackName {
            if ch.isNumber { digits.append(ch) }
            else if !digits.isEmpty { break }
        }
        if let n = Int(digits), n > 0 {
            return n == 1 ? 8 : n
        }
        return 0
    }

    /// 由 java 可执行文件路径反推 JavaInstall（用于沿用实例里配置的 Java）。
    static func installFromExecutable(_ exe: String) -> JavaInstall? {
        guard FileManager.default.isExecutableFile(atPath: exe) else { return nil }
        let home = ((exe as NSString).deletingLastPathComponent as NSString).deletingLastPathComponent
        let v = majorVersion(at: home, fallbackName: (home as NSString).lastPathComponent)
        guard v > 0 else { return nil }
        return JavaInstall(version: v, homePath: home, managed: false)
    }

    /// 给定解压目录，定位 JAVA_HOME（bin/java 所在）。
    static func javaHome(in dir: String) -> String {
        let fm = FileManager.default
        let contents = "\(dir)/Contents/Home"
        if fm.fileExists(atPath: "\(contents)/bin/java") { return contents }
        if fm.fileExists(atPath: "\(dir)/bin/java") { return dir }
        if let sub = (try? fm.contentsOfDirectory(atPath: dir))?
            .first(where: { fm.fileExists(atPath: "\(dir)/\($0)/bin/java") }) {
            return "\(dir)/\(sub)"
        }
        if let sub = (try? fm.contentsOfDirectory(atPath: dir))?
            .first(where: { fm.fileExists(atPath: "\(dir)/\($0)/Contents/Home/bin/java") }) {
            return "\(dir)/\(sub)/Contents/Home"
        }
        return dir
    }

    // MARK: - 下载安装

    /// 通过官方 Adoptium API 解析最新二进制文件名（仅取元数据），
    /// 再用结构化字段拼出清华镜像路径：
    ///   <mirror>/<feature>/jdk/<arch>/mac/<filename>
    /// 返回 (tuna 主用地址, github 官方回退地址, 文件名)。
    static func resolveDownloadURL(feature: Int) async throws -> (tuna: URL, fallback: URL, filename: String) {
        let arches = [arch] + (arch == "aarch64" ? ["x64"] : ["aarch64"])
        for tryArch in arches {
            let api = URL(string: "https://api.adoptium.net/v3/assets/latest/\(feature)/hotspot?architecture=\(tryArch)&os=mac&image_type=jdk&vendor=eclipse&release_type=ga")!
            do {
                let (data, _) = try await URLSession.shared.data(from: api)
                guard let arr = try JSONSerialization.jsonObject(with: data) as? [[String: Any]],
                      let first = arr.first,
                      let binary = first["binary"] as? [String: Any],
                      let pkg = binary["package"] as? [String: Any],
                      let link = pkg["link"] as? String,
                      let filename = URL(string: link)?.lastPathComponent, !filename.isEmpty else {
                    continue
                }
                // 清华镜像：<mirror>/<feature>/jdk/<arch>/mac/<filename>
                let mirror = SettingsStore.shared.javaMirror.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                guard let tuna = URL(string: "\(mirror)/\(feature)/jdk/\(tryArch)/mac/\(filename)") else {
                    continue
                }
                // 官方回退（GitHub release，支持重定向）。解码 %2B 等百分号编码。
                guard let fallback = URL(string: link.removingPercentEncoding ?? link) else {
                    continue
                }
                return (tuna, fallback, filename)
            } catch {
                // 尝试下一个架构
            }
        }
        throw NSError(domain: "JavaManager", code: 1,
                      userInfo: [NSLocalizedDescriptionKey: "无法解析 Adoptium 下载链接（该版本/架构可能不存在）"])
    }

    /// 下载并解压指定特性版本的 Java 到数据目录。
    /// 先解压到临时目录，校验 `bin/java` 存在后再落位，失败不留脏目录。
    static func install(feature: Int, progress: ((Double, String?) -> Void)? = nil) async throws -> JavaInstall {
        let (tuna, fallback, filename) = try await resolveDownloadURL(feature: feature)

        let fm = FileManager.default
        let staging = Paths.javaDir.appendingPathComponent(".staging-\(feature)-\(UUID().uuidString.prefix(6))", isDirectory: true)
        try fm.createDirectory(at: staging, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: staging) }

        let archive = staging.appendingPathComponent(filename)
        AppState.shared.log("从清华源下载 Java \(feature)：\(filename)")
        do {
            try await DownloadManager.download(tuna, to: archive, progress: progress)
        } catch {
            AppState.shared.log("清华源下载失败，回退官方源：\(error.localizedDescription)")
            try await DownloadManager.download(fallback, to: archive, progress: progress)
        }

        AppState.shared.log("解压 Java \(feature) ...")
        let payload = staging.appendingPathComponent("payload", isDirectory: true)
        try fm.createDirectory(at: payload, withIntermediateDirectories: true)
        try extract(tarGz: archive, into: payload)
        try? fm.removeItem(at: archive)

        let home = javaHome(in: payload.path)
        guard fm.isExecutableFile(atPath: home + "/bin/java") else {
            throw NSError(domain: "JavaManager", code: 4,
                          userInfo: [NSLocalizedDescriptionKey: "Java \(feature) 解压后未找到 bin/java，安装包可能不完整，请重试。"])
        }
        // 确保可执行权限（个别解压方式会丢权限）
        try? fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: home + "/bin/java")

        // 落位：java/<feature>/
        let destDir = Paths.javaDir.appendingPathComponent("\(feature)", isDirectory: true)
        try? fm.removeItem(at: destDir)
        // payload 里通常只有一个 jdk-xx.jdk 目录，直接搬其内容
        let inner = (try? fm.contentsOfDirectory(atPath: payload.path))?.filter { !$0.hasPrefix(".") } ?? []
        if inner.count == 1, Paths.isDirectory(payload.appendingPathComponent(inner[0]).path) {
            try fm.moveItem(at: payload.appendingPathComponent(inner[0]), to: destDir)
        } else {
            try fm.moveItem(at: payload, to: destDir)
        }
        appLog("Java \(feature) 安装完成：\(javaHome(in: destDir.path))")
        return JavaInstall(version: feature, homePath: javaHome(in: destDir.path), managed: true)
    }

    /// 删除一个已安装的 Java（仅限 GlassMC 管理的目录）。
    static func uninstall(_ install: JavaInstall) {
        guard install.managed else { return }
        let dir = Paths.javaDir.appendingPathComponent("\(install.version)", isDirectory: true)
        try? FileManager.default.removeItem(at: dir)
    }

    static func extract(tarGz: URL, into dir: URL) throws {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
        proc.arguments = ["xzf", tarGz.path, "-C", dir.path]
        try proc.run()
        proc.waitUntilExit()
        guard proc.terminationStatus == 0 else {
            throw NSError(domain: "JavaManager", code: 3,
                          userInfo: [NSLocalizedDescriptionKey: "Java 解压失败（tar 退出码 \(proc.terminationStatus)）"])
        }
    }

    private static func appLog(_ message: String) {
        AppState.shared.log(message)
    }
}
