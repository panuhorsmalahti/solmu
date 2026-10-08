import SwiftUI
import UIKit

private enum SolmuTab: String, CaseIterable {
    case chat = "Chat", profile = "Profile", audit = "Audit", tasks = "Tasks", more = "More"
    var icon: String {
        switch self { case .chat: "bubble.left.and.bubble.right"; case .profile: "person.crop.circle"; case .audit: "list.clipboard"; case .tasks: "calendar"; case .more: "ellipsis" }
    }
}

struct SolmuHomeView: View {
    @StateObject private var store: SolmuStore
    let onDisconnect: () -> Void
    @State private var tab: SolmuTab = .chat

    init(baseURL: String, onDisconnect: @escaping () -> Void) {
        _store = StateObject(wrappedValue: SolmuStore(baseURL: baseURL))
        self.onDisconnect = onDisconnect
    }

    var body: some View {
        TabView(selection: $tab) {
            NavigationStack { ConversationView(store: store, onDisconnect: onDisconnect) }
                .solmuNotices(from: store)
                .tabItem { Label("Chat", systemImage: SolmuTab.chat.icon) }.tag(SolmuTab.chat)
            NavigationStack { ProfileView(store: store) }
                .solmuNotices(from: store)
                .tabItem { Label("Profile", systemImage: SolmuTab.profile.icon) }.tag(SolmuTab.profile)
            NavigationStack { AuditView(store: store) }
                .solmuNotices(from: store)
                .tabItem { Label("Audit", systemImage: SolmuTab.audit.icon) }.tag(SolmuTab.audit)
            NavigationStack { TasksView(store: store) }
                .solmuNotices(from: store)
                .tabItem { Label("Tasks", systemImage: SolmuTab.tasks.icon) }.tag(SolmuTab.tasks)
            NavigationStack { MoreView(store: store) }
                .solmuNotices(from: store)
                .tabItem { Label("More", systemImage: SolmuTab.more.icon) }.tag(SolmuTab.more)
        }
        .tint(SolmuPalette.green)
        .task { await store.start() }
        .onChange(of: tab) { _, selected in
            Task {
                do {
                    switch selected {
                    case .profile: try await store.loadProfile(preserveDraft: true); try await store.loadCatalog()
                    case .audit: try await store.loadAudit(reset: true)
                    case .tasks: try await store.loadTasks()
                    default: break
                    }
                } catch { store.error = error.localizedDescription }
            }
        }
    }
}

private extension View {
    func solmuNotices(from store: SolmuStore) -> some View {
        alert("Solmu", isPresented: Binding(
            get: { !store.error.isEmpty || !store.notice.isEmpty },
            set: { if !$0 { store.error = ""; store.notice = "" } }
        )) {
            Button("OK", role: .cancel) { store.error = ""; store.notice = "" }
        } message: {
            Text(store.error.isEmpty ? store.notice : store.error)
        }
    }
}

struct ConversationView: View {
    @ObservedObject var store: SolmuStore
    let onDisconnect: () -> Void
    @FocusState private var draftFocused: Bool
    @State private var draft = ""
    @State private var showThreads = false
    @State private var showRename = false
    @State private var showModel = false
    @State private var showInfo = ""
    @State private var showDelete = false
    @State private var titleDraft = ""

