// A small line of text for the README as an SVG of glyph outlines — GitHub paints every text link
// blue and drops inline colours, so a grey link has to be a picture.
// Usage: swift scripts/tools/label.swift <out.svg> <size> <segment>...
//   segment = "text|regular|#8b949e" or "text|semibold|#f0f6fc"; segments follow each other.
import AppKit

let a = CommandLine.arguments
guard a.count >= 4, let size = Double(a[2]) else { print("usage: label <out.svg> <size> <text|weight|#color>..."); exit(2) }
let fs = CGFloat(size), H = (fs * 1.6).rounded()

func svgPath(_ p: CGPath) -> String {
    var d = ""
    func f(_ v: CGFloat) -> String { String(format: "%.2f", v) }
    p.applyWithBlock { e in
        let pts = e.pointee.points
        switch e.pointee.type {
        case .moveToPoint: d += "M\(f(pts[0].x)) \(f(pts[0].y))"
        case .addLineToPoint: d += "L\(f(pts[0].x)) \(f(pts[0].y))"
        case .addQuadCurveToPoint: d += "Q\(f(pts[0].x)) \(f(pts[0].y)) \(f(pts[1].x)) \(f(pts[1].y))"
        case .addCurveToPoint: d += "C\(f(pts[0].x)) \(f(pts[0].y)) \(f(pts[1].x)) \(f(pts[1].y)) \(f(pts[2].x)) \(f(pts[2].y))"
        case .closeSubpath: d += "Z"
        @unknown default: break
        }
    }
    return d
}

var x: CGFloat = 0, parts: [String] = []
for seg in a.dropFirst(3) {
    let f = seg.split(separator: "|", omittingEmptySubsequences: false).map(String.init)
    let font = NSFont.systemFont(ofSize: fs, weight: f[1] == "semibold" ? .semibold : .regular)
    let line = CTLineCreateWithAttributedString(NSAttributedString(string: f[0], attributes: [.font: font]))
    let path = CGMutablePath()
    for run in CTLineGetGlyphRuns(line) as! [CTRun] {
        let n = CTRunGetGlyphCount(run)
        var g = [CGGlyph](repeating: 0, count: n), pos = [CGPoint](repeating: .zero, count: n)
        CTRunGetGlyphs(run, CFRange(location: 0, length: n), &g)
        CTRunGetPositions(run, CFRange(location: 0, length: n), &pos)
        let rf = (CTRunGetAttributes(run) as NSDictionary)[kCTFontAttributeName] as! CTFont
        for i in 0..<n {
            var t = CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: pos[i].x, ty: 0)
            if let p = CTFontCreatePathForGlyph(rf, g[i], &t) { path.addPath(p) }
        }
    }
    parts.append("<path fill=\"\(f[2])\" transform=\"translate(\(String(format: "%.2f", x)) \((H / 2 + fs * 0.36).rounded()))\" d=\"\(svgPath(path))\"/>")
    x += CTLineGetTypographicBounds(line, nil, nil, nil)
}
let W = Int(x.rounded(.up))
try "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"\(W)\" height=\"\(Int(H))\" viewBox=\"0 0 \(W) \(Int(H))\">\n\(parts.joined(separator: "\n"))\n</svg>\n".write(toFile: a[1], atomically: true, encoding: .utf8)
print("\(a[1]): \(W)×\(Int(H))")
