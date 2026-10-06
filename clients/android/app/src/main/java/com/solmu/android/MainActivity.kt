package com.solmu.android

import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.ClipboardManager
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.collect
import org.json.JSONArray
import org.json.JSONObject
import java.net.URI
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

private val SolmuGreen = Color(0xFF276551)
private val SolmuInk = Color(0xFF263C36)
private val SolmuMuted = Color(0xFF75847A)
private val SolmuPaper = Color(0xFFFAFBF8)
private val SolmuSidebar = Color(0xFFF0F3EC)
private val SolmuBorder = Color(0xFFDFE6DD)
private val SolmuSoft = Color(0xFFEDF1EA)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            SolmuTheme {
                val context = LocalContext.current
                var server by remember { mutableStateOf(savedBackend(context)) }
                if (server.isBlank()) {
                    ConnectScreen(onConnect = { value ->
                        saveBackend(context, value)
                        server = value
                    })
                } else {
                    SolmuApplication(server = server, onChangeServer = {
                        saveBackend(context, "")
                        server = ""
                    })
                }
            }
        }
    }

    private fun savedBackend(context: Context) =
        context.getSharedPreferences("solmu", Context.MODE_PRIVATE).getString("backend", "").orEmpty()

    private fun saveBackend(context: Context, value: String) {
        context.getSharedPreferences("solmu", Context.MODE_PRIVATE).edit().putString("backend", value).apply()
    }
}

@Composable
private fun SolmuTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = lightColorScheme(
            primary = SolmuGreen,
            onPrimary = Color.White,
            background = SolmuPaper,
            onBackground = SolmuInk,
            surface = Color.White,
            onSurface = SolmuInk,
            surfaceVariant = SolmuSoft,
            outline = SolmuBorder,
            error = Color(0xFFA53535),
        ),
        content = content,
    )
}

@Composable
private fun ConnectScreen(onConnect: (String) -> Unit) {
    var address by remember { mutableStateOf("http://10.0.2.2:3000") }
    var problem by remember { mutableStateOf("") }
    Column(
        modifier = Modifier.fillMaxSize().background(SolmuPaper).verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("solmu", fontSize = 34.sp, fontWeight = FontWeight.Bold, letterSpacing = (-1.5).sp)
        Text("YOUR IDEAS, CONNECTED", color = SolmuMuted, fontSize = 11.sp, letterSpacing = 1.4.sp)
        Spacer(Modifier.height(30.dp))
        Surface(shape = RoundedCornerShape(18.dp), color = Color.White, border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
            Column(Modifier.padding(22.dp)) {
                Text("Connect to your backend", fontSize = 25.sp, fontFamily = FontFamily.Serif)
                Spacer(Modifier.height(10.dp))
                Text("Enter the address of the computer running Solmu. Conversations, profile, and agent tools stay on that backend.", color = SolmuMuted, lineHeight = 22.sp)
                Spacer(Modifier.height(24.dp))
                Text("BACKEND ADDRESS", color = SolmuMuted, fontSize = 11.sp, letterSpacing = 1.sp)
                Spacer(Modifier.height(7.dp))
                OutlinedTextField(
                    value = address,
                    onValueChange = { address = it; problem = "" },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    placeholder = { Text("http://10.0.2.2:3000") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, capitalization = KeyboardCapitalization.None),
                )
                if (problem.isNotBlank()) Text(problem, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(top = 8.dp), fontSize = 13.sp)
                Spacer(Modifier.height(16.dp))
                Button(
                    onClick = {
                        val normalized = normalizeAddress(address)
                        if (normalized == null) problem = "Enter a valid http:// or https:// backend address."
                        else onConnect(normalized)
                    },
                    modifier = Modifier.fillMaxWidth().height(50.dp),
                    shape = RoundedCornerShape(10.dp),
                ) { Text("Connect") }
                Spacer(Modifier.height(17.dp))
                Text("Android emulator: http://10.0.2.2:3000\nPhone: use your computer’s network address, for example http://192.168.1.20:3000.", color = SolmuMuted, fontSize = 12.sp, lineHeight = 19.sp)
            }
        }
    }
}

private fun normalizeAddress(raw: String): String? = runCatching {
    val uri = URI(raw.trim().trimEnd('/'))
    if (uri.scheme !in listOf("http", "https") || uri.host.isNullOrBlank() || uri.userInfo != null || uri.query != null || uri.fragment != null) null
    else uri.toString().trimEnd('/')
}.getOrNull()

private data class ThreadItem(val id: String, val title: String, val model: String?, val workspace: String?) {
    companion object {
        fun from(json: JSONObject) = ThreadItem(json.string("id"), json.string("title"), json.nullableString("model"), json.nullableString("workspace"))
    }
}

private data class ChatMessage(val id: String, val threadId: String, val role: String, val content: String) {
    companion object {
        fun from(json: JSONObject) = ChatMessage(json.string("id"), json.string("thread_id"), json.string("role"), json.string("content"))
    }
}