    var body: some View {
        VStack(spacing: 0) {
            if let thread = store.current {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 12) {
                            if store.messages.isEmpty && store.partial.isEmpty { welcome }
                            ForEach(store.messages, id: \.stableID) { message in
                                MessageBubble(message: message)
                                ForEach(store.tools.filter { $0.string("message_id") == message.string("id") }, id: \.stableID) { ToolCallCard(tool: $0) }
                            }
                            if !store.partial.isEmpty { MessageBubble(message: ["role": "assistant", "content": store.partial], streaming: true).id("stream") }
                        }
                        .padding(16)
                    }
                    .onChange(of: store.messages.count) { _, _ in withAnimation { proxy.scrollTo(store.messages.last?.string("id"), anchor: .bottom) } }
                    .onChange(of: store.partial) { _, value in if !value.isEmpty { proxy.scrollTo("stream", anchor: .bottom) } }
                }
                composer
            } else {
                ContentUnavailableView("Connecting to Solmu", systemImage: "network", description: Text(store.baseURL))
            }
        }
        .background(SolmuPalette.paper)
        .navigationTitle("solmu")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Button { showThreads = true } label: {
                    HStack(spacing: 5) { Text(store.current?.string("title", "Conversations") ?? "Conversations").lineLimit(1); Image(systemName: "chevron.down").font(.caption2) }
                        .font(.subheadline.weight(.semibold)).foregroundStyle(SolmuPalette.ink)
                }
                .accessibilityLabel(store.current?.string("title", "Conversations") ?? "Conversations")
                .accessibilityIdentifier("thread-selector")
            }
            ToolbarItem(placement: .topBarLeading) {
                Button { Task { await store.newThread() } } label: { Image(systemName: "square.and.pencil") }
                    .accessibilityLabel("New conversation").accessibilityIdentifier("new-thread")
                    .disabled(store.busy || (store.current != nil && store.messages.isEmpty))
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button("Rename", systemImage: "pencil") { titleDraft = store.current?.string("title") ?? ""; showRename = true }
                    Button("Model", systemImage: "cpu") { showModel = true }
                    Button("Status", systemImage: "bolt.horizontal") { showInfo = "Status" }
                    Button("Context", systemImage: "square.stack.3d.up") { showInfo = "Context" }
                    Button("Copy latest reply", systemImage: "doc.on.doc") { copyLatest() }
                    if let thread = store.current { ShareLink(item: exportText(thread), subject: Text("Solmu — \(thread.string("title"))")) { Label("Export conversation", systemImage: "square.and.arrow.up") } }
                    Button("Compact history", systemImage: "arrow.trianglehead.2.clockwise.rotate.90") { Task { await store.compact() } }
                        .disabled(store.messages.count < 2 || store.busy)
                    Button("Delete conversation", systemImage: "trash", role: .destructive) { showDelete = true }
                    Divider()
                    Button("Change backend", systemImage: "server.rack", action: onDisconnect)
                } label: { Image(systemName: "ellipsis.circle") }
                .accessibilityLabel("Conversation actions")
            }
            if store.current != nil {
                ToolbarItem(placement: .bottomBar) {
                    HStack(spacing: 12) {
                        Button { showModel = true } label: {
                            Text(store.current?.optionalString("model") ?? "\(store.catalog.string("default_model", "Backend default")) (default)")
                                .lineLimit(1).font(.caption).foregroundStyle(SolmuPalette.green)
                        }
                        Button("Compact") { Task { await store.compact() } }.disabled(store.messages.count < 2 || store.busy)
                        Menu {
                            Button("Skills") { Task { await store.act { try await store.loadSkills() }; showInfo = "Skills" } }
                            Button("MCP") { Task { await store.act { try await store.loadMCP() }; showInfo = "MCP" } }
                            Button("Plugins") { Task { await store.act { try await store.loadPlugins() }; showInfo = "Plugins" } }
                        } label: { Image(systemName: "square.grid.2x2") }
                    }
                }
            }
        }
        .sheet(isPresented: $showThreads) { ThreadPicker(store: store, dismiss: { showThreads = false }) }
        .sheet(isPresented: $showRename) {
            NavigationStack {
                Form { TextField("Conversation name", text: $titleDraft).accessibilityIdentifier("rename-title") }
                    .navigationTitle("Rename conversation")
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) { Button("Cancel") { showRename = false } }
                        ToolbarItem(placement: .confirmationAction) { Button("Save") { showRename = false; Task { await store.rename(titleDraft.trimmingCharacters(in: .whitespacesAndNewlines)) } }.disabled(titleDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) }
                    }
            }.presentationDetents([.medium])
        }
        .sheet(isPresented: $showModel) { ThreadModelPicker(store: store) }
        .sheet(isPresented: Binding(get: { !showInfo.isEmpty }, set: { if !$0 { showInfo = "" } })) {
            ContextSheet(store: store, kind: showInfo).presentationDetents([.medium, .large])
        }
        .confirmationDialog("Delete this conversation?", isPresented: $showDelete, titleVisibility: .visible) {
            Button("Delete conversation", role: .destructive) { Task { await store.deleteThread() } }
        }
        .task(id: store.current?.string("id")) { draft = "" }
    }

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("ROOM FOR POSSIBILITY").font(.caption.weight(.medium)).tracking(1.4).foregroundStyle(SolmuPalette.green)
            Text("A little space for your next big idea.").font(.system(size: 34, weight: .regular, design: .serif)).foregroundStyle(SolmuPalette.ink)
            Text("Ask a question. Untangle a thought. Follow an idea somewhere new.").foregroundStyle(SolmuPalette.muted)
            if !store.connected { Text("Waiting for the backend at \(store.baseURL)").font(.caption).foregroundStyle(SolmuPalette.muted) }
        }
        .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 38).accessibilityIdentifier("chat-welcome")
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Where shall we begin?", text: $draft, axis: .vertical)
                .lineLimit(1...5).padding(11).background(SolmuPalette.paper, in: RoundedRectangle(cornerRadius: 16))
                .overlay(RoundedRectangle(cornerRadius: 16).stroke(SolmuPalette.border))
                .focused($draftFocused)
                .accessibilityIdentifier("message-input")
                .disabled(store.busy)
            if store.responding {
                Button(action: store.stopReply) { Image(systemName: "stop.fill").frame(width: 44, height: 44) }
                    .buttonStyle(.borderedProminent).tint(.red).accessibilityLabel("Stop response").accessibilityIdentifier("stop-response")
            } else {
                Button {
                    let message = draft
                    draft = ""
                    draftFocused = false
                    store.send(message)
                } label: { Image(systemName: "arrow.up").fontWeight(.bold).frame(width: 44, height: 44) }
                    .buttonStyle(.borderedProminent).accessibilityLabel("Send message").accessibilityIdentifier("send-message")
                    .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.busy)
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 9).background(.white).overlay(alignment: .top) { Rectangle().fill(SolmuPalette.border).frame(height: 1) }
    }

    private func copyLatest() {
        guard let reply = store.messages.last(where: { $0.string("role") == "assistant" })?.string("content") else { store.notice = "No Solmu reply to copy yet"; return }
        UIPasteboard.general.string = reply
        store.notice = "Copied latest reply"
    }

    private func exportText(_ thread: SolmuJSON) -> String {
        var output = "# \(thread.string("title"))\n\n"
        for message in store.messages {
            output += "## \(message.string("role") == "user" ? "You" : "Solmu")\n\n\(message.string("content"))\n\n"
            for tool in store.tools.filter({ $0.string("message_id") == message.string("id") }) {
                output += "### \(tool.string("name")) · \(tool.string("status"))\n\nArguments: \(jsonText(tool["arguments"]))\nResult: \(jsonText(tool["result"]))\n\n"
            }
        }
        return output
    }
}

