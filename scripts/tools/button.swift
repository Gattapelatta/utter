// README download button as an SVG whose text is drawn as shapes (glyph outlines from the system
// font), so it looks the same on every computer, whatever fonts it has.
// Usage: swift scripts/tools/button.swift <out.svg> <label> <note>
//   e.g. button.swift docs/media/download-ru.svg "Скачать для Mac" "macOS 13+"
import AppKit

let a = CommandLine.arguments
guard a.count == 4 else { print("usage: button <out.svg> <label> <note>"); exit(2) }
let H: CGFloat = 48, pad: CGFloat = 20, iconW: CGFloat = 18, gap: CGFloat = 10, size: CGFloat = 16

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

/// The text's outline, baseline at y = 0, flipped for SVG (y down); returns path and width.
func textPath(_ s: String, _ font: NSFont) -> (CGPath, CGFloat) {
    let line = CTLineCreateWithAttributedString(NSAttributedString(string: s, attributes: [.font: font]))
    let out = CGMutablePath()
    for run in CTLineGetGlyphRuns(line) as! [CTRun] {
        let n = CTRunGetGlyphCount(run)
        var glyphs = [CGGlyph](repeating: 0, count: n), pos = [CGPoint](repeating: .zero, count: n)
        CTRunGetGlyphs(run, CFRange(location: 0, length: n), &glyphs)
        CTRunGetPositions(run, CFRange(location: 0, length: n), &pos)
        let runFont = (CTRunGetAttributes(run) as NSDictionary)[kCTFontAttributeName] as! CTFont
        for i in 0..<n {
            var t = CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: pos[i].x, ty: 0)
            if let g = CTFontCreatePathForGlyph(runFont, glyphs[i], &t) { out.addPath(g) }
        }
    }
    return (out, CTLineGetTypographicBounds(line, nil, nil, nil))
}

let (label, lw) = textPath(a[2], .systemFont(ofSize: size, weight: .semibold))
let (note, nw) = textPath("·  " + a[3], .systemFont(ofSize: size, weight: .regular))
let W = (pad + iconW + gap + lw + 10 + nw + pad).rounded(.up)
let baseline = (H / 2 + size * 0.36).rounded()
let labelX = pad + iconW + gap, noteX = labelX + lw + 10
let iconX = pad, iconY = (H - 20) / 2
let svg = """
<svg xmlns="http://www.w3.org/2000/svg" width="\(Int(W))" height="\(Int(H))" viewBox="0 0 \(Int(W)) \(Int(H))">
  <rect x="0.5" y="0.5" width="\(Int(W) - 1)" height="\(Int(H) - 1)" rx="12" fill="#000" stroke="#3d444d"/>
  <g fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" transform="translate(\(iconX) \(iconY))">
    <path d="M9 2v11M4.5 8.5 9 13l4.5-4.5M2 18h14"/>
  </g>
  <path fill="#fff" transform="translate(\(labelX) \(baseline))" d="\(svgPath(label))"/>
  <path fill="#8b949e" transform="translate(\(noteX) \(baseline))" d="\(svgPath(note))"/>
</svg>
"""
try svg.write(toFile: a[1], atomically: true, encoding: .utf8)
print("\(a[1]): \(Int(W))×\(Int(H))")
