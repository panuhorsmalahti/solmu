import SwiftUI

@main
struct SolmuApp: App {
    var body: some Scene {
        WindowGroup {
            SolmuRootView()
        }
    }
}

struct SolmuRootView: View {
    @AppStorage("backendURL") private var backendURL = ""
    @State private var address = "http://127.0.0.1:3000"
    @State private var error = ""

    private var testBackend: String? {
        let arguments = ProcessInfo.processInfo.arguments
        guard let index = arguments.firstIndex(of: "--backend"), arguments.indices.contains(index + 1) else { return nil }
        return arguments[index + 1]
    }

    var body: some View {
        Group {
            if let testBackend {
                SolmuHomeView(baseURL: testBackend, onDisconnect: {})
            } else if backendURL.isEmpty {
                connectView
            } else {
                SolmuHomeView(baseURL: backendURL, onDisconnect: { backendURL = "" })
            }
        }
        .tint(SolmuPalette.green)
    }

    private var connectView: some View {
        NavigationStack {
            Form {
                Section {
                    Text("solmu")
                        .font(.system(size: 40, weight: .bold, design: .rounded))
                    Text("YOUR IDEAS, CONNECTED")
                        .font(.caption.weight(.medium))
                        .tracking(1.5)
                        .foregroundStyle(SolmuPalette.muted)
                }
                Section("Connect to your backend") {
                    TextField("http://192.168.1.20:3000", text: $address)
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("backend-address")
                    Text("Enter the network address of the computer running Solmu. The backend must listen on that network interface.")
                        .font(.footnote)
                        .foregroundStyle(SolmuPalette.muted)
                    if !error.isEmpty { Text(error).foregroundStyle(.red).font(.footnote) }
                    Button("Connect") {
                        let value = address.trimmingCharacters(in: .whitespacesAndNewlines).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                        guard let url = URL(string: value), ["http", "https"].contains(url.scheme?.lowercased() ?? ""), url.host != nil, url.user == nil, url.password == nil, url.query == nil, url.fragment == nil else {
                            error = "Enter a valid http:// or https:// backend address."
                            return
                        }
                        backendURL = value
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("connect-backend")
                }
                Section {
                    Text("On the same Wi-Fi network, use your computer’s LAN address. The backend currently has no login, so only expose it to a network you trust.")
                        .font(.footnote)
                        .foregroundStyle(SolmuPalette.muted)
                }
            }
            .navigationTitle("Welcome")
        }
    }
}

enum SolmuPalette {
    static let green = Color(red: 39.0 / 255, green: 101.0 / 255, blue: 81.0 / 255)
    static let ink = Color(red: 38.0 / 255, green: 60.0 / 255, blue: 54.0 / 255)
    static let muted = Color(red: 117.0 / 255, green: 132.0 / 255, blue: 122.0 / 255)
    static let paper = Color(red: 250.0 / 255, green: 251.0 / 255, blue: 248.0 / 255)
    static let sidebar = Color(red: 240.0 / 255, green: 243.0 / 255, blue: 236.0 / 255)
    static let border = Color(red: 223.0 / 255, green: 230.0 / 255, blue: 221.0 / 255)
    static let soft = Color(red: 237.0 / 255, green: 241.0 / 255, blue: 234.0 / 255)
}