struct ThreadPicker: View {
    @ObservedObject var store: SolmuStore
    let dismiss: () -> Void
    var body: some View {
        NavigationStack {
            List(store.threads, id: \.stableID) { thread in
                Button {
                    dismiss()
                    Task { await store.openThread(thread) }
                } label: {
                    HStack { Text(thread.string("title")).foregroundStyle(SolmuPalette.ink); Spacer(); if thread.string("id") == store.current?.string("id") { Image(systemName: "checkmark").foregroundStyle(SolmuPalette.green) } }
                }.accessibilityIdentifier("thread-\(thread.string("id"))")
            }
            .overlay { if store.threads.isEmpty { ContentUnavailableView("No conversations", systemImage: "bubble.left.and.bubble.right") } }
            .navigationTitle("Conversations")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done", action: dismiss) } }
        }.presentationDetents([.medium, .large])
    }
}

struct MessageBubble: View {
    let message: SolmuJSON
    var streaming = false
    private var isUser: Bool { message.string("role") == "user" }
    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(isUser ? "YOU" : streaming ? "SOLMU · STREAMING" : "SOLMU")
                .font(.caption2.weight(.semibold)).tracking(1).foregroundStyle(SolmuPalette.green)
            Text(message.string("content")).font(.body).foregroundStyle(SolmuPalette.ink).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 16).padding(.vertical, 13).frame(maxWidth: .infinity, alignment: .leading)
        .background(isUser ? SolmuPalette.soft : .white, in: RoundedRectangle(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).stroke(isUser ? .clear : SolmuPalette.border))
    }
}

struct ToolCallCard: View {
    let tool: SolmuJSON
    var body: some View {
        DisclosureGroup("\(tool.string("name")) · \(tool.string("status", "running"))") {
            VStack(alignment: .leading, spacing: 8) {
                Text("Arguments").font(.caption).foregroundStyle(SolmuPalette.muted)
                Text(jsonText(tool["arguments"])).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                if let result = tool["result"] { Text("Result").font(.caption).foregroundStyle(SolmuPalette.muted); Text(jsonText(result)).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }
            }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, 8)
        }
        .font(.subheadline.weight(.semibold)).tint(SolmuPalette.green).padding(13)
        .background(SolmuPalette.soft, in: RoundedRectangle(cornerRadius: 12))
    }
}