private class SolmuModel(
    val api: SolmuApi,
    private val scope: CoroutineScope,
) {
    var page by mutableStateOf("Chat")
    var threads by mutableStateOf<List<ThreadItem>>(emptyList())
    var current by mutableStateOf<ThreadItem?>(null)
    var titleDraft by mutableStateOf("")
    var messages by mutableStateOf<List<ChatMessage>>(emptyList())
    var tools by mutableStateOf<List<JSONObject>>(emptyList())
    var draft by mutableStateOf("")
    var partial by mutableStateOf("")
    var error by mutableStateOf("")
    var busy by mutableStateOf(false)
    var responding by mutableStateOf(false)
    var connected by mutableStateOf(false)
    var profile by mutableStateOf<JSONObject?>(null)
    var profilePrompt by mutableStateOf("")
    var profileModel by mutableStateOf("")
    var catalog by mutableStateOf<JSONObject?>(null)
    var skills by mutableStateOf<JSONObject?>(null)
    var mcp by mutableStateOf<JSONObject?>(null)
    var plugins by mutableStateOf<JSONObject?>(null)
    var audit by mutableStateOf<List<JSONObject>>(emptyList())
    var auditCursor by mutableStateOf<Long?>(null)
    var auditCache by mutableStateOf<JSONObject?>(null)
    var auditLoading by mutableStateOf(false)
    var tasks by mutableStateOf<List<JSONObject>>(emptyList())
    var webhooks by mutableStateOf<List<JSONObject>>(emptyList())
    var notice by mutableStateOf("")
    private var replyJob: Job? = null
    private var stopSocket: (() -> Unit)? = null
    private val pagesLoading = mutableSetOf<String>()

    fun start() {
        scope.launch {
            runCatching { loadThreads(createOnStart = true); loadProfile(); loadCatalog() }
                .onFailure { error = it.message ?: "Cannot connect to the Solmu backend" }
        }
        stopSocket = api.observe(
            onEvent = { type -> scope.launch { handleChange(type) } },
            onConnection = { value ->
                connected = value
                if (value) scope.launch {
                    refreshLiveData()
                }
            },
        )
    }

    fun dispose() {
        stopSocket?.invoke()
        replyJob?.cancel()
    }

    private suspend fun refreshLiveData() {
        runCatching {
            loadThreads()
            loadProfile(preserveDraft = true)
            loadCatalog()
            if (current != null) loadThread(current!!.id, preserveDraft = true)
            if (page == "Audit") loadAudit(reset = true)
            if (page == "Tasks") loadTasks()
            if (page == "Webhooks") loadWebhooks()
            if (page == "Skills") loadSkills()
            if (page == "MCP") loadMcp()
            if (page == "Plugins") loadPlugins()
        }.onFailure { error = it.message ?: "Live update failed" }
    }

    private suspend fun handleChange(type: String) {
        when (type) {
            "conversation_changed", "resync" -> {
                loadThreads()
                current?.let { loadThread(it.id, preserveDraft = true) }
                if (page == "Audit") loadAudit(reset = true)
            }
            "profile_changed" -> loadProfile(preserveDraft = true)
            "tasks_changed" -> if (page == "Tasks") loadTasks()
            "webhooks_changed" -> if (page == "Webhooks") loadWebhooks()
            "skills_changed" -> if (page == "Skills") loadSkills()
            "mcp_changed" -> if (page == "MCP") loadMcp()
            "plugins_changed" -> if (page == "Plugins") loadPlugins()
        }
    }

    fun launch(block: suspend () -> Unit) {
        if (busy) return
        scope.launch {
            busy = true
            error = ""
            try { block() }
            catch (cause: CancellationException) { throw cause }
            catch (cause: Throwable) { error = cause.message ?: "The request failed" }
            finally { busy = false }
        }
    }

    suspend fun loadThreads(createOnStart: Boolean = false) {
        val items = api.list("/threads?limit=100").map(ThreadItem::from)
        threads = items
        if (current == null && createOnStart) {
            val reusable = items.firstOrNull { it.title == "New conversation" && runCatching { api.list("/threads/${it.id}/messages").isEmpty() }.getOrDefault(false) }
            val next = reusable ?: ThreadItem.from(api.post("/threads"))
            if (reusable == null) threads = listOf(next) + items
            loadThread(next.id)
        } else if (current != null) {
            val latest = items.firstOrNull { it.id == current!!.id }
            if (latest != null) {
                if (titleDraft == current!!.title || titleDraft.isBlank()) titleDraft = latest.title
                current = latest
            } else {
                current = null; messages = emptyList(); tools = emptyList()
            }
        }
    }

    suspend fun loadThread(id: String, preserveDraft: Boolean = false) {
        val thread = ThreadItem.from(api.get("/threads/$id"))
        val history = api.list("/threads/$id/messages?limit=100").map(ChatMessage::from)
        val activity = api.list("/threads/$id/tools?limit=100")
        current = thread
        if (!preserveDraft || titleDraft == current?.title || titleDraft.isBlank()) titleDraft = thread.title
        messages = history
        tools = activity
    }

    fun openThread(thread: ThreadItem) = launch {
        loadThread(thread.id)
        page = "Chat"
        error = ""
    }

    fun newThread() = launch {
        if (current != null && messages.isEmpty()) return@launch
        val next = ThreadItem.from(api.post("/threads"))
        current = next; titleDraft = next.title; messages = emptyList(); tools = emptyList(); draft = ""
        threads = listOf(next) + threads
        page = "Chat"
    }

    fun send() {
        val thread = current ?: return
        val content = draft.trim()
        if (content.isEmpty() || responding || busy) return
        if (content == "/goal" || content.startsWith("/goal ")) {
            val objective = content.removePrefix("/goal").trim()
            draft = ""
            scope.launch {
                busy = true; error = ""
                try {
                    if (objective.isEmpty()) {
                        val goals = api.list("/goals")
                        notice = if (goals.isEmpty()) "No goals yet. Start one with /goal <objective>." else goals.joinToString("\n") { "[${it.string("status")}] ${it.string("objective")} (${it.string("id")})" }
                    } else {
                        val goal = api.post("/goals", json { put("objective", objective); put("thread_id", thread.id) })
                        notice = "Goal started: ${goal.string("objective")} (${goal.string("id")})"
                    }
                } catch (failure: Exception) { error = failure.message ?: "Cannot access goals" }
                busy = false
            }
            return
        }
        val operation = scope.launch {
            busy = true; responding = true; error = ""; partial = ""
            try {
                val userMessage = ChatMessage.from(api.post("/threads/${thread.id}/messages", json { put("content", content) }))
                draft = ""
                messages = messages + userMessage
                replyJob = scope.launch {
                    try {
                        api.streamReply(thread.id, userMessage.id) { event, payload ->
                            when (event) {
                                "delta" -> partial += payload.string("text")
                                "reset" -> partial = ""
                                "done" -> {
                                    partial = ""
                                    messages = messages + ChatMessage.from(payload)
                                }
                                "tool_start", "tool_result" -> {
                                    val run = payload
                                    val old = tools.indexOfFirst { it.string("id") == run.string("id") }
                                    tools = if (old < 0) tools + run else tools.toMutableList().also { it[old] = run }
                                }
                            }
                        }
                        loadThreads()
                        current?.let { loadThread(it.id, preserveDraft = true) }
                    } catch (cause: CancellationException) { throw cause }
                    catch (cause: Throwable) { error = cause.message ?: "The reply failed"; partial = "" }
                    finally { responding = false; replyJob = null; busy = false }
                }
                replyJob?.join()
            } catch (cause: CancellationException) { throw cause }
            catch (cause: Throwable) { error = cause.message ?: "The message could not be sent"; responding = false; busy = false }
        }
        replyJob = operation
    }

    fun stop() {
        val thread = current ?: return
        scope.launch { runCatching { api.post("/threads/${thread.id}/stop") } }
        replyJob?.cancel()
        partial = ""
        responding = false
        busy = false
    }

    suspend fun loadProfile(preserveDraft: Boolean = false) {
        val next = api.get("/profile")
        val oldPrompt = profile?.string("system_prompt").orEmpty()
        if (!preserveDraft || profilePrompt == oldPrompt || profilePrompt.isBlank()) profilePrompt = next.string("system_prompt")
        if (!preserveDraft || profileModel == profile?.nullableString("model").orEmpty()) profileModel = next.nullableString("model").orEmpty()
        profile = next
    }

    suspend fun loadCatalog() { catalog = api.get("/models") }
    suspend fun loadSkills() { current?.let { skills = api.get("/threads/${it.id}/skills") } }
    suspend fun loadMcp() { current?.let { mcp = api.get("/threads/${it.id}/mcp") } }
    suspend fun loadPlugins() { current?.let { plugins = api.get("/threads/${it.id}/plugins") } }
    suspend fun loadTasks() { tasks = api.list("/tasks?limit=100") }
    suspend fun loadWebhooks() { webhooks = api.list("/webhooks") }

    suspend fun loadAudit(reset: Boolean = false) {
        if (auditLoading || (!reset && auditCursor == null && audit.isNotEmpty())) return
        auditLoading = true
        try {
            val before = if (reset) null else auditCursor
            val path = "/audit?limit=25" + (before?.let { "&before=$it" } ?: "")
            val result = api.get(path)
            val incoming = result.array("items")
            audit = if (reset) incoming else audit + incoming.filter { item -> audit.none { it.string("id") == item.string("id") } }
            auditCursor = if (result.isNull("next_cursor")) null else result.optLong("next_cursor")
            auditCache = result.optJSONObject("cache_24h")
        } finally { auditLoading = false }
    }
}

