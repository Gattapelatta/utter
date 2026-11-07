// README hero: the island (record-overlay.swift --no-backdrop frames) on a wide rounded card over a
// picture background, optionally with a big word whose letters overlap and show each other's
// outlines → a folder of frames for frames2anim.py. One picture keeps the layout exact
// (GitHub's Markdown can't lay things out precisely).
// Usage: swift scripts/tools/hero.swift <frames-in> <frames-out> <background.jpg>
//          [--scene-x 20] [--word Utter --cap 500 --font HelveticaNeue-Medium --tracking -0.08 --right 40]
//   --scene-x  left edge of the island scene (default: centred)
//   --cap      height of the capital letters in px; 500 = from the bottom edge to the top edge
//   --tracking extra space between letters, in em (negative = letters overlap)
import AppKit

var a = CommandLine.arguments
func option(_ name: String) -> String? {
    guard let i = a.firstIndex(of: name), i + 1 < a.count else { return nil }
    let v = a[i + 1]; a.removeSubrange(i...(i + 1)); return v
}
let sceneX = option("--scene-x").flatMap { Int($0) }
let word = option("--word")
let cap = CGFloat(Double(option("--cap") ?? "") ?? 500)
let fontName = option("--font") ?? "HelveticaNeue-Medium"
let tracking = CGFloat(Double(option("--tracking") ?? "") ?? -0.08)
let right = CGFloat(Double(option("--right") ?? "") ?? 40)
guard a.count == 4 else { print("usage: hero <frames-in> <frames-out> <background> [options]"); exit(2) }
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
if let word {
    // Letters as outlines: filled white, then every letter's contour drawn thin and dark inside
    // the white, so where letters overlap you still see each one.
    let size = cap / (CTFontGetCapHeight(CTFontCreateWithName(fontName as CFString, 100, nil)) / 100)
    let font = CTFontCreateWithName(fontName as CFString, size, nil)
    let chars = Array(word.utf16)
    var glyphs = [CGGlyph](repeating: 0, count: chars.count)
    CTFontGetGlyphsForCharacters(font, chars, &glyphs, chars.count)
    var adv = [CGSize](repeating: .zero, count: glyphs.count)
    CTFontGetAdvancesForGlyphs(font, .horizontal, glyphs, &adv, glyphs.count)
    var paths: [CGPath] = []
    var x: CGFloat = 0
    for (i, g) in glyphs.enumerated() {
        var t = CGAffineTransform(translationX: x, y: 0)
        if let p = CTFontCreatePathForGlyph(font, g, &t) { paths.append(p) }
        x += adv[i].width + (i < glyphs.count - 1 ? tracking * size : 0)
    }
    let all = CGMutablePath(); paths.forEach { all.addPath($0) }
    let box = all.boundingBoxOfPath
    let baseline = (CGFloat(H) - cap) / 2  // cap 500 → baseline on the bottom edge
    var place = CGAffineTransform(translationX: CGFloat(W) - right - box.maxX, y: baseline)
    let placed = paths.map { $0.copy(using: &place)! }
    base.setFillColor(CGColor(gray: 1, alpha: 1))
    for p in placed { base.addPath(p); base.fillPath() }
    // Each letter's contour only where it runs over another letter (no dark rim on the outside).
    base.setStrokeColor(CGColor(gray: 0, alpha: 0.85)); base.setLineWidth(2.2)
    for (i, p) in placed.enumerated() {
        let others = CGMutablePath()
        for (j, q) in placed.enumerated() where j != i { others.addPath(q) }
        base.saveGState(); base.addPath(others); base.clip()
        base.addPath(p); base.strokePath(); base.restoreGState()
    }
    print("word: \(Int(box.width)) px wide at cap \(Int(cap)) px, starts at x \(Int(CGFloat(W) - right - box.width))")
}
let baseImage = base.makeImage()!

try? FileManager.default.removeItem(at: dst)
try FileManager.default.createDirectory(at: dst, withIntermediateDirectories: true)
let names = try FileManager.default.contentsOfDirectory(atPath: src.path).filter { $0.hasSuffix(".png") }.sorted()
for n in names {
    let scene = NSImage(contentsOf: src.appendingPathComponent(n))!.cgImage(forProposedRect: nil, context: nil, hints: nil)!
    let c = context()
    c.draw(baseImage, in: CGRect(x: 0, y: 0, width: W, height: H))
    c.addPath(card); c.clip()
    // The island hangs from the card's top edge.
    c.draw(scene, in: CGRect(x: sceneX ?? (W - scene.width) / 2, y: H - scene.height, width: scene.width, height: scene.height))
    let d = CGImageDestinationCreateWithURL(dst.appendingPathComponent(n) as CFURL, "public.png" as CFString, 1, nil)!
    CGImageDestinationAddImage(d, c.makeImage()!, nil); CGImageDestinationFinalize(d)
}
try FileManager.default.copyItem(at: src.appendingPathComponent("delays.txt"), to: dst.appendingPathComponent("delays.txt"))
print("\(names.count) frames \(W)×\(H) → \(dst.path)")