struct ThreadModelPicker: View {
    @ObservedObject var store: SolmuStore
    @Environment(\.dismiss) private var dismiss
    @State private var custom = ""
    var body: some View {
        NavigationStack {
            List {
                Section("Backend default: \(store.catalog.string("default_model", "not configured"))") {
                    Button("Use backend default") { dismiss(); Task { await store.setThreadModel(nil) } }
                    ForEach(store.catalog.array("models"), id: \.stableID) { option in
                        Button { dismiss(); Task { await store.setThreadModel(option.string("id")) } } label: {
                            VStack(alignment: .leading) { Text(option.string("name")); Text(option.string("id")).font(.caption).foregroundStyle(SolmuPalette.muted) }
                        }
                    }
                }
                Section("Custom model") {
                    TextField("Model ID", text: $custom).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Apply custom model") { dismiss(); Task { await store.setThreadModel(custom.trimmingCharacters(in: .whitespacesAndNewlines)) } }.disabled(custom.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }.navigationTitle("Model for this conversation").toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }.presentationDetents([.medium, .large])
    }
}

struct ContextSheet: View {
    @ObservedObject var store: SolmuStore
    let kind: String
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        NavigationStack {
            List {
                if let thread = store.current {
                    LabeledContent("Connection", value: store.connected ? "Connected" : "Disconnected")
                    LabeledContent("Conversation", value: thread.string("title"))
                    LabeledContent("Thread ID", value: thread.string("id"))
                    LabeledContent("Model", value: thread.optionalString("model") ?? store.catalog.string("default_model", "Backend default"))
                    LabeledContent("Workspace", value: thread.string("workspace", "No workspace"))
                    if kind == "Context" {
                        LabeledContent("Messages", value: "\(store.messages.count)")
                        LabeledContent("Tool calls", value: "\(store.tools.count)")
                        LabeledContent("Skills", value: "\(store.skills.array("items").count)")
                        LabeledContent("MCP servers", value: "\(store.mcp.array("servers").count)")
                        LabeledContent("Plugins", value: "\(store.plugins.array("items").count)")
                    }
                } else { Text("No conversation selected.") }
            }.navigationTitle(kind == "Status" ? "Conversation status" : "Current context")
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
        .task {
            guard kind == "Context" else { return }
            do { try await store.loadSkills(); try await store.loadMCP(); try await store.loadPlugins() }
            catch { store.error = error.localizedDescription }
        }
    }
}

struct ProfileView: View {
    @ObservedObject var store: SolmuStore
    @FocusState private var promptFocused: Bool
    var body: some View {
        Form {
            Section {
                Text("Set the defaults Solmu uses across your conversations.").foregroundStyle(SolmuPalette.muted)
            }
            Section("System prompt") {
                TextEditor(text: $store.profilePrompt).focused($promptFocused).frame(minHeight: 230).accessibilityIdentifier("profile-prompt")
                Text("Edited on \(store.profile.string("edited_at", "Loading…"))").font(.caption).foregroundStyle(SolmuPalette.muted)
            }
            Section("Default model") {
                Text("Backend default: \(store.profile.string("backend_default_model", "not configured"))").font(.caption).foregroundStyle(SolmuPalette.muted)
                Picker("Default model", selection: $store.profileModel) {
                    Text("Use backend default").tag("")
                ForEach(store.catalog.array("models"), id: \.stableID) { model in Text(model.string("name") + " · " + model.string("id")).tag(model.string("id")) }
                }
                TextField("Custom model ID", text: Binding(get: { store.catalog.array("models").contains(where: { $0.string("id") == store.profileModel }) ? "" : store.profileModel }, set: { store.profileModel = $0 })).textInputAutocapitalization(.never).autocorrectionDisabled()
            }
            Section { Button("Save profile") { Task { await store.saveProfile() } }.disabled(store.busy || store.profilePrompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty).accessibilityIdentifier("save-profile") }
        }
        .scrollContentBackground(.hidden).background(SolmuPalette.paper)
        .navigationTitle("Profile")
        .toolbar { ToolbarItemGroup(placement: .keyboard) { Spacer(); Button("Done") { promptFocused = false }.accessibilityIdentifier("dismiss-keyboard") } }
        .task { do { try await store.loadProfile(preserveDraft: true); try await store.loadCatalog() } catch { store.error = error.localizedDescription } }
    }
}

struct AuditView: View {
    @ObservedObject var store: SolmuStore
    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 7) {
                    Text("PROMPT CACHE · LAST 24 HOURS").font(.caption2.weight(.semibold)).tracking(1).foregroundStyle(SolmuPalette.green)
                    Text(store.auditCache["hit_rate_percent"] == nil ? "—" : String(format: "%.1f%%", store.auditCache.number("hit_rate_percent")))
                        .font(.system(size: 40, design: .serif)).foregroundStyle(SolmuPalette.green).accessibilityIdentifier("audit-cache-hit-rate")
                    Text("Cached input \(Int(store.auditCache.number("cached_input_tokens"))) · Input \(Int(store.auditCache.number("input_tokens")))")
                    Text("Output \(Int(store.auditCache.number("output_tokens"))) · Cache writes \(Int(store.auditCache.number("cache_creation_input_tokens")))")
                }.font(.caption).foregroundStyle(SolmuPalette.muted).padding(10)
            }
            Section("Tool calls · newest first") {
                if store.audit.isEmpty { ContentUnavailableView("No tool calls yet", systemImage: "list.clipboard") }
                ForEach(store.audit.indices, id: \.self) { index in
                    let entry = store.audit[index]
                    DisclosureGroup {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Arguments").font(.caption).foregroundStyle(SolmuPalette.muted)
                            Text(jsonText(entry["arguments"])).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                            Text("Result").font(.caption).foregroundStyle(SolmuPalette.muted)
                            Text(jsonText(entry["result"])).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                        }.padding(.vertical, 5)
                    } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("\(entry.string("name")) · \(entry.string("status"))").fontWeight(.semibold)
                            Text(entry.string("thread_title")).foregroundStyle(SolmuPalette.green)
                            Text(entry.string("created_at")).font(.caption).foregroundStyle(SolmuPalette.muted)
                        }
                    }
                    .onAppear { if index >= store.audit.count - 4 { Task { do { try await store.loadAudit() } catch { store.error = error.localizedDescription } } } }
                }
                if store.auditCursor == nil && !store.audit.isEmpty { Text("You’re up to date.").font(.caption).foregroundStyle(SolmuPalette.muted) }
            }
        }
        .scrollContentBackground(.hidden).background(SolmuPalette.paper)
        .navigationTitle("Audit")
        .task { do { try await store.loadAudit(reset: true) } catch { store.error = error.localizedDescription } }
    }
}