private val bottomDestinations = listOf("Chat", "Profile", "Audit", "Tasks", "More")

@Composable
private fun SolmuApplication(server: String, onChangeServer: () -> Unit) {
    val scope = rememberCoroutineScope()
    val api = remember(server) { SolmuApi(server) }
    val model = remember(api) { SolmuModel(api, scope) }
    DisposableEffect(model) {
        model.start()
        onDispose { model.dispose() }
    }
    BackHandler(enabled = model.page != "Chat") { model.page = "Chat" }

    Scaffold(
        containerColor = SolmuPaper,
        bottomBar = {
            NavigationBar(containerColor = Color.White) {
                bottomDestinations.forEach { destination ->
                    NavigationBarItem(
                        selected = model.page == destination || (model.page in listOf("Webhooks", "Skills", "MCP", "Plugins") && destination == "More"),
                        onClick = { if (destination == "Chat" && model.page != "Chat") model.page = "Chat" else model.page = destination },
                        modifier = Modifier.testTag("nav-$destination"),
                        icon = { Text(when (destination) { "Chat" -> "◌"; "Profile" -> "○"; "Audit" -> "≡"; "Tasks" -> "◷"; else -> "···" }, fontSize = 20.sp) },
                        label = { Text(destination) },
                        alwaysShowLabel = true,
                    )
                }
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (model.page == "Chat") ChatHeader(model, onChangeServer)
            if (model.error.isNotBlank()) ErrorBanner(model.error) { model.error = "" }
            if (model.notice.isNotBlank()) NoticeBanner(model.notice) { model.notice = "" }
            when (model.page) {
                "Chat" -> ConversationScreen(model)
                "Profile" -> ProfileScreen(model)
                "Audit" -> AuditScreen(model)
                "Tasks" -> TasksScreen(model)
                "More" -> MoreScreen(model)
                "Webhooks" -> WebhooksScreen(model)
                "Skills" -> CatalogScreen("Workspace skills", model.skills, "No skills installed in this workspace.", "items")
                "MCP" -> McpScreen(model)
                "Plugins" -> CatalogScreen("Agent Plugins", model.plugins, "No plugins installed in this workspace.", "items")
                else -> MoreScreen(model)
            }
        }
    }
}

@Composable
private fun ChatHeader(model: SolmuModel, onChangeServer: () -> Unit) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var chooseThread by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    var editTitle by remember { mutableStateOf(false) }
    var titleValue by remember { mutableStateOf("") }
    var chooseModel by remember { mutableStateOf(false) }
    var showInfo by remember { mutableStateOf("") }
    Column(Modifier.background(SolmuSidebar).padding(horizontal = 14.dp, vertical = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("solmu", modifier = Modifier.weight(1f), color = SolmuInk, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Box {
                TextButton(onClick = { chooseThread = true }, enabled = !model.busy) {
                    Text(model.current?.title ?: "Select conversation", maxLines = 1)
                }
                DropdownMenu(expanded = chooseThread, onDismissRequest = { chooseThread = false }) {
                    model.threads.forEach { thread -> DropdownMenuItem(text = { Text(thread.title) }, onClick = { chooseThread = false; model.openThread(thread) }) }
                }
            }
            IconButton(onClick = model::newThread, enabled = !model.busy) { Icon(Icons.Default.Add, "New conversation", tint = SolmuGreen) }
            Box {
                IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, "Conversation actions") }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(text = { Text("Rename") }, onClick = { menu = false; titleValue = model.current?.title.orEmpty(); editTitle = true }, enabled = model.current != null && !model.busy)
                    DropdownMenuItem(text = { Text("Model") }, onClick = { menu = false; chooseModel = true }, enabled = model.current != null && !model.busy)
                    DropdownMenuItem(text = { Text("Skills") }, onClick = { menu = false; model.page = "Skills"; model.launch { model.loadSkills() } }, enabled = model.current != null)
                    DropdownMenuItem(text = { Text("MCP") }, onClick = { menu = false; model.page = "MCP"; model.launch { model.loadMcp() } }, enabled = model.current != null)
                    DropdownMenuItem(text = { Text("Plugins") }, onClick = { menu = false; model.page = "Plugins"; model.launch { model.loadPlugins() } }, enabled = model.current != null)
                    DropdownMenuItem(text = { Text("Status") }, onClick = { menu = false; showInfo = "Status" })
                    DropdownMenuItem(text = { Text("Context") }, onClick = { menu = false; showInfo = "Context" })
                    DropdownMenuItem(text = { Text("Copy latest reply") }, onClick = { menu = false; model.messages.lastOrNull { it.role == "assistant" }?.let { clipboard.setText(AnnotatedString(it.content)); model.notice = "Copied latest reply" } ?: run { model.notice = "No Solmu reply to copy yet" } }, enabled = model.messages.any { it.role == "assistant" })
                    DropdownMenuItem(text = { Text("Export conversation") }, onClick = { menu = false; exportConversation(context, model) }, enabled = model.current != null)
                    DropdownMenuItem(text = { Text("Compact history") }, onClick = { menu = false; model.launch { model.api.post("/threads/${model.current!!.id}/compact"); model.loadThread(model.current!!.id) } }, enabled = model.current != null && model.messages.size >= 2 && !model.busy)
                    DropdownMenuItem(text = { Text("Delete conversation", color = MaterialTheme.colorScheme.error) }, onClick = { menu = false; model.launch { model.api.delete("/threads/${model.current!!.id}"); model.current = null; model.messages = emptyList(); model.tools = emptyList(); model.loadThreads(createOnStart = true) } }, enabled = model.current != null && !model.busy)
                    DropdownMenuItem(text = { Text("Change backend") }, onClick = { menu = false; onChangeServer() })
                }
            }
        }
        if (model.current != null) {
            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
                AssistChip(onClick = { chooseModel = true }, label = { Text(model.current?.model ?: model.catalog?.string("default_model")?.let { "$it (default)" } ?: "Default model") }, enabled = !model.busy)
                Spacer(Modifier.width(6.dp))
                TextButton(onClick = { model.launch { model.api.post("/threads/${model.current!!.id}/compact"); model.loadThread(model.current!!.id) } }, enabled = model.messages.size >= 2 && !model.busy) { Text("Compact") }
                TextButton(onClick = { showInfo = "Status" }) { Text(if (model.connected) "Connected" else "Offline") }
                TextButton(onClick = { showInfo = "Context" }) { Text("Context") }
                TextButton(onClick = { model.page = "Skills"; model.launch { model.loadSkills() } }) { Text("Skills") }
                TextButton(onClick = { model.page = "MCP"; model.launch { model.loadMcp() } }) { Text("MCP") }
                TextButton(onClick = { model.page = "Plugins"; model.launch { model.loadPlugins() } }) { Text("Plugins") }
            }
        }
    }
    if (editTitle) AlertDialog(
        onDismissRequest = { editTitle = false }, title = { Text("Rename conversation") },
        text = { OutlinedTextField(titleValue, { titleValue = it }, singleLine = true, label = { Text("Conversation name") }) },
        confirmButton = { TextButton(onClick = { editTitle = false; model.launch { val updated = model.api.patch("/threads/${model.current!!.id}", json { put("title", titleValue.trim()) }); model.current = ThreadItem.from(updated); model.titleDraft = titleValue.trim(); model.loadThreads() } }) { Text("Save") } },
        dismissButton = { TextButton(onClick = { editTitle = false }) { Text("Cancel") } },
    )
    if (chooseModel) ModelDialog(model) { chooseModel = false }
    if (showInfo.isNotBlank()) InfoDialog(showInfo, model) { showInfo = "" }
}

