// README hero: the recorded island scene (record-overlay.swift frames) on the left, the name and
// tagline on the right, drawn once into every frame → a folder of frames for frames2anim.py.
// One picture keeps the layout exact (GitHub's Markdown can't place text beside an image well).
// Usage: swift scripts/tools/hero.swift <frames-in> <frames-out> <icon.png> <title> <line1> <line2> <muted>
import AppKit

let a = CommandLine.arguments
guard a.count == 8 else { print("usage: hero <in> <out> <icon> <title> <line1> <line2> <muted>"); exit(2) }
let (src, dst) = (URL(fileURLWithPath: a[1]), URL(fileURLWithPath: a[2]))
let W = 1660, H = 500, sceneX = 36, radius: CGFloat = 28
let screen = CGColor(srgbRed: 0.12, green: 0.13, blue: 0.16, alpha: 1)   // same as record-overlay's backdrop
let menuBar = CGColor(srgbRed: 0.17, green: 0.18, blue: 0.21, alpha: 1)
let sRGB = CGColorSpace(name: CGColorSpace.sRGB)!

func context() -> CGContext {
    CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: sRGB,
              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
}

// Static layer: rounded card, menu bar across, icon + title + tagline on the right.
let base = context()
let card = CGPath(roundedRect: CGRect(x: 0, y: 0, width: W, height: H), cornerWidth: radius, cornerHeight: radius, transform: nil)
base.addPath(card); base.clip()
base.setFillColor(screen); base.fill(CGRect(x: 0, y: 0, width: W, height: H))
base.setFillColor(menuBar); base.fill(CGRect(x: 0, y: H - 74, width: W, height: 74))
NSGraphicsContext.current = NSGraphicsContext(cgContext: base, flipped: false)
let textX: CGFloat = CGFloat(sceneX + 880 + 70)
func draw(_ s: String, size: CGFloat, weight: NSFont.Weight, color: NSColor, top: CGFloat) -> CGFloat {
    let attr: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color]
    let str = NSAttributedString(string: s, attributes: attr)
    let h = str.size().height
    str.draw(at: NSPoint(x: textX, y: CGFloat(H) - top - h))
    return top + h
}
let icon = NSImage(contentsOfFile: a[3])!
let blockTop: CGFloat = 74 + 40
icon.draw(in: NSRect(x: textX, y: CGFloat(H) - blockTop - 84, width: 84, height: 84))
var y = blockTop + 84 + 14
y = draw(a[4], size: 92, weight: .semibold, color: .white, top: y) + 10
y = draw(a[5], size: 34, weight: .medium, color: NSColor(white: 0.93, alpha: 1), top: y) + 2
y = draw(a[6], size: 34, weight: .medium, color: NSColor(white: 0.93, alpha: 1), top: y) + 14
_ = draw(a[7], size: 28, weight: .regular, color: NSColor(srgbRed: 0.56, green: 0.58, blue: 0.63, alpha: 1), top: y)
NSGraphicsContext.current = nil
let baseImage = base.makeImage()!

try? FileManager.default.removeItem(at: dst)
try FileManager.default.createDirectory(at: dst, withIntermediateDirectories: true)
let names = try FileManager.default.contentsOfDirectory(atPath: src.path).filter { $0.hasSuffix(".png") }.sorted()
for n in names {
    let scene = NSImage(contentsOf: src.appendingPathComponent(n))!.cgImage(forProposedRect: nil, context: nil, hints: nil)!
    let c = context()
    c.draw(baseImage, in: CGRect(x: 0, y: 0, width: W, height: H))
    c.addPath(card); c.clip()
    c.draw(scene, in: CGRect(x: sceneX, y: H - scene.height, width: scene.width, height: scene.height))
    let out = c.makeImage()!
    let d = CGImageDestinationCreateWithURL(dst.appendingPathComponent(n) as CFURL, "public.png" as CFString, 1, nil)!
    CGImageDestinationAddImage(d, out, nil); CGImageDestinationFinalize(d)
}
try FileManager.default.copyItem(at: src.appendingPathComponent("delays.txt"), to: dst.appendingPathComponent("delays.txt"))
print("\(names.count) frames \(W)×\(H) → \(dst.path)")
