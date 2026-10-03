package com.solmu.android

import android.content.Context
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodes
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.FileOutputStream
import java.util.Collections
import java.util.concurrent.atomic.AtomicReference

@RunWith(AndroidJUnit4::class)
class NativeAndroidClientTest {
    private val server = MockWebServer()
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val messages = Collections.synchronizedList(mutableListOf<String>())
    private val profilePrompt = AtomicReference("You are Solmu, an autonomous agent.")
    private val taskCreated = AtomicReference(false)
    private val webhookCreated = AtomicReference(false)

    init {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                val path = request.path.orEmpty().substringBefore('?')
                return when {
                    path == "/api/v1/events" -> MockResponse().withWebSocketUpgrade(object : okhttp3.WebSocketListener() {})
                    path == "/api/v1/threads" && request.method == "GET" -> jsonResponse("""{"items":[${thread()}],"limit":100,"offset":0}""")
                    path == "/api/v1/threads" && request.method == "POST" -> jsonResponse(thread())
                    path == "/api/v1/threads/thread-android" && request.method == "GET" -> jsonResponse(thread())
                    path == "/api/v1/threads/thread-android/messages" && request.method == "GET" -> jsonResponse("""{"items":${messagesJson()},"limit":100,"offset":0}""")
                    path == "/api/v1/threads/thread-android/tools" -> jsonResponse("""{"items":[],"limit":100,"offset":0}""")
                    path == "/api/v1/threads/thread-android/messages" && request.method == "POST" -> {
                        val input = request.body.readUtf8()
                        val text = org.json.JSONObject(input).getString("content")
                        messages.add("""{"id":"message-user","thread_id":"thread-android","role":"user","content":${org.json.JSONObject.quote(text)}}""")
                        jsonResponse(messages.last())
                    }
                    path == "/api/v1/threads/thread-android/responses" -> {
                        messages.add("""{"id":"message-assistant","thread_id":"thread-android","role":"assistant","content":"Hello from Solmu"}""")
                        MockResponse().setHeader("Content-Type", "text/event-stream").setBody(
                            "event: delta\ndata: {\"text\":\"Hello from Solmu\"}\n\n" +
                                "event: done\ndata: ${messages.last()}\n\n",
                        )
                    }
                    path == "/api/v1/threads/thread-android/compact" -> jsonResponse("{}")
                    path == "/api/v1/profile" && request.method == "GET" -> jsonResponse("""{"system_prompt":${org.json.JSONObject.quote(profilePrompt.get())},"model":null,"backend_default_model":"gpt-6-sol","edited_at":"2026-10-03T00:00:00Z"}""")
                    path == "/api/v1/profile" && request.method == "PUT" -> {
                        profilePrompt.set(org.json.JSONObject(request.body.readUtf8()).getString("system_prompt"))
                        jsonResponse("""{"system_prompt":${org.json.JSONObject.quote(profilePrompt.get())},"model":null,"backend_default_model":"gpt-6-sol","edited_at":"2026-10-03T00:00:00Z"}""")
                    }
                    path == "/api/v1/models" -> jsonResponse("""{"provider":"OpenAI","default_model":"gpt-6-sol","models":[{"id":"gpt-6-sol","name":"GPT 6 Sol"}]}""")
                    path == "/api/v1/audit" -> jsonResponse("""{"items":[],"next_cursor":null,"cache_24h":{"requests":2,"input_tokens":100,"output_tokens":20,"cached_input_tokens":40,"cache_creation_input_tokens":5,"hit_rate_percent":40.0}}""")
                    path == "/api/v1/tasks" && request.method == "GET" -> jsonResponse("""{"items":${if (taskCreated.get()) "[${task()}]" else "[]"},"limit":10,"offset":0}""")
                    path == "/api/v1/tasks" && request.method == "POST" -> { taskCreated.set(true); jsonResponse(task()) }
                    path == "/api/v1/webhooks" && request.method == "GET" -> jsonResponse("""${if (webhookCreated.get()) "[${webhook()}]" else "[]"}""")
                    path == "/api/v1/webhooks" && request.method == "POST" -> { webhookCreated.set(true); jsonResponse(webhook()) }
                    path.endsWith("/skills") -> jsonResponse("""{"directory":".agents/skills","items":[],"issues":[]}""")
                    path.endsWith("/mcp") -> jsonResponse("""{"workspace":"/workspace","files":[],"servers":[],"issues":[]}""")
                    path.endsWith("/plugins") -> jsonResponse("""{"directory":".agents/plugins","items":[],"issues":[]}""")
                    path.endsWith("/runs") -> jsonResponse("""{"items":[]}""")
                    else -> MockResponse().setResponseCode(404).setBody("""{"error":{"message":"No fake route for $path"}}""")
                }
            }
        }
        server.start()
        context.getSharedPreferences("solmu", Context.MODE_PRIVATE).edit()
            .putString("backend", server.url("/").toString().trimEnd('/'))
            .commit()
    }

    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @After
    fun closeServer() {
        context.getSharedPreferences("solmu", Context.MODE_PRIVATE).edit().clear().commit()
        server.shutdown()
    }

    @Test
    fun nativeClientStreamsRepliesAndManagesItsCorePages() {
        compose.waitUntil(15_000) {
            compose.onAllNodesWithText("A little space for your next big idea.").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag("message-input").performTextInput("Hello from Android")
        compose.onNodeWithTag("send-message").performClick()
        compose.waitUntil(15_000) {
            compose.onAllNodesWithText("Hello from Solmu").fetchSemanticsNodes().isNotEmpty()
        }

        compose.onNodeWithTag("nav-Profile").performClick()
        compose.onNodeWithText("System prompt").assertIsDisplayed()
        compose.onAllNodes(hasSetTextAction()).get(0).performTextInput("\nBe concise.")
        compose.onNodeWithText("Save profile").performScrollTo().performClick()
        compose.waitUntil(5_000) { profilePrompt.get().contains("Be concise.") }
        assertTrue(profilePrompt.get().contains("You are Solmu"))

        compose.onNodeWithTag("nav-Audit").performClick()
        compose.onNodeWithText("40.0%", substring = false).assertIsDisplayed()

        compose.onNodeWithTag("nav-Tasks").performClick()
        compose.onAllNodes(hasSetTextAction()).get(0).performScrollTo().performTextInput("Android follow-up")
        compose.onAllNodes(hasSetTextAction()).get(1).performScrollTo().performTextInput("Review the workspace")
        compose.onNodeWithText("Create task").performScrollTo().performClick()
        compose.waitUntil(5_000) { taskCreated.get() }

        compose.onNodeWithTag("nav-More").performClick()
        compose.onNodeWithText("Webhooks").performClick()
        compose.onAllNodes(hasSetTextAction()).get(1).performScrollTo().performTextInput("android-test-secret-123")
        compose.onNodeWithText("Create webhook").performScrollTo().performClick()
        compose.waitUntil(5_000) { webhookCreated.get() }

        compose.onNodeWithText("Change backend").assertDoesNotExist()
        compose.onNodeWithTag("nav-More").performClick()
        compose.onNodeWithText("Skills").performClick()
        compose.onNodeWithText("No skills installed in this workspace.").assertIsDisplayed()
        compose.onNodeWithTag("nav-Chat").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithText("Hello from Solmu").fetchSemanticsNodes().isNotEmpty() }
        val screenshot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        val screenshotFile = File(context.filesDir, "android-e2e.png")
        FileOutputStream(screenshotFile).use { assertTrue(screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)) }
    }

    private fun thread() = """{"id":"thread-android","title":"New conversation","model":null,"workspace":"/workspace"}"""
    private fun messagesJson() = "[${messages.joinToString(",")}]"
    private fun task() = """{"id":"task-android","name":"Android follow-up","prompt":"Review the workspace","schedule_kind":"cron","schedule":"0 9 * * *","thread_id":"thread-android","enabled":true,"running":false,"next_run_at":"2026-10-04T09:00:00Z","last_run_at":null,"last_status":null}"""
    private fun webhook() = """{"id":"hook-android","name":"Android webhook","enabled":true,"auth_type":"github-hmac-sha256","secret_configured":true,"instructions":"","created_at":"2026-10-03T00:00:00Z","updated_at":"2026-10-03T00:00:00Z","port":3001}"""

    private fun jsonResponse(body: String) = MockResponse().setHeader("Content-Type", "application/json").setBody(body)
}