@Composable
private fun ModelDialog(model: SolmuModel, close: () -> Unit) {
    val catalog = model.catalog
    val options = catalog?.array("models").orEmpty()
    var custom by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = close,
        title = { Text("Model for this conversation") },
        text = {
            Column(Modifier.heightIn(max = 480.dp).verticalScroll(rememberScrollState())) {
                val default = catalog?.string("default_model").orEmpty()
                Text("Backend default: ${default.ifBlank { "not configured" }}", color = SolmuMuted, fontSize = 13.sp)
                TextButton(onClick = { close(); model.launch { model.current = ThreadItem.from(model.api.patch("/threads/${model.current!!.id}", json { put("model", JSONObject.NULL) })); model.loadThreads() } }) { Text("Use backend default") }
                options.forEach { option ->
                    TextButton(onClick = { close(); model.launch { model.current = ThreadItem.from(model.api.patch("/threads/${model.current!!.id}", json { put("model", option.string("id")) })); model.loadThreads() } }) {
                        Column { Text(option.string("name")); Text(option.string("id"), color = SolmuMuted, fontSize = 11.sp) }
                    }
                }
                OutlinedTextField(custom, { custom = it }, label = { Text("Custom model ID") }, singleLine = true)
                TextButton(enabled = custom.isNotBlank(), onClick = { close(); model.launch { model.current = ThreadItem.from(model.api.patch("/threads/${model.current!!.id}", json { put("model", custom.trim()) })); model.loadThreads() } }) { Text("Apply custom model") }
            }
        },
        confirmButton = { TextButton(onClick = close) { Text("Done") } },
    )
}

@Composable
private fun InfoDialog(kind: String, model: SolmuModel, close: () -> Unit) {
    var details by remember(kind, model.current?.id) { mutableStateOf("Loading…") }
    LaunchedEffect(kind, model.current?.id) {
        val thread = model.current
        if (thread == null) details = "No conversation selected."
        else if (kind == "Status") details = "${if (model.connected) "Connected" else "Disconnected"}\n${thread.title}\nThread ID: ${thread.id}\nModel: ${thread.model ?: model.catalog?.string("default_model", "Backend default")}\nWorkspace: ${thread.workspace ?: "No workspace"}"
        else runCatching {
            val skills = model.api.get("/threads/${thread.id}/skills").array("items").size
            val mcp = model.api.get("/threads/${thread.id}/mcp").array("servers").size
            val plugins = model.api.get("/threads/${thread.id}/plugins").array("items").size
            val toolCount = model.api.list("/threads/${thread.id}/tools").size
            details = "${model.messages.size} messages · $toolCount tool calls · $skills skills · $mcp MCP servers · $plugins plugins\nWorkspace: ${thread.workspace ?: "No workspace"}"
        }.onFailure { details = it.message ?: "Cannot load context" }
    }
    AlertDialog(onDismissRequest = close, title = { Text(if (kind == "Status") "Conversation status" else "Current context") }, text = { Text(details, lineHeight = 21.sp) }, confirmButton = { TextButton(onClick = close) { Text("Close") } })
}

