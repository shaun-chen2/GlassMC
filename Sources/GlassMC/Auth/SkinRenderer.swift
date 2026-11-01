import Foundation
import AppKit

/// 把 64×64（或 64×32 旧版）Minecraft 皮肤渲染成等距视角的 3D 头部头像。
/// 纯 CoreGraphics 绘制（原生，无 Web 技术），用于账号列表与皮肤预览。
final class SkinRenderer {

    /// 把一个皮肤 PNG 数据渲染成头部头像。数据为 nil 时回退到内置默认 Steve。
    static func renderHead(skinData: Data?, size: CGFloat = 72) -> NSImage {
        let (buf, w, h) = skinData.flatMap { readPixels($0) } ?? defaultSteve()
        let img = renderHead(from: buf, width: w, height: h, size: size)
        return img
    }

    /// 把皮肤缓冲渲染成头部头像。坐标采用 y 向下（与常规位图一致）。
    static func renderHead(from buf: [UInt8], width w: Int, height h: Int, size: CGFloat) -> NSImage {
        let F = size * 0.62          // 正面边长
        let dx = F * 0.5             // 等距水平偏移
        let dy = F * 0.5             // 等距垂直偏移
        let pad: CGFloat = size * 0.16
        let W = Int(F + dx + pad * 2)
        let H = Int(F + dy + pad * 2)

        let out = NSMutableData(length: W * H * 4)!
        let ctx = CGContext(data: out.mutableBytes, width: W, height: H,
                            bitsPerComponent: 8, bytesPerRow: W * 4,
                            space: CGColorSpaceCreateDeviceRGB(),
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        ctx.clear(CGRect(x: 0, y: 0, width: W, height: H))
        // CGContext 默认 y 向上；翻转为 y 向下，使绘制坐标与“从上到下”一致。
        ctx.translateBy(x: 0, y: CGFloat(H))
        ctx.scaleBy(x: 1, y: -1)

        let ox = pad
        let oy = pad + dy

        // 取皮肤像素（y 向下，顶部为 0）
        func colorAt(_ x: Int, _ y: Int) -> CGColor {
            guard x >= 0, y >= 0, x < w, y < h else { return NSColor.clear.cgColor }
            let i = (y * w + x) * 4
            let r = CGFloat(buf[i]) / 255
            let g = CGFloat(buf[i + 1]) / 255
            let b = CGFloat(buf[i + 2]) / 255
            let a = CGFloat(buf[i + 3]) / 255
            return NSColor(red: r, green: g, blue: b, alpha: a).cgColor
        }

        // 三个面：正面(8,8,8,8)、顶面(8,0,8,8)、右侧面(0,8,8,8)
        let front: [CGPoint] = [CGPoint(x: ox, y: oy), CGPoint(x: ox + F, y: oy),
                                CGPoint(x: ox + F, y: oy + F), CGPoint(x: ox, y: oy + F)]
        let top: [CGPoint] = [CGPoint(x: ox + dx, y: oy - dy), CGPoint(x: ox + F + dx, y: oy - dy),
                              CGPoint(x: ox + F, y: oy), CGPoint(x: ox, y: oy)]
        let right: [CGPoint] = [CGPoint(x: ox + F, y: oy), CGPoint(x: ox + F + dx, y: oy - dy),
                                CGPoint(x: ox + F + dx, y: oy + F - dy), CGPoint(x: ox + F, y: oy + F)]

        fillFace(ctx: ctx, corners: front, sx: 8, sy: 8, colorAt: colorAt)
        fillFace(ctx: ctx, corners: top, sx: 8, sy: 0, colorAt: colorAt)
        fillFace(ctx: ctx, corners: right, sx: 0, sy: 8, colorAt: colorAt)

        guard let cg = ctx.makeImage() else { return NSImage(size: NSSize(width: W, height: H)) }
        let image = NSImage(cgImage: cg, size: NSSize(width: W, height: H))
        return image
    }

    /// 把一个 8×8 网格的面投影到四边形并逐像素填充。
    private static func fillFace(ctx: CGContext,
                                corners: [CGPoint],
                                sx: Int, sy: Int,
                                colorAt: (Int, Int) -> CGColor) {
        let TL = corners[0], TR = corners[1], BR = corners[2], BL = corners[3]
        for j in 0..<8 {
            for i in 0..<8 {
                let u0 = CGFloat(i) / 8, u1 = CGFloat(i + 1) / 8
                let v0 = CGFloat(j) / 8, v1 = CGFloat(j + 1) / 8
                let p00 = lerp(TL, TR, BL, BR, u0, v0)
                let p10 = lerp(TL, TR, BL, BR, u1, v0)
                let p11 = lerp(TL, TR, BL, BR, u1, v1)
                let p01 = lerp(TL, TR, BL, BR, u0, v1)
                ctx.setFillColor(colorAt(sx + i, sy + j))
                ctx.beginPath()
                ctx.move(to: p00); ctx.addLine(to: p10)
                ctx.addLine(to: p11); ctx.addLine(to: p01)
                ctx.closePath(); ctx.fillPath()
            }
        }
    }

    /// 双线性插值得到面内 (u,v) 处的点。
    private static func lerp(_ TL: CGPoint, _ TR: CGPoint, _ BL: CGPoint, _ BR: CGPoint,
                             _ u: CGFloat, _ v: CGFloat) -> CGPoint {
        let top = CGPoint(x: TL.x + (TR.x - TL.x) * u, y: TL.y + (TR.y - TL.y) * u)
        let bot = CGPoint(x: BL.x + (BR.x - BL.x) * u, y: BL.y + (BR.y - BL.y) * u)
        return CGPoint(x: top.x + (bot.x - top.x) * v, y: top.y + (bot.y - top.y) * v)
    }

    /// 把皮肤 PNG 读成 y 向下的 RGBA 缓冲（64×64 或 64×32）。
    static func readPixels(_ data: Data) -> (buf: [UInt8], w: Int, h: Int)? {
        guard let src = NSImage(data: data),
              let rep = src.representations.first as? NSBitmapImageRep,
              let cg = rep.cgImage else { return nil }
        let w = cg.width, h = cg.height
        var buf = [UInt8](repeating: 0, count: w * h * 4)
        let ok: Bool = buf.withUnsafeMutableBytes { raw -> Bool in
            guard let ctx = CGContext(data: raw.baseAddress, width: w, height: h,
                                      bitsPerComponent: 8, bytesPerRow: w * 4,
                                      space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            ctx.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))
            return true
        }
        guard ok else { return nil }
        return (buf, w, h)
    }

    /// 内置默认 Steve 头部缓冲（离线且无本地皮肤时使用）。
    static func defaultSteve() -> (buf: [UInt8], w: Int, h: Int) {
        let w = 64, h = 64
        var buf = [UInt8](repeating: 0, count: w * h * 4)
        let skinTone: [UInt8] = [197, 139, 91, 255]
        let hair: [UInt8] = [90, 58, 34, 255]
        let side: [UInt8] = [169, 116, 74, 255]
        let clear: [UInt8] = [0, 0, 0, 0]
        func set(_ x: Int, _ y: Int, _ c: [UInt8]) {
            guard x >= 0, y >= 0, x < w, y < h else { return }
            let i = (y * w + x) * 4
            buf[i...i + 3] = c[0...3]
        }
        for y in 0..<64 { for x in 0..<64 { set(x, y, clear) } }
        // 正面
        for y in 8..<16 { for x in 8..<16 { set(x, y, skinTone) } }
        // 顶面（头发）
        for y in 0..<8 { for x in 8..<16 { set(x, y, hair) } }
        // 右侧面
        for y in 8..<16 { for x in 0..<8 { set(x, y, side) } }
        return (buf, w, h)
    }
}