struct TasksView: View {
    @ObservedObject var store: SolmuStore
    @State private var editing: SolmuJSON?
    @State private var creating = false
    var body: some View {
        List {
            Section {
                Text("Schedule Solmu to run once later or on a recurring schedule.").font(.subheadline).foregroundStyle(SolmuPalette.muted)
                Button { editing = nil; creating = true } label: { Label("New scheduled task", systemImage: "plus.circle.fill") }.accessibilityIdentifier("new-task")
            }
            Section("Scheduled tasks") {
                if store.tasks.isEmpty { ContentUnavailableView("No tasks yet", systemImage: "calendar") }
                ForEach(store.tasks, id: \.stableID) { task in TaskRow(store: store, task: task, edit: { editing = task; creating = true }) }
            }
        }
        .scrollContentBackground(.hidden).background(SolmuPalette.paper).navigationTitle("Tasks")
        .sheet(isPresented: $creating) { TaskEditor(store: store, task: editing).presentationDetents([.large]) }
        .task { do { try await store.loadTasks() } catch { store.error = error.localizedDescription } }
    }
}

private struct TaskRow: View {
    @ObservedObject var store: SolmuStore
    let task: SolmuJSON
    let edit: () -> Void
    @State private var runs: [SolmuJSON]?
    var body: some View {
        let scheduleStatus = task.bool("enabled") ? "Scheduled" : "Paused"
        let runningStatus = task.bool("running") ? " · Running" : ""
        VStack(alignment: .leading, spacing: 8) {
            Text(task.string("name")).font(.headline).accessibilityIdentifier("task-\(task.string("id"))")
            Text(task.string("prompt")).font(.subheadline).foregroundStyle(SolmuPalette.muted)
            Text("\(task.string("schedule_kind")): \(task.string("schedule")) · \(scheduleStatus)\(runningStatus)").font(.caption).foregroundStyle(SolmuPalette.muted)
            HStack {
                Button("Run now") { Task { await store.act { _ = try await store.request("POST", "/tasks/\(task.string("id"))/run"); try await store.loadTasks() } } }.disabled(task.bool("running"))
                Button(task.bool("enabled") ? "Pause" : "Resume") { Task { await store.act { _ = try await store.request("PATCH", "/tasks/\(task.string("id"))", body: ["enabled": !task.bool("enabled")]); try await store.loadTasks() } } }.disabled(task.bool("running"))
                Button("Edit", action: edit).disabled(task.bool("running"))
                Button(role: .destructive) { Task { await store.act { _ = try await store.request("DELETE", "/tasks/\(task.string("id"))"); try await store.loadTasks() } } } label: { Image(systemName: "trash") }.disabled(task.bool("running"))
            }.buttonStyle(.borderless).font(.caption)
            DisclosureGroup("Run history") {
                if let runs {
                    if runs.isEmpty { Text("No runs yet.").foregroundStyle(SolmuPalette.muted) }
                    ForEach(Array(runs.enumerated()), id: \.offset) { _, run in Text("\(run.string("status")) · \(run.string("started_at")) \(run.string("error"))").font(.caption) }
                } else {
                    Button("Load run history") { Task { do { runs = try await store.list("/tasks/\(task.string("id"))/runs") } catch { store.error = error.localizedDescription } } }
                }
            }.font(.caption)
        }.padding(.vertical, 5)
    }
}

