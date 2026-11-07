// Records only Utter's overlay window (ScreenCaptureKit) into a looping GIF for the README, over a
// drawn backdrop: a dark screen, a menu bar and the camera notch. Nothing else on the screen is
// captured. Optionally writes a file when recording starts (e.g. the dev build's debug-draft.txt).
// Usage: swift scripts/tools/record-overlay.swift <out.gif | frames-dir> <seconds> [--scale 1] [--fps 60]
//        [--notch 179x32] [--start-file <path> <text>]
import AppKit
import CoreImage
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers

var args = Array(CommandLine.arguments.dropFirst())
func take(_ name: String, _ n: Int) -> [String]? {
    guard let i = args.firstIndex(of: name), i + n < args.count else { return nil }
    let v = Array(args[(i + 1)...(i + n)])
    args.removeSubrange(i...(i + n))
    return v
}
let scale = Double(take("--scale", 1)?.first ?? "") ?? 1
let fps = Double(take("--fps", 1)?.first ?? "") ?? 60
let notchArg = (take("--notch", 1)?.first ?? "179x32").split(separator: "x").compactMap { Double($0) }
let startFile = take("--start-file", 2)
guard args.count == 2, let seconds = Double(args[1]) else {
    FileHandle.standardError.write("usage: record-overlay <out.gif> <seconds> [--scale x] [--fps n] [--notch WxH] [--start-file path text]\n".data(using: .utf8)!)
    exit(2)
}

final class Frames: NSObject, SCStreamOutput {
    var images: [(CGImage, Double)] = []
    let ctx = CIContext()
    func stream(_ stream: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, let pb = sb.imageBuffer else { return }
        // Only complete frames carry a new picture (idle frames repeat the last one).
        if let info = (CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]])?.first,
           let raw = info[.status] as? Int, raw != SCFrameStatus.complete.rawValue { return }
        let ci = CIImage(cvPixelBuffer: pb)
        guard let cg = ctx.createCGImage(ci, from: ci.extent) else { return }
        images.append((cg, CMTimeGetSeconds(sb.presentationTimeStamp)))
    }
}

let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
guard let win = content.windows.first(where: { $0.title == "Utter Overlay" }) else {
    FileHandle.standardError.write("Utter's overlay window not found — is Utter running?\n".data(using: .utf8)!)
    exit(1)
}
let pt = win.frame.size
let cfg = SCStreamConfiguration()
cfg.width = Int(pt.width * 2)
cfg.height = Int(pt.height * 2)
cfg.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(fps))
cfg.showsCursor = false
cfg.pixelFormat = kCVPixelFormatType_32BGRA
cfg.backgroundColor = .clear
cfg.ignoreShadowsSingleWindow = true
let frames = Frames()
let stream = SCStream(filter: SCContentFilter(desktopIndependentWindow: win), configuration: cfg, delegate: nil)
try stream.addStreamOutput(frames, type: .screen, sampleHandlerQueue: DispatchQueue(label: "frames"))
try await stream.startCapture()
if let s = startFile { try s[1].write(toFile: s[0], atomically: true, encoding: .utf8) }
try await Task.sleep(nanoseconds: UInt64(seconds * 1e9))
try await stream.stopCapture()

// Backdrop in points ×2, then the whole picture scaled.
let W = Int(pt.width * 2 * scale), H = Int(pt.height * 2 * scale)
let k = 2 * scale
func backdrop(_ c: CGContext) {
    c.setFillColor(CGColor(srgbRed: 0.12, green: 0.13, blue: 0.16, alpha: 1))
    c.fill(CGRect(x: 0, y: 0, width: W, height: H))
    c.setFillColor(CGColor(srgbRed: 0.17, green: 0.18, blue: 0.21, alpha: 1))
    c.fill(CGRect(x: 0, y: Double(H) - 37 * k, width: Double(W), height: 37 * k)) // menu bar
    let nw = notchArg.first ?? 179, nh = notchArg.count > 1 ? notchArg[1] : 32
    let notch = CGRect(x: (Double(W) - nw * k) / 2, y: Double(H) - nh * k, width: nw * k, height: nh * k)
    let r = 8 * k
    let p = CGMutablePath()
    p.move(to: CGPoint(x: notch.minX, y: notch.maxY))
    p.addLine(to: CGPoint(x: notch.minX, y: notch.minY + r))
    p.addQuadCurve(to: CGPoint(x: notch.minX + r, y: notch.minY), control: CGPoint(x: notch.minX, y: notch.minY))
    p.addLine(to: CGPoint(x: notch.maxX - r, y: notch.minY))
    p.addQuadCurve(to: CGPoint(x: notch.maxX, y: notch.minY + r), control: CGPoint(x: notch.maxX, y: notch.minY))
    p.addLine(to: CGPoint(x: notch.maxX, y: notch.maxY))
    p.closeSubpath()
    c.setFillColor(CGColor(gray: 0, alpha: 1))
    c.addPath(p)
    c.fillPath()
}
// <out>.gif → a GIF; any other path → a folder of PNG frames + delays.txt (seconds per frame),
// for an encoder that keeps every colour (e.g. scripts/tools/frames2webp).
let out = URL(fileURLWithPath: args[0])
let asGif = out.pathExtension.lowercased() == "gif"
var gif: CGImageDestination?
if asGif {
    gif = CGImageDestinationCreateWithURL(out as CFURL, UTType.gif.identifier as CFString, frames.images.count, nil)
    guard let gif else { exit(1) }
    CGImageDestinationSetProperties(gif, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
} else {
    try? FileManager.default.removeItem(at: out)
    try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
}
var delays: [String] = []
for (i, (img, t)) in frames.images.enumerated() {
    let next = i + 1 < frames.images.count ? frames.images[i + 1].1 : t + 1.5 // hold the last frame
    // A GIF can't show a frame shorter than 20 ms; the frames folder keeps real timing (60 fps = 16.7 ms).
    let delay = asGif ? max(0.02, next - t) : next - t
    guard let c = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { continue }
    c.interpolationQuality = .high
    backdrop(c)
    c.draw(img, in: CGRect(x: 0, y: 0, width: W, height: H))
    guard let composed = c.makeImage() else { continue }
    if let gif {
        CGImageDestinationAddImage(gif, composed, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: delay, kCGImagePropertyGIFUnclampedDelayTime: delay]] as CFDictionary)
    } else {
        let url = out.appendingPathComponent(String(format: "frame_%04d.png", delays.count))
        guard let d = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else { exit(1) }
        CGImageDestinationAddImage(d, composed, nil)
        guard CGImageDestinationFinalize(d) else { exit(1) }
        delays.append(String(format: "%.5f", delay))
    }
}
if let gif { guard CGImageDestinationFinalize(gif) else { exit(1) } }
else { try delays.joined(separator: "\n").write(to: out.appendingPathComponent("delays.txt"), atomically: true, encoding: .utf8) }
print("\(frames.images.count) frames, \(W)×\(H) → \(out.path)")
