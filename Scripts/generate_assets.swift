import AppKit
import Foundation

/// 用 CoreGraphics / AppKit 原生绘制应用图标与 DMG 背景图（无需任何图片素材）。
/// 用法：swiftc generate_assets.swift -o gen_assets && ./gen_assets <输出目录>

let iconSizes = [16, 32, 64, 128, 256, 512, 1024]

/// 在精确像素尺寸的位图上下文绘制，不受显示器缩放（视网膜）影响。
func renderPNG(size: CGFloat, _ draw: (CGFloat) -> Void) -> Data {
    renderPixels(Int(size), Int(size)) { w, h in draw(CGFloat(w)) }
}

/// 矩形画布版本（用于 DMG 背景，需与 DMG 窗口尺寸完全一致，避免拉伸错位）。
func renderPNG(width: CGFloat, height: CGFloat, _ draw: (CGFloat, CGFloat) -> Void) -> Data {
    renderPixels(Int(width), Int(height)) { w, h in draw(CGFloat(w), CGFloat(h)) }
}

/// 在指定像素尺寸的 NSBitmapImageRep 上绘制，确保输出 PNG 像素尺寸精确。
func renderPixels(_ w: Int, _ h: Int, _ draw: (Int, Int) -> Void) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil,
                               pixelsWide: w, pixelsHigh: h,
                               bitsPerSample: 8, samplesPerPixel: 4,
                               hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    let ctx = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = ctx
    ctx.cgContext.clear(CGRect(x: 0, y: 0, width: w, height: h))
    draw(w, h)
    NSGraphicsContext.restoreGraphicsState()
    guard let png = rep.representation(using: .png, properties: [:]) else { fatalError("png") }
    return png
}

func drawIcon(_ size: CGFloat) {
    let r = size * 0.22
    let path = NSBezierPath(roundedRect: NSRect(x: 0, y: 0, width: size, height: size), xRadius: r, yRadius: r)
    path.addClip()
    // 玻璃蓝紫渐变底
    let g = NSGradient(starting: NSColor(red: 0.03, green: 0.48, blue: 0.98, alpha: 1),
                       ending: NSColor(red: 0.36, green: 0.34, blue: 0.92, alpha: 1))!
    g.draw(from: NSPoint(x: 0, y: size), to: NSPoint(x: size, y: 0), options: [])
    // 顶部高光
    NSColor.white.withAlphaComponent(0.28).setFill()
    NSBezierPath(ovalIn: NSRect(x: size * 0.16, y: size * 0.52, width: size * 0.68, height: size * 0.36)).fill()
    // 玻璃方块（等距立方体）暗示 Minecraft
    let cx = size * 0.5, cy = size * 0.52, h = size * 0.20, d = size * 0.20
    let top = NSBezierPath()
    top.move(to: NSPoint(x: cx, y: cy - h))
    top.line(to: NSPoint(x: cx + d, y: cy - h * 0.5))
    top.line(to: NSPoint(x: cx, y: cy))
    top.line(to: NSPoint(x: cx - d, y: cy - h * 0.5))
    top.close()
    NSColor.white.withAlphaComponent(0.92).setFill(); top.fill()
    let left = NSBezierPath()
    left.move(to: NSPoint(x: cx - d, y: cy - h * 0.5))
    left.line(to: NSPoint(x: cx, y: cy))
    left.line(to: NSPoint(x: cx, y: cy + h))
    left.line(to: NSPoint(x: cx - d, y: cy + h * 0.5))
    left.close()
    NSColor.white.withAlphaComponent(0.62).setFill(); left.fill()
    let right = NSBezierPath()
    right.move(to: NSPoint(x: cx + d, y: cy - h * 0.5))
    right.line(to: NSPoint(x: cx, y: cy))
    right.line(to: NSPoint(x: cx, y: cy + h))
    right.line(to: NSPoint(x: cx + d, y: cy + h * 0.5))
    right.close()
    NSColor.white.withAlphaComponent(0.40).setFill(); right.fill()
    // 边缘高光
    NSColor.white.withAlphaComponent(0.5).setStroke()
    path.lineWidth = max(1, size * 0.012); path.stroke()
}

