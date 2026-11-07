// Turns a screen recording into a looping GIF for the README (ffmpeg is not on this Mac).
// Usage: swift scripts/tools/mov2gif.swift <in.mov> <out.gif> [--fps 30] [--scale 1.0] [--from 0] [--to <end s>]
import AVFoundation
import ImageIO
import UniformTypeIdentifiers

var args = Array(CommandLine.arguments.dropFirst())
func option(_ name: String, _ fallback: Double) -> Double {
    guard let i = args.firstIndex(of: name), i + 1 < args.count, let v = Double(args[i + 1]) else { return fallback }
    args.removeSubrange(i...(i + 1))
    return v
}
let fps = option("--fps", 30), scale = option("--scale", 1), from = option("--from", 0), toArg = option("--to", -1)
guard args.count == 2 else {
    FileHandle.standardError.write("usage: mov2gif <in.mov> <out.gif> [--fps n] [--scale x] [--from s] [--to s]\n".data(using: .utf8)!)
    exit(2)
}
let asset = AVURLAsset(url: URL(fileURLWithPath: args[0]))
let duration = CMTimeGetSeconds(asset.duration)
let end = toArg > 0 ? min(toArg, duration) : duration
let gen = AVAssetImageGenerator(asset: asset)
gen.requestedTimeToleranceBefore = .zero
gen.requestedTimeToleranceAfter = .zero
gen.appliesPreferredTrackTransform = true
let times = stride(from: from, to: end, by: 1 / fps).map { CMTime(seconds: $0, preferredTimescale: 600) }
guard let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: args[1]) as CFURL, UTType.gif.identifier as CFString, times.count, nil) else { exit(1) }
CGImageDestinationSetProperties(dest, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
let frameProps = [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: 1 / fps, kCGImagePropertyGIFUnclampedDelayTime: 1 / fps]] as CFDictionary
for t in times {
    guard var img = try? gen.copyCGImage(at: t, actualTime: nil) else { continue }
    if scale != 1, let ctx = CGContext(data: nil, width: Int(Double(img.width) * scale), height: Int(Double(img.height) * scale),
                                       bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                       bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) {
        ctx.interpolationQuality = .high
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: ctx.width, height: ctx.height))
        img = ctx.makeImage() ?? img
    }
    CGImageDestinationAddImage(dest, img, frameProps)
}
guard CGImageDestinationFinalize(dest) else { exit(1) }
print("\(times.count) frames → \(args[1])")