private struct TaskEditor: View {
    @ObservedObject var store: SolmuStore
    let task: SolmuJSON?
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var prompt: String
    @State private var kind: String
    @State private var schedule: String
    @FocusState private var formFocused: Bool

    init(store: SolmuStore, task: SolmuJSON?) {
        self.store = store
        self.task = task
        _name = State(initialValue: task?.string("name") ?? "")
        _prompt = State(initialValue: task?.string("prompt") ?? "")
        _kind = State(initialValue: task?.string("schedule_kind") ?? "once")
        _schedule = State(initialValue: task?.string("schedule") ?? ISO8601DateFormatter().string(from: Date().addingTimeInterval(3600)))
    }

    var body: some View {
        NavigationStack {
            Form {
                TextField("Name", text: $name).focused($formFocused).accessibilityIdentifier("task-name")
                TextField("What should Solmu do?", text: $prompt, axis: .vertical).focused($formFocused).lineLimit(3...7).accessibilityIdentifier("task-prompt")
                Picker("Schedule", selection: $kind) { Text("One time").tag("once"); Text("Cron").tag("cron") }
                TextField(kind == "once" ? "Future RFC 3339 time" : "Cron expression (UTC)", text: $schedule).focused($formFocused).textInputAutocapitalization(.never).autocorrectionDisabled().accessibilityIdentifier("task-schedule")
                if kind == "cron" { Text("Example: 0 9 * * *").font(.caption).foregroundStyle(SolmuPalette.muted) }
                Button(task == nil ? "Create task" : "Save task") {
                    let body: SolmuJSON = ["name": name.trimmingCharacters(in: .whitespacesAndNewlines), "prompt": prompt.trimmingCharacters(in: .whitespacesAndNewlines), "schedule_kind": kind, "schedule": schedule.trimmingCharacters(in: .whitespacesAndNewlines)]
                    Task { await store.act { _ = try await store.request(task == nil ? "POST" : "PATCH", task == nil ? "/tasks" : "/tasks/\(task!.string("id"))", body: body); try await store.loadTasks() }; if store.error.isEmpty { dismiss() } }
                }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || schedule.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.busy).accessibilityIdentifier("save-task")
            }.navigationTitle(task == nil ? "New task" : "Edit task")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                    ToolbarItemGroup(placement: .keyboard) { Spacer(); Button("Done") { formFocused = false }.accessibilityIdentifier("dismiss-keyboard") }
                }
        }
    }
}

struct MoreView: View {
    @ObservedObject var store: SolmuStore
    var body: some View {
        List {
            Section { Text("Workspace status and agent settings.").foregroundStyle(SolmuPalette.muted) }
            Section("Workspace") {
                NavigationLink { MemoriesView(store: store) } label: { Label("Memories", systemImage: "brain.head.profile") }
                NavigationLink { WorkspaceCatalogView(store: store, kind: "Skills") } label: { Label("Skills", systemImage: "sparkles") }
                NavigationLink { WorkspaceCatalogView(store: store, kind: "MCP") } label: { Label("MCP", systemImage: "point.3.connected.trianglepath.dotted") }
                NavigationLink { WorkspaceCatalogView(store: store, kind: "Plugins") } label: { Label("Plugins", systemImage: "puzzlepiece.extension") }
                NavigationLink { WebhooksView(store: store) } label: { Label("Webhooks", systemImage: "arrow.trianglehead.2.clockwise") }
            }
        }.scrollContentBackground(.hidden).background(SolmuPalette.paper).navigationTitle("More")
    }
}

