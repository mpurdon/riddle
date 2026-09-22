// Prove the window can actually be moved by dragging its title bar.
//
// A window nobody can drag is the kind of defect that unit tests and
// screenshots both miss, so this synthesizes a real left-button drag across
// the top bar with CGEvent and checks the window moved. Run it with the app
// already open and frontmost:
//
//     swift scripts/check-window.swift [process-name]
//
// Needs Accessibility permission for the terminal (System Settings →
// Privacy & Security → Accessibility), the same permission screenshots of
// window bounds need.

import Cocoa

let processName = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "riddle-desktop"

func windowFrame() -> CGRect? {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
        return nil
    }
    for w in list {
        guard let owner = w[kCGWindowOwnerName as String] as? String, owner == processName,
              let bounds = w[kCGWindowBounds as String] as? [String: Any],
              let x = bounds["X"] as? CGFloat, let y = bounds["Y"] as? CGFloat,
              let width = bounds["Width"] as? CGFloat, let height = bounds["Height"] as? CGFloat,
              width > 200, height > 200
        else { continue }
        return CGRect(x: x, y: y, width: width, height: height)
    }
    return nil
}

func post(_ type: CGEventType, _ point: CGPoint) {
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left)
    e?.post(tap: .cghidEventTap)
    usleep(40_000)
}

guard let before = windowFrame() else {
    print("FAIL: no on-screen window owned by \(processName)")
    exit(2)
}

// A point in the top bar, left of the centred search box and right of the
// Dig switcher and the traffic lights: empty drag surface either way.
let grab = CGPoint(x: before.minX + 400, y: before.minY + 22)
let dx: CGFloat = 120
let dy: CGFloat = 80

print("window at \(Int(before.minX)),\(Int(before.minY)) \(Int(before.width))x\(Int(before.height)); grabbing at \(Int(grab.x)),\(Int(grab.y))")

post(.mouseMoved, grab)
post(.leftMouseDown, grab)
// Several small steps: one jump can be swallowed as a click.
for i in 1...6 {
    let t = CGFloat(i) / 6
    post(.leftMouseDragged, CGPoint(x: grab.x + dx * t, y: grab.y + dy * t))
}
post(.leftMouseUp, CGPoint(x: grab.x + dx, y: grab.y + dy))
usleep(300_000)

guard let after = windowFrame() else {
    print("FAIL: window disappeared during the drag")
    exit(2)
}

let movedX = after.minX - before.minX
let movedY = after.minY - before.minY
print("moved by \(Int(movedX)),\(Int(movedY)) (asked for \(Int(dx)),\(Int(dy)))")

// Put it back so the check leaves no trace.
if movedX != 0 || movedY != 0 {
    let home = CGPoint(x: after.minX + 400, y: after.minY + 22)
    post(.mouseMoved, home)
    post(.leftMouseDown, home)
    for i in 1...6 {
        let t = CGFloat(i) / 6
        post(.leftMouseDragged, CGPoint(x: home.x - dx * t, y: home.y - dy * t))
    }
    post(.leftMouseUp, CGPoint(x: home.x - dx, y: home.y - dy))
}

if abs(movedX - dx) <= 12 && abs(movedY - dy) <= 12 {
    print("PASS: the title bar drags the window")
    exit(0)
}
print("FAIL: the window did not follow the drag — the title bar is not a drag region")
exit(1)
