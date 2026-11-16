import AppKit
// Render an SVG to a PNG with AppKit. usage: svg2png <in.svg> <out.png> <width> [height]
let a = CommandLine.arguments, px = Int(a[3])!, py = a.count > 4 ? Int(a[4])! : px
guard let img = NSImage(contentsOf: URL(fileURLWithPath: a[1])) else { fatalError("cannot load \(a[1])") }
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: py, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState(); NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
img.draw(in: NSRect(x: 0, y: 0, width: px, height: py)); NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[2]))