struct MemoriesView: View {
    @ObservedObject var store: SolmuStore
    var body: some View {
        List {
            Section { Text("Saved facts that Solmu can recall across conversations when they match what you ask.").foregroundStyle(SolmuPalette.muted) }
            Section("Saved memories · newest first") {
                if store.memories.isEmpty { ContentUnavailableView("No saved memories yet", systemImage: "brain.head.profile") }
                ForEach(store.memories.indices, id: \.self) { index in
                    let memory = store.memories[index]
                    VStack(alignment: .leading, spacing: 8) {
                        Text(memory.string("content")).textSelection(.enabled)
                        Text("Saved \(memory.string("created_at"))").font(.caption).foregroundStyle(SolmuPalette.muted)
                    }
                    .padding(.vertical, 5)
                    .onAppear { if index >= store.memories.count - 4 && store.memoryHasMore { Task { do { try await store.loadMemories() } catch { store.error = error.localizedDescription } } } }
                }
                if !store.memoryHasMore && !store.memories.isEmpty { Text("You’re up to date.").font(.caption).foregroundStyle(SolmuPalette.muted) }
            }
        }
        .scrollContentBackground(.hidden).background(SolmuPalette.paper)
        .navigationTitle("Memories")
        .task { do { try await store.loadMemories(reset: true) } catch { store.error = error.localizedDescription } }
    }
}

struct WorkspaceCatalogView: View {
    @ObservedObject var store: SolmuStore
    let kind: String
    private var data: SolmuJSON { kind == "Skills" ? store.skills : kind == "MCP" ? store.mcp : store.plugins }
    private var entries: [SolmuJSON] { data.array(kind == "MCP" ? "servers" : "items") }
    var body: some View {
        List {
            Section { Text("Automatically discovered from this conversation’s workspace.").foregroundStyle(SolmuPalette.muted) }
            Section(kind) {
                if entries.isEmpty { ContentUnavailableView(kind == "MCP" ? "No MCP servers configured in this workspace." : "No \(kind.lowercased()) installed in this workspace.", systemImage: kind == "Skills" ? "sparkles" : "square.grid.2x2") }
                ForEach(Array(entries.enumerated()), id: \.offset) { _, item in
                    VStack(alignment: .leading, spacing: 5) {
                        Text(item.string("name")).font(.headline)
                        if kind == "MCP" { Text("\(item.string("status")) · \(item.string("transport"))").font(.caption).foregroundStyle(SolmuPalette.green) }
                        if let description = item.optionalString("description") { Text(description).font(.subheadline).foregroundStyle(SolmuPalette.muted) }
                        if let path = item.optionalString("path") { Text(path).font(.caption.monospaced()).foregroundStyle(SolmuPalette.muted) }
                        ForEach(item.array("tools"), id: \.toolName) { tool in Text("\(tool.string("name")) — \(tool.string("description"))").font(.caption) }
                        ForEach(item.array("issues"), id: \.issueMessage) { issue in Text(issue.string("message")).font(.caption).foregroundStyle(.red) }
                        if let error = item.optionalString("error") { Text(error).font(.caption).foregroundStyle(.red) }
                    }.padding(.vertical, 4)
                }
                ForEach(data.array("issues"), id: \.issuePath) { issue in Text("\(issue.string("path")): \(issue.string("message"))").font(.caption).foregroundStyle(.red) }
            }
        }.scrollContentBackground(.hidden).background(SolmuPalette.paper).navigationTitle(kind)
            .task {
                do { if kind == "Skills" { try await store.loadSkills() } else if kind == "MCP" { try await store.loadMCP() } else { try await store.loadPlugins() } }
                catch { store.error = error.localizedDescription }
            }
    }
}

struct WebhooksView: View {
    @ObservedObject var store: SolmuStore
    @State private var create = false
    var body: some View {
        List {
            Section { Text("Let GitHub and other services start an agent conversation.").foregroundStyle(SolmuPalette.muted); Button { create = true } label: { Label("Add webhook", systemImage: "plus.circle.fill") }.accessibilityIdentifier("new-webhook") }
            Section("Configured webhooks") {
                if store.webhooks.isEmpty { ContentUnavailableView("No webhooks configured", systemImage: "arrow.trianglehead.2.clockwise") }
                ForEach(store.webhooks, id: \.stableID) { hook in WebhookRow(store: store, hook: hook) }
            }
        }.scrollContentBackground(.hidden).background(SolmuPalette.paper).navigationTitle("Webhooks")
            .sheet(isPresented: $create) { WebhookEditor(store: store, hook: nil).presentationDetents([.large]) }
            .task { do { try await store.loadWebhooks() } catch { store.error = error.localizedDescription } }
    }
}

