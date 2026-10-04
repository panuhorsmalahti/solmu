import Foundation
import Combine

typealias SolmuJSON = [String: Any]

extension Dictionary where Key == String, Value == Any {
    func string(_ key: String, _ fallback: String = "") -> String {
        guard let value = self[key], !(value is NSNull) else { return fallback }
        return String(describing: value)
    }
    func optionalString(_ key: String, _ fallback: String? = nil) -> String? {
        let value = string(key)
        return value.isEmpty ? fallback : value
    }
    var stableID: String { string("id") }
    var toolName: String { string("name") }
    var issuePath: String { string("path") }
    var issueMessage: String { string("message") }
    func bool(_ key: String, _ fallback: Bool = false) -> Bool { self[key] as? Bool ?? fallback }
    func array(_ key: String) -> [SolmuJSON] { self[key] as? [SolmuJSON] ?? [] }
    func number(_ key: String) -> Double { (self[key] as? NSNumber)?.doubleValue ?? 0 }
}

enum SolmuAPIError: LocalizedError {
    case invalidResponse
    case request(String)
    case disconnected

    var errorDescription: String? {
        switch self {
        case .invalidResponse: "The backend returned an invalid response."
        case .request(let message): message
        case .disconnected: "The connection to Solmu was interrupted."
        }
    }
}

final class SolmuURLProtocol: URLProtocol {
    static var handler: ((URLRequest) -> (Int, String))?

