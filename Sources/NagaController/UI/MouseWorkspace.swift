import Cocoa
import SwiftUI

struct MouseWorkspace: View {
    @Binding var selectedButton: Int
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var topView = false

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                VStack(spacing: 16) {
                    Picker("Mouse view", selection: $topView) {
                        Text("Side").tag(false)
                        Text("Top").tag(true)
                    }.pickerStyle(.segmented).labelsHidden().frame(width: 232)
                    MouseDiagram(selectedButton: $selectedButton, topView: topView)
                        .frame(height: max(220, min(440, geometry.size.height - 315)))
                    HStack {
                        Text(topView ? "Top buttons" : "12-button side panel")
                            .font(.system(size: 12, weight: .semibold))
                        Spacer()
                        Text("Select on the photo or in the list")
                            .font(.system(size: 11)).foregroundStyle(.secondary)
                    }
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 6), count: topView ? 2 : 3), spacing: 6) {
                        ForEach(topView ? Array(13...19) : Array(1...12), id: \.self) { index in
                            assignment(index)
                        }
                    }
                }.padding(22)
            }
        }
        .onChange(of: selectedButton) { value in topView = value >= 13 }
    }

    private func assignment(_ index: Int) -> some View {
        Button { selectedButton = index } label: {
            HStack(spacing: 8) {
                Text("\(index)").font(.system(size: 12, weight: .medium, design: .rounded))
                    .foregroundStyle(selectedButton == index ? UIStyle.accent : .secondary)
                    .frame(width: 23, height: 26)
                    .background(selectedButton == index ? UIStyle.accent.opacity(0.10) : UIStyle.inset)
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                VStack(alignment: .leading, spacing: 2) {
                    if index >= 13 { Text(buttonName(index)).font(.system(size: 11, weight: .medium)).lineLimit(1) }
                    Text(model.mapping[index]?.displayName ?? "Original")
                        .font(.system(size: 11)).foregroundStyle(index >= 13 ? .secondary : .primary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }.padding(.horizontal, 8).frame(height: 40)
                .background(selectedButton == index ? UIStyle.selection : UIStyle.inset.opacity(0.45))
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(
                    model.activeButton == index ? UIStyle.accent : selectedButton == index ? UIStyle.accent.opacity(0.5) : .clear,
                    lineWidth: model.activeButton == index ? 2 : 1))
                .contentShape(Rectangle())
        }.buttonStyle(.plain)
            .help("\(buttonName(index)): \(model.mapping[index]?.displayName ?? "Original")")
            .accessibilityLabel("\(buttonName(index)), \(model.mapping[index]?.displayName ?? "Original")")
            .accessibilityAddTraits(selectedButton == index ? [.isSelected] : [])
    }
}

/// Coordinates belong to the bundled images and remain proportional on resize.
struct MouseHotspot: Identifiable {
    let id: Int
    let points: [CGPoint]

    static func sideButton(at point: CGPoint, imageSize: CGSize) -> Int? {
        guard imageSize.width > 0, imageSize.height > 0 else { return nil }
        return side.first {
            HotspotShape(points: $0.points).path(in: CGRect(origin: .zero, size: imageSize)).contains(point)
        }?.id
    }

    static let side: [MouseHotspot] = [
        .key(1, [(122,648),(166,626),(183,697),(133,716)]),
        .key(2, [(168,623),(219,599),(240,678),(186,698)]),
        .key(3, [(222,596),(289,567),(319,648),(243,678)]),
        .key(4, [(129,745),(171,723),(184,792),(138,811)]),
        .key(5, [(175,721),(235,697),(249,777),(187,794)]),
        .key(6, [(238,694),(307,671),(339,749),(252,776)]),
        .key(7, [(132,840),(177,818),(186,888),(144,908)]),
        .key(8, [(180,817),(238,793),(254,870),(190,891)]),
        .key(9, [(241,791),(317,767),(348,844),(256,872)]),
        .key(10, [(141,938),(184,918),(193,988),(153,1006)]),
        .key(11, [(187,914),(242,895),(257,969),(195,990)]),
        .key(12, [(245,891),(319,871),(345,943),(259,967)])
    ]

    private static func key(_ id: Int, _ pairs: [(Double, Double)]) -> MouseHotspot {
        MouseHotspot(id: id, points: pairs.map { CGPoint(x: $0.0 / 1024, y: $0.1 / 1536) })
    }
}