func drawDMGBackground(_ w: CGFloat, _ h: CGFloat) {
    // 背景底色（与 DMG 窗口 658×498 完全一致，避免拉伸错位）。
    NSColor(red: 0.93, green: 0.95, blue: 0.98, alpha: 1).setFill()
    NSRect(x: 0, y: 0, width: w, height: h).fill()
    let g = NSGradient(starting: NSColor(red: 0.96, green: 0.98, blue: 1.0, alpha: 1),
                       ending: NSColor(red: 0.86, green: 0.90, blue: 0.96, alpha: 1))!
    g.draw(in: NSRect(x: 0, y: 0, width: w, height: h * 0.6), angle: -90)

    // 标题
    let titleFont = NSFont.boldSystemFont(ofSize: 54)
    let titleAttrs: [NSAttributedString.Key: Any] = [.font: titleFont, .foregroundColor: NSColor(red: 0.1, green: 0.2, blue: 0.4, alpha: 1)]
    let title = "GlassMC" as NSString
    title.draw(at: NSPoint(x: (w - title.size(withAttributes: titleAttrs).width) / 2, y: h - 110), withAttributes: titleAttrs)

    // 副标题（安装指引）
    let subFont = NSFont.systemFont(ofSize: 22)
    let subAttrs: [NSAttributedString.Key: Any] = [.font: subFont, .foregroundColor: NSColor(red: 0.2, green: 0.3, blue: 0.5, alpha: 1)]
    let sub = "将 GlassMC 拖到右侧「应用程序」文件夹即可完成安装" as NSString
    sub.draw(at: NSPoint(x: (w - sub.size(withAttributes: subAttrs).width) / 2, y: h - 150), withAttributes: subAttrs)

    // 箭头（从左 app 区指向右 Applications 区，位于图标上方）
    NSColor(red: 0.2, green: 0.4, blue: 0.7, alpha: 0.85).setFill()
    let arrow = NSBezierPath()
    let ax = w * 0.5, ay = h * 0.46
    arrow.move(to: NSPoint(x: ax - 70, y: ay))
    arrow.line(to: NSPoint(x: ax + 30, y: ay))
    arrow.line(to: NSPoint(x: ax + 30, y: ay - 12))
    arrow.line(to: NSPoint(x: ax + 78, y: ay + 4))
    arrow.line(to: NSPoint(x: ax + 30, y: ay + 20))
    arrow.line(to: NSPoint(x: ax + 30, y: ay + 8))
    arrow.line(to: NSPoint(x: ax - 70, y: ay + 8))
    arrow.close(); arrow.fill()
}

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
let fm = FileManager.default
let iconSet = URL(fileURLWithPath: outDir).appendingPathComponent("AppIcon.iconset")
try? fm.createDirectory(at: iconSet, withIntermediateDirectories: true)

for s in iconSizes {
    let data = renderPNG(size: CGFloat(s)) { _ in drawIcon(CGFloat(s)) }
    let name = s <= 512 ? "icon_\(s)x\(s).png" : "icon_512x512@2x.png"
    try? data.write(to: iconSet.appendingPathComponent(name))
}

// DMG 背景（与窗口尺寸完全一致：658 × 498）
let bgData = renderPNG(width: 658, height: 498) { w, h in drawDMGBackground(w, h) }
try? bgData.write(to: URL(fileURLWithPath: outDir).appendingPathComponent("dmg_background.png"))

// 生成 icns
let task = Process()
task.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
task.arguments = ["--convert", "icns", "--output", "\(outDir)/AppIcon.icns", iconSet.path]
try? task.run(); task.waitUntilExit()

print("资源已生成：\(outDir)")