@Composable
private fun ConversationScreen(model: SolmuModel) {
    val listState = rememberLazyListState()
    LaunchedEffect(model.messages.size, model.partial, model.tools.size) {
        val count = model.messages.size + if (model.partial.isNotBlank()) 1 else 0
        if (count > 0) listState.animateScrollToItem(count - 1)
    }
    Column(Modifier.fillMaxSize()) {
        LazyColumn(
            state = listState,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 18.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (model.messages.isEmpty() && model.partial.isBlank()) item {
                Column(Modifier.fillMaxWidth().padding(vertical = 34.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Room for possibility", color = SolmuGreen, fontSize = 11.sp, letterSpacing = 1.4.sp)
                    Text("A little space for your next big idea.", fontSize = 34.sp, fontFamily = FontFamily.Serif, lineHeight = 39.sp)
                    Text("Ask a question. Untangle a thought. Follow an idea somewhere new.", color = SolmuMuted, lineHeight = 23.sp)
                    if (!model.connected) Text("Waiting for the backend at ${model.api.baseUrl}", color = SolmuMuted, fontSize = 12.sp)
                }
            }
            items(model.messages, key = { it.id }) { message ->
                MessageCard(message)
                model.tools.filter { it.string("message_id") == message.id }.forEach { ToolCard(it) }
            }
            if (model.partial.isNotBlank()) item(key = "streaming") { MessageCard(ChatMessage("streaming", model.current?.id.orEmpty(), "assistant", model.partial), streaming = true) }
        }
        Surface(color = Color.White, shadowElevation = 5.dp) {
            Row(Modifier.fillMaxWidth().navigationBarsPadding().imePadding().padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.Bottom) {
                OutlinedTextField(
                    value = model.draft,
                    onValueChange = { model.draft = it },
                    modifier = Modifier.weight(1f).testTag("message-input"),
                    enabled = model.current != null && !model.busy,
                    placeholder = { Text(if (model.current == null) "Loading conversation…" else "Where shall we begin?") },
                    minLines = 1,
                    maxLines = 5,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    shape = RoundedCornerShape(16.dp),
                )
                Spacer(Modifier.width(8.dp))
                if (model.responding) {
                    FilledTonalIconButton(onClick = model::stop, modifier = Modifier.size(52.dp)) { Icon(Icons.Default.Stop, "Stop response") }
                } else {
                    FilledIconButton(onClick = model::send, enabled = model.draft.isNotBlank() && model.current != null && !model.busy, modifier = Modifier.size(52.dp).testTag("send-message")) { Icon(Icons.AutoMirrored.Filled.Send, "Send message") }
                }
            }
        }
    }
}

@Composable
private fun MessageCard(message: ChatMessage, streaming: Boolean = false) {
    val isUser = message.role == "user"
    Surface(
        modifier = Modifier.fillMaxWidth(),
        color = if (isUser) Color(0xFFF0F3EC) else Color.White,
        shape = RoundedCornerShape(16.dp),
        border = if (isUser) null else androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder),
    ) {
        Column(Modifier.padding(horizontal = 17.dp, vertical = 14.dp)) {
            Text(if (isUser) "YOU" else if (streaming) "SOLMU · STREAMING" else "SOLMU", color = SolmuGreen, fontSize = 10.sp, letterSpacing = 1.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(7.dp))
            Text(message.content, color = SolmuInk, lineHeight = 23.sp, fontSize = 15.sp)
        }
    }
}

@Composable
private fun ToolCard(tool: JSONObject) {
    var expanded by remember { mutableStateOf(false) }
    val status = tool.string("status", "running")
    Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = SolmuSoft)) {
        Column(Modifier.clickable { expanded = !expanded }.padding(14.dp)) {
            Text("${tool.string("name")} · $status", color = SolmuGreen, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            if (expanded) {
                Spacer(Modifier.height(8.dp))
                Text("Arguments", color = SolmuMuted, fontSize = 11.sp)
                Text(tool.optJSONObject("arguments")?.toString(2) ?: "{}", fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                if (!tool.isNull("result")) {
                    Spacer(Modifier.height(8.dp)); Text("Result", color = SolmuMuted, fontSize = 11.sp)
                    Text(tool.opt("result").toString(), fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                }
            }
        }
    }
}

@Composable
private fun ProfileScreen(model: SolmuModel) {
    LaunchedEffect(Unit) { if (model.profile == null) runCatching { model.loadProfile(); model.loadCatalog() }.onFailure { model.error = it.message.orEmpty() } }
    val profile = model.profile
    PageColumn(title = "Profile", subtitle = "Set the defaults Solmu uses across your conversations.") {
        OutlinedTextField(model.profilePrompt, { model.profilePrompt = it }, modifier = Modifier.fillMaxWidth().heightIn(min = 240.dp), label = { Text("System prompt") }, supportingText = { Text("Saved prompt: ${profile?.string("edited_at") ?: "Loading…"}") })
        Spacer(Modifier.height(18.dp))
        Text("Default model", fontWeight = FontWeight.SemiBold)
        Text("Backend default: ${profile?.string("backend_default_model", "not configured")}", color = SolmuMuted, fontSize = 13.sp)
        ModelSelection(model.catalog, model.profileModel) { model.profileModel = it }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Button(enabled = !model.busy && model.profilePrompt.isNotBlank(), onClick = { model.launch { model.profile = model.api.put("/profile", json { put("system_prompt", model.profilePrompt); put("model", if (model.profileModel.isBlank()) JSONObject.NULL else model.profileModel) }); model.loadProfile() } }) { Text("Save profile") }
            OutlinedButton(onClick = { model.launch { model.loadProfile(); model.loadCatalog() } }) { Text("Discard draft") }
        }
    }
}

private fun exportConversation(context: Context, model: SolmuModel) {
    val thread = model.current ?: return
    val content = buildString {
        appendLine("# ${thread.title}")
        appendLine()
        model.messages.forEach { message ->
            appendLine(if (message.role == "user") "## You" else "## Solmu")
            appendLine()
            appendLine(message.content)
            appendLine()
            model.tools.filter { it.string("message_id") == message.id }.forEach { tool ->
                appendLine("### ${tool.string("name")} · ${tool.string("status")}")
                appendLine()
                appendLine("Arguments: ${tool.optJSONObject("arguments") ?: "{}"}")
                appendLine("Result: ${tool.opt("result") ?: "No result"}")
                appendLine()
            }
        }
    }
    val intent = Intent(android.content.Intent.ACTION_SEND).apply {
        type = "text/markdown"
        putExtra(android.content.Intent.EXTRA_SUBJECT, "Solmu — ${thread.title}")
        putExtra(android.content.Intent.EXTRA_TEXT, content)
    }
    context.startActivity(Intent.createChooser(intent, "Export conversation"))
}

@Composable
private fun ModelSelection(catalog: JSONObject?, selected: String, onSelect: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }, modifier = Modifier.fillMaxWidth()) {
            Text(if (selected.isBlank()) "${catalog?.string("default_model", "Backend default") ?: "Backend default"} (default)" else selected)
        }
        DropdownMenu(expanded, { expanded = false }) {
            DropdownMenuItem(text = { Text("Use backend default${catalog?.nullableString("default_model")?.let { " ($it)" } ?: ""}") }, onClick = { onSelect(""); expanded = false })
            catalog?.array("models")?.forEach { entry ->
                DropdownMenuItem(text = { Column { Text(entry.string("name")); Text(entry.string("id"), color = SolmuMuted, fontSize = 11.sp) } }, onClick = { onSelect(entry.string("id")); expanded = false })
            }
        }
    }
}

