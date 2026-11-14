// Draws the OpenPlug app icon — an OBD connector: a trapezoid outline with two rows
// of pins, one of them lit — and writes the PNGs the Apple asset catalog needs.
//
//   swift tools/make_icons.swift apple/App/Assets.xcassets/AppIcon.appiconset
//
// The same geometry is in docs/logo.svg and in the Android vector drawable
// (android/app/src/main/res/drawable/ic_launcher_foreground.xml); change all three
// together.
import AppKit

let ground = NSColor(srgbRed: 0x0B / 255, green: 0x6E / 255, blue: 0x99 / 255, alpha: 1)
let lit = NSColor(srgbRed: 0x7C / 255, green: 0xC7 / 255, blue: 0xE8 / 255, alpha: 1)

/// `mac`: the rounded, inset tile macOS icons use; otherwise a full-bleed square
/// (iOS masks it itself).
func icon(size: Int, mac: Bool) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let g = NSGraphicsContext.current!.cgContext
    let s = CGFloat(size) / 1024
    g.scaleBy(x: s, y: s)
    g.translateBy(x: 0, y: 1024)
    g.scaleBy(x: 1, y: -1) // y down, as in the SVG

    ground.setFill()
    if mac {
        NSBezierPath(roundedRect: NSRect(x: 100, y: 100, width: 824, height: 824), xRadius: 185, yRadius: 185).fill()
        g.translateBy(x: 512, y: 512); g.scaleBy(x: 0.8, y: 0.8); g.translateBy(x: -512, y: -512)
    } else {
        g.fill(CGRect(x: 0, y: 0, width: 1024, height: 1024))
    }

    // the connector: wider at the top, like an OBD-II socket
    g.setStrokeColor(NSColor.white.cgColor)
    g.setLineWidth(56)
    g.setLineJoin(.round)
    g.addLines(between: [CGPoint(x: 212, y: 347), CGPoint(x: 812, y: 347), CGPoint(x: 732, y: 677), CGPoint(x: 292, y: 677)])
    g.closePath()
    g.strokePath()

    // pins: four above, three below; the middle one below is lit
    func pin(_ x: CGFloat, _ y: CGFloat, _ color: NSColor) {
        g.setFillColor(color.cgColor)
        g.fillEllipse(in: CGRect(x: x - 30, y: y - 30, width: 60, height: 60))
    }
    for x in [347, 457, 567, 677] as [CGFloat] { pin(x, 450, .white) }
    pin(402, 574, .white); pin(512, 574, lit); pin(622, 574, .white)

    NSGraphicsContext.current = nil
    return rep.representation(using: .png, properties: [:])!
}

let out = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? ".", isDirectory: true)
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
try icon(size: 1024, mac: false).write(to: out.appendingPathComponent("ios-1024.png"))
for size in [16, 32, 64, 128, 256, 512, 1024] {
    try icon(size: size, mac: true).write(to: out.appendingPathComponent("mac-\(size).png"))
}
print("icons written to \(out.path)")
