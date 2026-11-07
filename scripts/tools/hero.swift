// README hero: the island (record-overlay.swift --no-backdrop frames) in the middle of a wide
// rounded card over a picture background → a folder of frames for frames2anim.py.
// One picture keeps the layout exact (GitHub's Markdown can't lay things out precisely).
// Usage: swift scripts/tools/hero.swift <frames-in> <frames-out> <background.jpg>
import AppKit

let a = CommandLine.arguments
guard a.count == 4 else { print("usage: hero <frames-in> <frames-out> <background>"); exit(2) }
let (src, dst) = (URL(fileURLWithPath: a[1]), URL(fileURLWithPath: a[2]))
let W = 1660, H = 500, radius: CGFloat = 28
let sRGB = CGColorSpace(name: CGColorSpace.sRGB)!
func context() -> CGContext {
    CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: sRGB,
              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
}
let card = CGPath(roundedRect: CGRect(x: 0, y: 0, width: W, height: H), cornerWidth: radius, cornerHeight: radius, transform: nil)

// Background: the picture scaled to cover the card, its middle band kept.
let bg = NSImage(contentsOfFile: a[3])!.cgImage(forProposedRect: nil, context: nil, hints: nil)!
let k = max(CGFloat(W) / CGFloat(bg.width), CGFloat(H) / CGFloat(bg.height))
let (bw, bh) = (CGFloat(bg.width) * k, CGFloat(bg.height) * k)
let base = context()
base.addPath(card); base.clip()
base.interpolationQuality = .high
base.draw(bg, in: CGRect(x: (CGFloat(W) - bw) / 2, y: (CGFloat(H) - bh) / 2, width: bw, height: bh))
let baseImage = base.makeImage()!

try? FileManager.default.removeItem(at: dst)
try FileManager.default.createDirectory(at: dst, withIntermediateDirectories: true)
let names = try FileManager.default.contentsOfDirectory(atPath: src.path).filter { $0.hasSuffix(".png") }.sorted()
for n in names {
    let scene = NSImage(contentsOf: src.appendingPathComponent(n))!.cgImage(forProposedRect: nil, context: nil, hints: nil)!
    let c = context()
    c.draw(baseImage, in: CGRect(x: 0, y: 0, width: W, height: H))
    c.addPath(card); c.clip()
    // The island hangs from the card's top edge, centred.
    c.draw(scene, in: CGRect(x: (W - scene.width) / 2, y: H - scene.height, width: scene.width, height: scene.height))
    let d = CGImageDestinationCreateWithURL(dst.appendingPathComponent(n) as CFURL, "public.png" as CFString, 1, nil)!
    CGImageDestinationAddImage(d, c.makeImage()!, nil); CGImageDestinationFinalize(d)
}
try FileManager.default.copyItem(at: src.appendingPathComponent("delays.txt"), to: dst.appendingPathComponent("delays.txt"))
print("\(names.count) frames \(W)×\(H) → \(dst.path)")