@Composable
private fun AuditScreen(model: SolmuModel) {
    LaunchedEffect(Unit) { if (model.audit.isEmpty()) runCatching { model.loadAudit(reset = true) }.onFailure { model.error = it.message.orEmpty() } }
    val listState = rememberLazyListState()
    LaunchedEffect(listState, model.auditCursor, model.auditLoading) {
        snapshotFlow { listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0 }.collect { index ->
            if (!model.auditLoading && model.auditCursor != null && index >= model.audit.size - 4) runCatching { model.loadAudit() }.onFailure { model.error = it.message.orEmpty() }
        }
    }
    LazyColumn(Modifier.fillMaxSize(), state = listState, contentPadding = PaddingValues(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Text("THE WORK BEHIND THE WORK", color = SolmuGreen, fontSize = 10.sp, letterSpacing = 1.3.sp)
            Text("Audit", fontFamily = FontFamily.Serif, fontSize = 39.sp)
            Text("Saved tool calls across conversations, newest first.", color = SolmuMuted)
        }
        item { CacheCard(model.auditCache) }
        if (model.audit.isEmpty() && !model.auditLoading) item { Text("No tool calls yet.", color = SolmuMuted) }
        items(model.audit, key = { it.string("id") }) { AuditCard(it) }
        if (model.auditLoading) item { Text("Loading more tool calls…", color = SolmuMuted, modifier = Modifier.padding(12.dp)) }
        if (model.auditCursor == null && model.audit.isNotEmpty()) item { Text("You’re up to date.", color = SolmuMuted, modifier = Modifier.padding(12.dp)) }
    }
}

@Composable
private fun CacheCard(cache: JSONObject?) {
    val percent = if (cache == null || cache.isNull("hit_rate_percent")) "—" else "${cache.optDouble("hit_rate_percent").oneDecimal()}%"
    Card(colors = CardDefaults.cardColors(containerColor = SolmuSidebar)) {
        Column(Modifier.fillMaxWidth().padding(18.dp)) {
            Text("PROMPT CACHE · LAST 24 HOURS", color = SolmuGreen, fontSize = 10.sp, letterSpacing = 1.sp)
            Text(percent, fontSize = 40.sp, fontFamily = FontFamily.Serif, color = SolmuGreen)
            Text("Cached input ${cache?.optLong("cached_input_tokens") ?: 0} · Input ${cache?.optLong("input_tokens") ?: 0}", color = SolmuMuted, fontSize = 12.sp)
            Text("Output ${cache?.optLong("output_tokens") ?: 0} · Cache writes ${cache?.optLong("cache_creation_input_tokens") ?: 0}", color = SolmuMuted, fontSize = 12.sp)
        }
    }
}

@Composable
private fun AuditCard(entry: JSONObject) {
    var expanded by remember { mutableStateOf(false) }
    Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = Color.White), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
        Column(Modifier.clickable { expanded = !expanded }.padding(14.dp)) {
            Text("${entry.string("name")} · ${entry.string("status")}", fontWeight = FontWeight.SemiBold)
            Text(entry.string("thread_title"), color = SolmuGreen, fontSize = 12.sp)
            Text(entry.string("created_at"), color = SolmuMuted, fontSize = 11.sp)
            if (expanded) {
                HorizontalDivider(Modifier.padding(vertical = 10.dp))
                Text("Arguments", color = SolmuMuted, fontSize = 11.sp)
                Text(entry.opt("arguments")?.toString() ?: "{}", fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                Text("Result", color = SolmuMuted, fontSize = 11.sp, modifier = Modifier.padding(top = 8.dp))
                Text(entry.opt("result")?.toString() ?: "No result", fontFamily = FontFamily.Monospace, fontSize = 11.sp)
            }
        }
    }
}