private struct WebhookRow: View {
    @ObservedObject var store: SolmuStore
    let hook: SolmuJSON
    @State private var editing = false
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(hook.string("name")).font(.headline).accessibilityIdentifier("webhook-\(hook.string("id"))")
            Toggle("Enabled", isOn: Binding(get: { hook.bool("enabled") }, set: { value in Task { await store.act { _ = try await store.request("PATCH", "/webhooks/\(hook.string("id"))", body: ["enabled": value]); try await store.loadWebhooks() } } }))
            Text("\(hook.string("auth_type")) · Port \(hook.string("port"))").font(.caption).foregroundStyle(SolmuPalette.muted)
            Text("Webhook URL").font(.caption).foregroundStyle(SolmuPalette.muted)
            Text(webhookURL(base: store.baseURL, hook: hook)).font(.caption.monospaced()).textSelection(.enabled)
            HStack { Button("Edit") { editing = true }; Spacer(); Button("Delete", role: .destructive) { Task { await store.act { _ = try await store.request("DELETE", "/webhooks/\(hook.string("id"))"); try await store.loadWebhooks() } } } }.buttonStyle(.borderless)
        }.padding(.vertical, 5).sheet(isPresented: $editing) { WebhookEditor(store: store, hook: hook).presentationDetents([.large]) }
    }
}

private struct WebhookEditor: View {
    private enum Field: Hashable { case name, secret, instructions }

    @ObservedObject var store: SolmuStore
    let hook: SolmuJSON?
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var secret = ""
    @State private var instructions: String
    @State private var authType: String
    @FocusState private var focusedField: Field?

    init(store: SolmuStore, hook: SolmuJSON?) {
        self.store = store; self.hook = hook
        _name = State(initialValue: hook?.string("name") ?? "")
        _instructions = State(initialValue: hook?.string("instructions") ?? "")
        _authType = State(initialValue: hook?.string("auth_type") ?? "github-hmac-sha256")
    }

    var body: some View {
        NavigationStack {
            Form {
                TextField("Name", text: $name).focused($focusedField, equals: .name).accessibilityIdentifier("webhook-name")
                if hook == nil || hook?.bool("secret_configured") == false { SecureField("Secret (16 characters minimum)", text: $secret).focused($focusedField, equals: .secret) }
                else { SecureField("Replace secret (optional)", text: $secret).focused($focusedField, equals: .secret); Text("Secret is stored. Leave blank to keep it.").font(.caption).foregroundStyle(SolmuPalette.muted) }
                Picker("Authentication", selection: $authType) { Text("GitHub HMAC").tag("github-hmac-sha256"); Text("Bearer").tag("bearer") }
                TextField("Instructions for Solmu", text: $instructions, axis: .vertical).focused($focusedField, equals: .instructions).lineLimit(2...6)
                Button(hook == nil ? "Create webhook" : "Save changes") {
                    var body: SolmuJSON = ["name": name.trimmingCharacters(in: .whitespacesAndNewlines), "instructions": instructions, "auth_type": authType]
                    if hook == nil || !secret.isEmpty { body["secret"] = secret }
                    Task { await store.act { _ = try await store.request(hook == nil ? "POST" : "PATCH", hook == nil ? "/webhooks" : "/webhooks/\(hook!.string("id"))", body: body); try await store.loadWebhooks() }; if store.error.isEmpty { dismiss() } }
                }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || (hook == nil && secret.count < 16) || (!secret.isEmpty && secret.count < 16) || store.busy).accessibilityIdentifier("save-webhook")
            }.navigationTitle(hook == nil ? "Add webhook" : "Edit webhook")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                    ToolbarItemGroup(placement: .keyboard) { Spacer(); Button("Done") { focusedField = nil }.accessibilityIdentifier("dismiss-keyboard") }
                }
        }
    }
}

private func webhookURL(base: String, hook: SolmuJSON) -> String {
    guard let components = URLComponents(string: base), let host = components.host else { return "\(base)/hooks/\(hook.string("id"))" }
    var endpoint = URLComponents(); endpoint.scheme = components.scheme; endpoint.host = host; endpoint.port = Int(hook.string("port")); endpoint.path = "/hooks/\(hook.string("id"))"
    return endpoint.string ?? "\(base)/hooks/\(hook.string("id"))"
}

private func jsonText(_ value: Any?) -> String {
    guard let value, !(value is NSNull) else { return "No result" }
    if JSONSerialization.isValidJSONObject(value), let data = try? JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys]), let text = String(data: data, encoding: .utf8) { return text }
    return String(describing: value)
}