private struct HotspotShape: Shape {
    let points: [CGPoint]
    func path(in rect: CGRect) -> Path {
        Path { path in
            for (index, point) in points.enumerated() {
                let scaled = CGPoint(x: point.x * rect.width, y: point.y * rect.height)
                if index == 0 { path.move(to: scaled) } else { path.addLine(to: scaled) }
            }
            path.closeSubpath()
        }
    }
}

struct MouseDiagram: View {
    @Binding var selectedButton: Int
    let topView: Bool
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var hovered: Int?
    private static let perspective = load("naga-perspective")
    private static let top = load("naga-top")

    var body: some View {
        GeometryReader { geometry in
            let size = imageSize(in: geometry.size)
            ZStack {
                if let image = topView ? Self.top : Self.perspective {
                    Image(nsImage: image).resizable().interpolation(.high)
                        .frame(width: size.width, height: size.height)
                        .accessibilityHidden(true)
                } else {
                    Label("Mouse photo unavailable", systemImage: "computermouse")
                        .foregroundStyle(.secondary)
                }
                if topView {
                    topControls(size: size)
                } else {
                    ForEach(MouseHotspot.side) { hotspot in
                        let highlighted = selectedButton == hotspot.id || model.activeButton == hotspot.id
                        Button { selectedButton = hotspot.id } label: {
                            HotspotShape(points: hotspot.points)
                                .fill(highlighted ? Color(red: 0.45, green: 0.92, blue: 0.30).opacity(0.35) : .clear)
                                .overlay(HotspotShape(points: hotspot.points).stroke(
                                    highlighted ? Color(red: 0.55, green: 1, blue: 0.39) : hovered == hotspot.id ? Color.white.opacity(0.65) : .clear,
                                    lineWidth: model.activeButton == hotspot.id ? 2.5 : 1.5))
                                .contentShape(HotspotShape(points: hotspot.points))
                        }.buttonStyle(.plain).frame(width: size.width, height: size.height)
                            .help(buttonName(hotspot.id))
                            .accessibilityLabel("Select \(buttonName(hotspot.id))")
                    }
                }
            }.frame(width: geometry.size.width, height: geometry.size.height)
                .onContinuousHover { phase in
                    switch phase {
                    case .active(let location):
                        let imagePoint = CGPoint(
                            x: location.x - (geometry.size.width - size.width) / 2,
                            y: location.y - (geometry.size.height - size.height) / 2)
                        hovered = topView ? nil : MouseHotspot.sideButton(at: imagePoint, imageSize: size)
                    case .ended:
                        hovered = nil
                    }
                }
        }
        .onChange(of: topView) { _ in hovered = nil }
    }

    private func imageSize(in available: CGSize) -> CGSize {
        let ratio: CGFloat = topView ? 1.5 : 2.0 / 3.0
        let height = min(available.height, available.width / ratio)
        return CGSize(width: height * ratio, height: height)
    }

    private func topControls(size: CGSize) -> some View {
        // The official top photograph is 1500 x 1000, including its transparent margins.
        let positions: [(Int, CGFloat, CGFloat)] = [
            (13, 0.346, 0.125), (14, 0.350, 0.227),
            (18, 0.420, 0.320), (19, 0.595, 0.320),
            (15, 0.447, 0.240), (17, 0.501, 0.240), (16, 0.555, 0.240)
        ]
        return ZStack(alignment: .topLeading) {
            ForEach(positions, id: \.0) { index, x, y in
                Button { selectedButton = index } label: {
                    Text(index == 15 ? "←" : index == 16 ? "→" : "\(index)")
                        .font(.system(size: 10, weight: .semibold, design: .rounded))
                        .foregroundStyle(selectedButton == index ? Color.black : Color.white)
                        .frame(width: 23, height: 23)
                        .background(selectedButton == index ? Color(red: 0.55, green: 1, blue: 0.39) : Color.black.opacity(0.75))
                        .clipShape(Circle())
                        .overlay(Circle().stroke(Color.white.opacity(0.75), lineWidth: model.activeButton == index ? 2.5 : 1))
                }.buttonStyle(.plain)
                    .position(x: x * size.width, y: y * size.height)
                    .help(buttonName(index)).accessibilityLabel("Select \(buttonName(index))")
            }
        }.frame(width: size.width, height: size.height)
    }

    private static func load(_ name: String) -> NSImage? {
        if let url = Bundle.main.url(forResource: name, withExtension: "png", subdirectory: "Mouse") {
            return NSImage(contentsOf: url)
        }
        // Supports local command-line UI snapshots from the project checkout.
        return NSImage(contentsOfFile: FileManager.default.currentDirectoryPath + "/Resources/Mouse/\(name).png")
    }
}