@Composable
private fun TasksScreen(model: SolmuModel) {
    var name by remember { mutableStateOf("") }
    var prompt by remember { mutableStateOf("") }
    var kind by remember { mutableStateOf("once") }
    var schedule by remember { mutableStateOf("${Instant.now().plusSeconds(3600).toString()}") }
    var editing by remember { mutableStateOf("") }
    var runs by remember { mutableStateOf<Map<String, List<JSONObject>>>(emptyMap()) }
    var expandedRuns by remember { mutableStateOf("") }
    LaunchedEffect(Unit) { runCatching { model.loadTasks() }.onFailure { model.error = it.message.orEmpty() } }
    PageColumn(title = "Tasks", subtitle = "Schedule Solmu to run once later or on a recurring schedule.") {
        Surface(color = Color.White, shape = RoundedCornerShape(14.dp), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("New scheduled task", fontSize = 20.sp, fontFamily = FontFamily.Serif)
                OutlinedTextField(name, { name = it }, modifier = Modifier.fillMaxWidth(), label = { Text("Name") }, singleLine = true)
                OutlinedTextField(prompt, { prompt = it }, modifier = Modifier.fillMaxWidth(), label = { Text("What should Solmu do?") }, minLines = 3)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(selected = kind == "once", onClick = { kind = "once" }, label = { Text("One time") })
                    FilterChip(selected = kind == "cron", onClick = { kind = "cron" }, label = { Text("Cron") })
                }
                OutlinedTextField(schedule, { schedule = it }, modifier = Modifier.fillMaxWidth(), label = { Text(if (kind == "once") "Future RFC 3339 time" else "Cron expression (UTC)") }, supportingText = { if (kind == "cron") Text("Example: 0 9 * * *") })
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(enabled = name.isNotBlank() && prompt.isNotBlank() && !model.busy, onClick = { model.launch { val value = if (kind == "once") normalizedInstant(schedule) else schedule.trim(); val body = json { put("name", name.trim()); put("prompt", prompt.trim()); put("schedule_kind", kind); put("schedule", value) }; if (editing.isBlank()) model.api.post("/tasks", body) else model.api.patch("/tasks/$editing", body); name = ""; prompt = ""; editing = ""; model.loadTasks() } }) { Text(if (editing.isBlank()) "Create task" else "Save task") }
                    if (editing.isNotBlank()) TextButton(onClick = { editing = ""; name = ""; prompt = "" }) { Text("Cancel") }
                }
            }
        }
        Text("Scheduled tasks", fontSize = 22.sp, fontFamily = FontFamily.Serif)
        if (model.tasks.isEmpty()) Text("No tasks yet.", color = SolmuMuted)
        model.tasks.forEach { task ->
            Card(colors = CardDefaults.cardColors(containerColor = Color.White), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
                Column(Modifier.fillMaxWidth().padding(15.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(task.string("name"), fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
                    Text(task.string("prompt"), color = SolmuMuted, fontSize = 13.sp)
                    Text("${task.string("schedule_kind")}: ${task.string("schedule")} · ${if (task.boolean("enabled")) "Scheduled" else "Paused"}${if (task.boolean("running")) " · Running" else ""}", fontSize = 12.sp)
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        TextButton(enabled = !task.boolean("running"), onClick = { model.launch { model.api.post("/tasks/${task.string("id")}/run"); model.loadTasks() } }) { Text("Run now") }
                        TextButton(enabled = !task.boolean("running"), onClick = { model.launch { model.api.patch("/tasks/${task.string("id")}", json { put("enabled", !task.boolean("enabled")) }); model.loadTasks() } }) { Text(if (task.boolean("enabled")) "Pause" else "Resume") }
                        TextButton(onClick = { if (expandedRuns == task.string("id")) expandedRuns = "" else { expandedRuns = task.string("id"); model.launch { runs = runs + (task.string("id") to model.api.list("/tasks/${task.string("id")}/runs")) } } }) { Text("Run history") }
                        TextButton(enabled = !task.boolean("running"), onClick = { editing = task.string("id"); name = task.string("name"); prompt = task.string("prompt"); kind = task.string("schedule_kind"); schedule = task.string("schedule") }) { Text("Edit") }
                        IconButton(enabled = !task.boolean("running"), onClick = { model.launch { model.api.delete("/tasks/${task.string("id")}"); model.loadTasks() } }) { Icon(Icons.Default.Delete, "Delete task", tint = MaterialTheme.colorScheme.error) }
                    }
                    if (expandedRuns == task.string("id")) {
                        (runs[expandedRuns].orEmpty()).forEach { run -> Text("${run.string("status")} · ${run.string("started_at")} ${run.nullableString("error").orEmpty()}", fontSize = 12.sp, color = SolmuMuted) }
                        if (runs[expandedRuns].isNullOrEmpty()) Text("No runs yet.", fontSize = 12.sp, color = SolmuMuted)
                    }
                }
            }
        }
    }
}

private fun normalizedInstant(value: String): String = runCatching { Instant.parse(value).toString() }.getOrElse {
    runCatching { LocalDateTime.parse(value).atOffset(ZoneOffset.UTC).toInstant().toString() }.getOrDefault(value)
}

@Composable
private fun WebhooksScreen(model: SolmuModel) {
    var name by remember { mutableStateOf("") }
    var secret by remember { mutableStateOf("") }
    var instructions by remember { mutableStateOf("") }
    var authType by remember { mutableStateOf("github-hmac-sha256") }
    LaunchedEffect(Unit) { runCatching { model.loadWebhooks() }.onFailure { model.error = it.message.orEmpty() } }
    PageColumn(title = "Webhooks", subtitle = "Let GitHub and other services start an agent conversation.") {
        Card(colors = CardDefaults.cardColors(containerColor = Color.White), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Add webhook", fontFamily = FontFamily.Serif, fontSize = 20.sp)
                OutlinedTextField(name, { name = it }, modifier = Modifier.fillMaxWidth(), label = { Text("Name") }, singleLine = true)
                OutlinedTextField(secret, { secret = it }, modifier = Modifier.fillMaxWidth(), label = { Text("Secret (16 characters minimum)") }, singleLine = true)
                Text("Authentication", color = SolmuMuted, fontSize = 12.sp)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(selected = authType == "github-hmac-sha256", onClick = { authType = "github-hmac-sha256" }, label = { Text("GitHub HMAC") })
                    FilterChip(selected = authType == "bearer", onClick = { authType = "bearer" }, label = { Text("Bearer") })
                }
                OutlinedTextField(instructions, { instructions = it }, modifier = Modifier.fillMaxWidth(), label = { Text("Instructions") }, minLines = 2)
                Button(enabled = name.isNotBlank() && secret.length >= 16 && !model.busy, onClick = { model.launch { model.api.post("/webhooks", json { put("name", name.trim()); put("secret", secret); put("auth_type", authType); put("instructions", instructions) }); name = ""; secret = ""; instructions = ""; model.loadWebhooks() } }) { Text("Create webhook") }
            }
        }
        model.webhooks.forEach { hook ->
            WebhookCard(model, hook)
        }
    }
}