    override class func canInit(with request: URLRequest) -> Bool { handler != nil && request.url?.scheme != "ws" && request.url?.scheme != "wss" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let (status, text) = Self.handler?(request) ?? (500, "{}")
        let data = Data(text.utf8)
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": request.value(forHTTPHeaderField: "Accept") == "text/event-stream" ? "text/event-stream" : "application/json"])!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

@MainActor
final class SolmuStore: ObservableObject {
    @Published var threads: [SolmuJSON] = []
    @Published var current: SolmuJSON?
    @Published var messages: [SolmuJSON] = []
    @Published var tools: [SolmuJSON] = []
    @Published var catalog: SolmuJSON = [:]
    @Published var profile: SolmuJSON = [:]
    @Published var profilePrompt = ""
    @Published var profileModel = ""
    @Published var audit: [SolmuJSON] = []
    @Published var auditCursor: String?
    @Published var auditCache: SolmuJSON = [:]
    @Published var tasks: [SolmuJSON] = []
    @Published var webhooks: [SolmuJSON] = []
    @Published var skills: SolmuJSON = [:]
    @Published var mcp: SolmuJSON = [:]
    @Published var plugins: SolmuJSON = [:]
    @Published var partial = ""
    @Published var error = ""
    @Published var notice = ""
    @Published var connected = false
    @Published var busy = false
    @Published var responding = false

    let baseURL: String
    private let session: URLSession
    private var eventTask: Task<Void, Never>?
    private var replyTask: Task<Void, Never>?
    private var auditLoading = false
    private var testMode: Bool

    init(baseURL: String, testMode: Bool = ProcessInfo.processInfo.arguments.contains("--uitesting")) {
        self.baseURL = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        self.testMode = testMode
        let configuration = URLSessionConfiguration.default
        if testMode { configuration.protocolClasses = [SolmuURLProtocol.self] }
        self.session = URLSession(configuration: configuration)
        if testMode { Self.installUITestFixture() }
    }

    deinit {
        eventTask?.cancel()
        replyTask?.cancel()
    }

    private func url(_ path: String) throws -> URL {
        guard let base = URL(string: baseURL), var components = URLComponents(url: base, resolvingAgainstBaseURL: false) else { throw SolmuAPIError.request("Enter a valid backend address.") }
        components.path = "/api/v1" + (path.hasPrefix("/") ? path : "/" + path)
        guard let result = components.url else { throw SolmuAPIError.request("The backend address is invalid.") }
        return result
    }

    func request(_ method: String = "GET", _ path: String, body: SolmuJSON? = nil) async throws -> SolmuJSON {
        var request = URLRequest(url: try url(path))
        request.httpMethod = method
        request.timeoutInterval = 30
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body, options: [.fragmentsAllowed, .sortedKeys])
        }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw SolmuAPIError.invalidResponse }
        let decoded = try? JSONSerialization.jsonObject(with: data)
        let value: SolmuJSON
        if let object = decoded as? SolmuJSON { value = object }
        else if let items = decoded as? [SolmuJSON] { value = ["items": items] }
        else { value = [:] }
        guard (200..<300).contains(http.statusCode) else {
            let error = value["error"] as? SolmuJSON
            throw SolmuAPIError.request(error?.string("message", "Request failed (\(http.statusCode))") ?? "Request failed (\(http.statusCode))")
        }
        return value
    }

    func list(_ path: String) async throws -> [SolmuJSON] {
        let value = try await request("GET", path)
        return (value["items"] as? [SolmuJSON]) ?? (value["servers"] as? [SolmuJSON]) ?? []
    }

    func start() async {
        do {
            try await loadThreads(createOnStart: true)
            try await loadProfile()
            try await loadCatalog()
        } catch { self.error = error.localizedDescription }
        if !testMode { connectEvents() }
    }

    func stop() {
        eventTask?.cancel()
        replyTask?.cancel()
        eventTask = nil
    }

    func loadThreads(createOnStart: Bool = false) async throws {
        let items = try await list("/threads?limit=100")
        threads = items
        if current == nil && createOnStart {
            let reusable = items.first { $0.string("title") == "New conversation" }
            let next: SolmuJSON
            if let reusable { next = reusable }
            else { next = try await request("POST", "/threads"); threads.insert(next, at: 0) }
            try await loadThread(next.string("id"))
        } else if let id = current?.string("id"), let latest = items.first(where: { $0.string("id") == id }) {
            current = latest
        } else if let id = current?.string("id"), !items.contains(where: { $0.string("id") == id }) {
            current = nil
            messages = []
            tools = []
        }
    }

    func loadThread(_ id: String, preserveDraft: Bool = false) async throws {
        async let thread = request("GET", "/threads/\(id)")
        async let history = list("/threads/\(id)/messages?limit=100")
        async let calls = list("/threads/\(id)/tools?limit=100")
        let (threadValue, historyValue, callsValue) = try await (thread, history, calls)
        current = threadValue
        messages = historyValue
        tools = callsValue
        if !preserveDraft { partial = "" }
    }

    func openThread(_ thread: SolmuJSON) async { await perform { try await self.loadThread(thread.string("id")) } }

    func newThread() async {
        guard current == nil || !messages.isEmpty else { return }
        await perform {
            let thread = try await self.request("POST", "/threads")
            self.current = thread
            self.messages = []
            self.tools = []
            self.threads.insert(thread, at: 0)
        }
    }

    func send(_ raw: String) {
        let content = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !content.isEmpty, let id = current?.string("id"), !responding, !busy else { return }
        replyTask?.cancel()
        busy = true
        responding = true
        partial = ""
        error = ""
        replyTask = Task {
            do {
                let user = try await request("POST", "/threads/\(id)/messages", body: ["content": content])
                messages.append(user)
                try await streamReply(threadID: id, messageID: user.string("id"))
                try await loadThreads()
            } catch is CancellationError {
            } catch { self.error = error.localizedDescription; partial = "" }
            responding = false
            busy = false
            replyTask = nil
        }
    }

    func stopReply() {
        guard let id = current?.string("id") else { return }
        Task { _ = try? await request("POST", "/threads/\(id)/stop") }
        replyTask?.cancel()
        partial = ""
        responding = false
        busy = false
    }

    func compact() async {
        guard let id = current?.string("id") else { return }
        await perform { _ = try await self.request("POST", "/threads/\(id)/compact"); try await self.loadThread(id) }
    }

    func rename(_ title: String) async {
        guard let id = current?.string("id") else { return }
        await perform { self.current = try await self.request("PATCH", "/threads/\(id)", body: ["title": title]); try await self.loadThreads() }
    }

    func setThreadModel(_ value: String?) async {
        guard let id = current?.string("id") else { return }
        await perform { self.current = try await self.request("PATCH", "/threads/\(id)", body: ["model": value as Any? ?? NSNull()]); try await self.loadThreads() }
    }

    func deleteThread() async {
        guard let id = current?.string("id") else { return }
        await perform { _ = try await self.request("DELETE", "/threads/\(id)"); self.current = nil; self.messages = []; self.tools = []; try await self.loadThreads(createOnStart: true) }
    }

    func loadProfile(preserveDraft: Bool = false) async throws {
        let next = try await request("GET", "/profile")
        let oldPrompt = profile.string("system_prompt")
        if !preserveDraft || profilePrompt == oldPrompt || profilePrompt.isEmpty { profilePrompt = next.string("system_prompt") }
        if !preserveDraft || profileModel == (profile.optionalString("model") ?? "") { profileModel = next.optionalString("model") ?? "" }
        profile = next
    }

    func saveProfile() async {
        await perform { [self] in
            self.profile = try await self.request("PUT", "/profile", body: ["system_prompt": self.profilePrompt, "model": self.profileModel.isEmpty ? NSNull() : self.profileModel as Any])
            try await self.loadProfile()
            self.notice = "Profile saved"
        }
    }

    func loadCatalog() async throws { catalog = try await request("GET", "/models") }
    func loadSkills() async throws { guard let id = current?.string("id") else { return }; skills = try await request("GET", "/threads/\(id)/skills") }
    func loadMCP() async throws { guard let id = current?.string("id") else { return }; mcp = try await request("GET", "/threads/\(id)/mcp") }
    func loadPlugins() async throws { guard let id = current?.string("id") else { return }; plugins = try await request("GET", "/threads/\(id)/plugins") }
    func loadTasks() async throws { tasks = try await list("/tasks?limit=100") }
    func loadWebhooks() async throws { webhooks = try await list("/webhooks") }

    func loadAudit(reset: Bool = false) async throws {
        guard !auditLoading else { return }
        if !reset && auditCursor == nil && !audit.isEmpty { return }
        auditLoading = true
        defer { auditLoading = false }
        let before = reset ? nil : auditCursor
        let path = "/audit?limit=25" + (before.map { "&before=\($0)" } ?? "")
        let result = try await request("GET", path)
        let incoming = result.array("items")
        if reset { audit = incoming }
        else { audit.append(contentsOf: incoming.filter { item in !audit.contains(where: { $0.string("id") == item.string("id") }) }) }
        auditCursor = result["next_cursor"] as? String ?? (result["next_cursor"] as? NSNumber)?.stringValue
        auditCache = result["cache_24h"] as? SolmuJSON ?? [:]
    }

    func refreshLiveData() async {
        do {
            try await loadThreads()
            try await loadProfile(preserveDraft: true)
            try await loadCatalog()
            if let id = current?.string("id") { try await loadThread(id, preserveDraft: true) }
        } catch { self.error = error.localizedDescription }
    }

    private func perform(_ operation: @escaping () async throws -> Void) async {
        guard !busy else { return }
        busy = true
        error = ""
        defer { busy = false }
        do { try await operation() }
        catch { self.error = error.localizedDescription }
    }

    func act(_ operation: @escaping () async throws -> Void) async { await perform(operation) }

    private func streamReply(threadID: String, messageID: String) async throws {
        var request = URLRequest(url: try url("/threads/\(threadID)/responses"))
        request.httpMethod = "POST"
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: ["message_id": messageID])
        if testMode {
            let (body, response) = try await session.data(for: request)
            guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else { throw SolmuAPIError.request("Solmu could not start the reply.") }
            try consumeEvents(String(data: body, encoding: .utf8) ?? "")
            return
        }
        let (bytes, response) = try await session.bytes(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else { throw SolmuAPIError.request("Solmu could not start the reply.") }
        var event = "message"
        var data = ""
        for try await line in bytes.lines {
            try Task.checkCancellation()
            if line.isEmpty {
                if !data.isEmpty {
                    try consumeEvent(event, data: data)
                }
                event = "message"
                data = ""
            } else if line.hasPrefix("event:") { event = String(line.dropFirst(6)).trimmingCharacters(in: .whitespaces) }
            else if line.hasPrefix("data:") { data += String(line.dropFirst(5)).trimmingCharacters(in: .whitespaces) }
        }
    }

    private func consumeEvents(_ text: String) throws {
        var event = "message"
        var data = ""
        for line in text.components(separatedBy: .newlines) {
            if line.isEmpty {
                if !data.isEmpty { try consumeEvent(event, data: data) }
                event = "message"
                data = ""
            } else if line.hasPrefix("event:") { event = String(line.dropFirst(6)).trimmingCharacters(in: .whitespaces) }
            else if line.hasPrefix("data:") { data += String(line.dropFirst(5)).trimmingCharacters(in: .whitespaces) }
        }
    }

    private func consumeEvent(_ event: String, data: String) throws {
        let value = (try? JSONSerialization.jsonObject(with: Data(data.utf8))) as? SolmuJSON ?? [:]
        switch event {
        case "delta": partial += value.string("text")
        case "reset": partial = ""
        case "done": partial = ""; messages.append(value)
        case "tool_start", "tool_result":
            if let index = tools.firstIndex(where: { $0.string("id") == value.string("id") }) { tools[index] = value }
            else { tools.append(value) }
        case "error": throw SolmuAPIError.request((value["error"] as? SolmuJSON)?.string("message", "The reply failed") ?? "The reply failed")
        default: break
        }
    }

    private func connectEvents() {
        eventTask?.cancel()
        eventTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                do {
                    var components = URLComponents(string: self.baseURL)!
                    components.scheme = components.scheme == "https" ? "wss" : "ws"
                    components.path = "/api/v1/events"
                    guard let url = components.url else { throw SolmuAPIError.invalidResponse }
                    let socket = self.session.webSocketTask(with: url)
                    socket.resume()
                    self.connected = true
                    await self.refreshLiveData()
                    while !Task.isCancelled {
                        let message = try await socket.receive()
                        guard case .string(let text) = message,
                              let data = text.data(using: .utf8),
                              let value = (try? JSONSerialization.jsonObject(with: data)) as? SolmuJSON else { continue }
                        await self.handleEvent(value.string("type"))
                    }
                } catch {
                    self.connected = false
                    if !Task.isCancelled { try? await Task.sleep(for: .seconds(2)) }
                }
            }
        }
    }

    private func handleEvent(_ type: String) async {
        do {
            switch type {
            case "conversation_changed", "resync":
                try await loadThreads()
                if let id = current?.string("id") { try await loadThread(id, preserveDraft: true) }
            case "profile_changed": try await loadProfile(preserveDraft: true)
            case "tasks_changed": try await loadTasks()
            case "webhooks_changed": try await loadWebhooks()
            case "skills_changed": try await loadSkills()
            case "mcp_changed": try await loadMCP()
            case "plugins_changed": try await loadPlugins()
            default: break
            }
        } catch { self.error = error.localizedDescription }
    }

    private static func installUITestFixture() {
        let screenshotMode = ProcessInfo.processInfo.arguments.contains("--screenshot")
        var thread: SolmuJSON = ["id": "thread-ios", "title": screenshotMode ? "A thoughtful next step" : "New conversation", "model": NSNull(), "workspace": "/workspace"]
        var messages: [SolmuJSON] = screenshotMode ? [["id": "sample-ios", "thread_id": "thread-ios", "role": "assistant", "content": "Hello. I can help you make a plan, explore an idea, or work through a task in your workspace."]] : []
        var profilePrompt = "You are Solmu, an autonomous agent."
        var task: SolmuJSON?
        var webhook: SolmuJSON?
        SolmuURLProtocol.handler = { request in
            let path = request.url?.path.replacingOccurrences(of: "/api/v1", with: "") ?? ""
            let body = request.httpBody.flatMap { try? JSONSerialization.jsonObject(with: $0) as? SolmuJSON } ?? [:]
            func json(_ value: SolmuJSON) -> (Int, String) { (200, String(data: (try? JSONSerialization.data(withJSONObject: value)) ?? Data("{}".utf8), encoding: .utf8) ?? "{}") }
            switch (request.httpMethod ?? "GET", path) {
            case ("GET", "/threads"): return json(["items": [thread]])
            case ("POST", "/threads"):
                thread = ["id": "thread-ios-\(UUID().uuidString.prefix(5))", "title": "New conversation", "model": NSNull(), "workspace": "/workspace"]
                messages = []
                return json(thread)
            case ("GET", "/threads/thread-ios"): return json(thread)
            case ("GET", let path) where path.hasPrefix("/threads/") && !path.hasSuffix("/messages") && !path.hasSuffix("/tools"):
                return json(thread)
            case ("PATCH", let path) where path.hasPrefix("/threads/"):
                if let title = body["title"] as? String { thread["title"] = title }
                if body.keys.contains("model") { thread["model"] = body["model"] ?? NSNull() }
                return json(thread)
            case ("GET", let path) where path.hasPrefix("/threads/") && path.hasSuffix("/messages"): return json(["items": messages])
            case ("POST", let path) where path.hasPrefix("/threads/") && path.hasSuffix("/messages"):
                let user: SolmuJSON = ["id": "user-ios-\(messages.count)", "thread_id": thread.string("id"), "role": "user", "content": body.string("content")]
                messages.append(user)
                return json(user)
            case ("POST", let path) where path.hasPrefix("/threads/") && path.hasSuffix("/responses"):
                let reply: SolmuJSON = ["id": "assistant-ios-\(messages.count)", "thread_id": thread.string("id"), "role": "assistant", "content": "Hello from Solmu iOS"]
                messages.append(reply)
                return (200, "event: delta\ndata: {\"text\":\"Hello from Solmu iOS\"}\n\nevent: done\ndata: \(String(data: (try? JSONSerialization.data(withJSONObject: reply)) ?? Data("{}".utf8), encoding: .utf8) ?? "{}")\n\n")
            case ("GET", let path) where path.hasPrefix("/threads/") && path.hasSuffix("/tools"): return json(["items": []])
            case ("POST", let path) where path.hasPrefix("/threads/") && path.hasSuffix("/compact"): return json([:])
            case ("POST", let path) where path.hasPrefix("/threads/") && path.hasSuffix("/stop"): return json([:])
            case ("DELETE", let path) where path.hasPrefix("/threads/"): return json([:])
            case ("GET", "/profile"): return json(["system_prompt": profilePrompt, "model": NSNull(), "backend_default_model": "gpt-6-sol", "edited_at": "2026-10-03T00:00:00Z"])
            case ("PUT", "/profile"): profilePrompt = body.string("system_prompt"); return json(["system_prompt": profilePrompt, "model": body["model"] ?? NSNull(), "backend_default_model": "gpt-6-sol", "edited_at": "2026-10-03T00:00:00Z"])
            case ("GET", "/models"): return json(["default_model": "gpt-6-sol", "models": [["id": "gpt-6-sol", "name": "GPT 6 Sol"]]])
            case ("GET", "/audit"): return json(["items": [["id": "audit-ios", "name": "Read", "status": "completed", "thread_title": thread.string("title"), "created_at": "2026-10-03T00:00:00Z", "arguments": ["path": "README.md"], "result": "# Solmu"]], "next_cursor": NSNull(), "cache_24h": ["input_tokens": 100, "output_tokens": 20, "cached_input_tokens": 40, "cache_creation_input_tokens": 5, "hit_rate_percent": 40.0]])
            case ("GET", "/tasks"): return json(["items": task.map { [$0] } ?? []])
            case ("POST", "/tasks"):
                task = ["id": "task-ios", "name": body.string("name"), "prompt": body.string("prompt"), "schedule_kind": body.string("schedule_kind"), "schedule": body.string("schedule"), "enabled": true, "running": false]
                return json(task!)
            case ("PATCH", let path) where path.hasPrefix("/tasks/"):
                task?.merge(body) { _, new in new }
                return json(task ?? [:])
            case ("DELETE", let path) where path.hasPrefix("/tasks/"): task = nil; return json([:])
            case ("GET", let path) where path.hasSuffix("/runs"): return json(["items": []])
            case ("POST", let path) where path.hasSuffix("/run"): return json([:])
            case ("GET", "/webhooks"): return json(["items": webhook.map { [$0] } ?? []])
            case ("POST", "/webhooks"):
                webhook = ["id": "hook-ios", "name": body.string("name"), "enabled": true, "auth_type": body.string("auth_type"), "secret_configured": true, "instructions": body.string("instructions"), "port": 3001]
                return json(webhook!)
            case ("PATCH", let path) where path.hasPrefix("/webhooks/"):
                webhook?.merge(body) { _, new in new }
                return json(webhook ?? [:])
            case ("DELETE", let path) where path.hasPrefix("/webhooks/"): webhook = nil; return json([:])
            case (_, let path) where path.hasSuffix("/skills"): return json(["directory": ".agents/skills", "items": [], "issues": []])
            case (_, let path) where path.hasSuffix("/mcp"): return json(["workspace": "/workspace", "servers": [], "issues": []])
            case (_, let path) where path.hasSuffix("/plugins"): return json(["directory": ".agents/plugins", "items": [], "issues": []])
            default: return (200, "{}")
            }
        }
    }
}