@Composable
private fun WebhookCard(model: SolmuModel, hook: JSONObject) {
    val id = hook.string("id")
    var name by remember(id) { mutableStateOf(hook.string("name")) }
    var instructions by remember(id) { mutableStateOf(hook.string("instructions")) }
    var authType by remember(id) { mutableStateOf(hook.string("auth_type")) }
    var secret by remember(id) { mutableStateOf("") }
    val endpoint = remember(model.api.baseUrl, hook.optInt("port"), id) {
        runCatching {
            val backend = URI(model.api.baseUrl)
            URI(backend.scheme, null, backend.host, hook.optInt("port"), null, null, null).toString().trimEnd('/') + "/hooks/$id"
        }.getOrDefault("${model.api.baseUrl}/hooks/$id")
    }
    Card(colors = CardDefaults.cardColors(containerColor = Color.White), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
        Column(Modifier.padding(15.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
            Text(name, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("${if (hook.boolean("enabled")) "Enabled" else "Disabled"} · ${hook.string("auth_type")}", modifier = Modifier.weight(1f), color = SolmuMuted, fontSize = 12.sp)
                Text("Enabled", color = SolmuMuted, fontSize = 12.sp)
                Switch(checked = hook.boolean("enabled"), onCheckedChange = { enabled -> model.launch { model.api.patch("/webhooks/$id", json { put("enabled", enabled) }); model.loadWebhooks() } }, enabled = !model.busy)
            }
            Text("Webhook URL", color = SolmuMuted, fontSize = 11.sp)
            Text(endpoint, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
            OutlinedTextField(name, { name = it }, modifier = Modifier.fillMaxWidth(), label = { Text("Name") }, singleLine = true)
            Text("Authentication", color = SolmuMuted, fontSize = 12.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(selected = authType == "github-hmac-sha256", onClick = { authType = "github-hmac-sha256" }, label = { Text("GitHub HMAC") })
                FilterChip(selected = authType == "bearer", onClick = { authType = "bearer" }, label = { Text("Bearer") })
            }
            OutlinedTextField(secret, { secret = it }, modifier = Modifier.fillMaxWidth(), label = { Text("Replace secret (optional)") }, supportingText = { Text(if (hook.boolean("secret_configured")) "Secret is stored. Leave blank to keep it." else "Set a secret with at least 16 characters.") }, singleLine = true)
            OutlinedTextField(instructions, { instructions = it }, modifier = Modifier.fillMaxWidth(), label = { Text("Instructions for Solmu") }, minLines = 2)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(enabled = name.isNotBlank() && !model.busy && (secret.isBlank() || secret.length >= 16), onClick = { model.launch { model.api.patch("/webhooks/$id", json { put("name", name.trim()); put("instructions", instructions); put("auth_type", authType); if (secret.isNotBlank()) put("secret", secret); this }); secret = ""; model.loadWebhooks() } }) { Text("Save changes") }
                TextButton(onClick = { model.launch { model.api.delete("/webhooks/$id"); model.loadWebhooks() } }) { Text("Delete", color = MaterialTheme.colorScheme.error) }
            }
        }
    }
}

@Composable
private fun MoreScreen(model: SolmuModel) {
    PageColumn(title = "More", subtitle = "Workspace status and agent settings.") {
        listOf("Skills" to "Installed workspace instructions", "MCP" to "Connected servers and tools", "Plugins" to "Installed Agent Plugins", "Webhooks" to "External triggers").forEach { (name, description) ->
            Card(Modifier.fillMaxWidth().clickable { model.page = name; model.launch { when (name) { "Skills" -> model.loadSkills(); "MCP" -> model.loadMcp(); "Plugins" -> model.loadPlugins(); "Webhooks" -> model.loadWebhooks() } } }, colors = CardDefaults.cardColors(containerColor = Color.White), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
                Column(Modifier.padding(16.dp)) { Text(name, fontWeight = FontWeight.SemiBold, fontSize = 17.sp); Text(description, color = SolmuMuted, fontSize = 13.sp) }
            }
        }
    }
}

@Composable
private fun CatalogScreen(title: String, catalog: JSONObject?, empty: String, key: String) {
    PageColumn(title = title, subtitle = "Automatically discovered from this conversation’s workspace.") {
        if (catalog == null) CircularProgressIndicator()
        val values = catalog?.array(key).orEmpty()
        if (values.isEmpty()) Text(empty, color = SolmuMuted)
        values.forEach { item ->
            Card(colors = CardDefaults.cardColors(containerColor = Color.White), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
                Column(Modifier.padding(16.dp)) {
                    Text(item.string("name"), fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
                    item.nullableString("description")?.let { Text(it, color = SolmuMuted, lineHeight = 21.sp) }
                    item.nullableString("path")?.let { Text(it, color = SolmuMuted, fontSize = 11.sp, fontFamily = FontFamily.Monospace) }
                    item.array("issues").forEach { Text(it.string("message"), color = MaterialTheme.colorScheme.error, fontSize = 12.sp) }
                }
            }
        }
        catalog?.array("issues")?.forEach { Text("${it.string("path")}: ${it.string("message")}", color = MaterialTheme.colorScheme.error, fontSize = 12.sp) }
    }
}

@Composable
private fun McpScreen(model: SolmuModel) {
    LaunchedEffect(Unit) { if (model.mcp == null) runCatching { model.loadMcp() }.onFailure { model.error = it.message.orEmpty() } }
    PageColumn(title = "MCP", subtitle = "Connected Model Context Protocol servers in this workspace.") {
        val servers = model.mcp?.array("servers").orEmpty()
        if (servers.isEmpty() && model.mcp != null) Text("No MCP servers configured in this workspace.", color = SolmuMuted)
        servers.forEach { server ->
            Card(colors = CardDefaults.cardColors(containerColor = Color.White), border = androidx.compose.foundation.BorderStroke(1.dp, SolmuBorder)) {
                Column(Modifier.padding(16.dp)) {
                    Text(server.string("name"), fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
                    Text("${server.string("status")} · ${server.string("transport")}", color = SolmuGreen, fontSize = 12.sp)
                    server.nullableString("error")?.let { Text(it, color = MaterialTheme.colorScheme.error, fontSize = 12.sp) }
                    server.array("tools").forEach { tool -> Text("${tool.string("name")} — ${tool.string("description")}", modifier = Modifier.padding(top = 8.dp), fontSize = 12.sp) }
                }
            }
        }
        model.mcp?.array("issues")?.forEach { Text("${it.string("path")}: ${it.string("message")}", color = MaterialTheme.colorScheme.error, fontSize = 12.sp) }
    }
}

@Composable
private fun PageColumn(title: String, subtitle: String, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 18.dp, vertical = 20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Text(title.uppercase(), color = SolmuGreen, fontSize = 10.sp, letterSpacing = 1.4.sp)
        Text(title, fontFamily = FontFamily.Serif, fontSize = 38.sp)
        Text(subtitle, color = SolmuMuted, lineHeight = 21.sp)
        content()
    }
}

@Composable
private fun ErrorBanner(error: String, onClose: () -> Unit) {
    Surface(color = Color(0xFFFFF0EE), modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(horizontal = 16.dp, vertical = 9.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(error, modifier = Modifier.weight(1f), color = Color(0xFFA53535), fontSize = 12.sp)
            TextButton(onClick = onClose) { Text("Dismiss") }
        }
    }
}

@Composable
private fun NoticeBanner(message: String, onClose: () -> Unit) {
    Surface(color = SolmuSoft, modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(horizontal = 16.dp, vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(message, modifier = Modifier.weight(1f), color = SolmuGreen, fontSize = 12.sp)
            TextButton(onClick = onClose) { Text("Dismiss") }
        }
    }
}

private fun Double.oneDecimal(): String = String.format(java.util.Locale.ROOT, "%.1f", this)
