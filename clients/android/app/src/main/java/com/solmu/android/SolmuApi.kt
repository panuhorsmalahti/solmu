package com.solmu.android

import android.os.Handler
import android.os.Looper
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext

class SolmuApi(val baseUrl: String) {
    private val http = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .build()

    suspend fun get(path: String): JSONObject = call("GET", path)
    suspend fun post(path: String, body: JSONObject = JSONObject()): JSONObject = call("POST", path, body)
    suspend fun put(path: String, body: JSONObject): JSONObject = call("PUT", path, body)
    suspend fun patch(path: String, body: JSONObject): JSONObject = call("PATCH", path, body)
    suspend fun delete(path: String): JSONObject = call("DELETE", path)

    suspend fun list(path: String): List<JSONObject> {
        val result = get(path)
        val items = result.optJSONArray("items") ?: result.optJSONArray("servers") ?: return emptyList()
        return items.objects()
    }

    private suspend fun call(method: String, path: String, body: JSONObject? = null): JSONObject = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url(apiUrl(path))
            .method(method, body?.toString()?.toRequestBody(JSON) ?: if (method == "GET" || method == "DELETE") null else "{}".toRequestBody(JSON))
            .build()
        http.newCall(request).execute().use { response ->
            val text = response.body?.string().orEmpty()
            val payload = if (text.isBlank()) JSONObject() else runCatching {
                if (text.trimStart().startsWith("[")) JSONObject().put("items", JSONArray(text)) else JSONObject(text)
            }.getOrElse { JSONObject().put("raw", text) }
            if (!response.isSuccessful) {
                val message = payload.optJSONObject("error")?.optString("message")?.takeIf(String::isNotBlank)
                    ?: "Request failed (${response.code})"
                throw SolmuApiException(message)
            }
            payload
        }
    }

    suspend fun streamReply(threadId: String, messageId: String, onEvent: suspend (String, JSONObject) -> Unit) = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url(apiUrl("/threads/$threadId/responses"))
            .post(JSONObject().put("message_id", messageId).toString().toRequestBody(JSON))
            .header("Accept", "text/event-stream")
            .build()
        val job: Job? = currentCoroutineContext()[Job]
        val call = http.newCall(request)
        val registration = job?.invokeOnCompletion { cause -> if (cause is CancellationException) call.cancel() }
        try {
            call.execute().use { response ->
                if (!response.isSuccessful) {
                    val message = response.body?.string().orEmpty()
                    val payload = runCatching { JSONObject(message) }.getOrNull()
                    throw SolmuApiException(payload?.optJSONObject("error")?.optString("message") ?: "Reply failed (${response.code})")
                }
                val source = response.body?.source() ?: throw SolmuApiException("Streaming is unavailable")
                var event = "message"
                val data = StringBuilder()
                while (!source.exhausted() && currentCoroutineContext().isActive) {
                    val line = source.readUtf8Line() ?: break
                    if (line.isEmpty()) {
                        if (data.isNotEmpty()) {
                            val payload = runCatching { JSONObject(data.toString().trimEnd('\n')) }.getOrElse { JSONObject() }
                            if (event == "error") throw SolmuApiException(payload.optJSONObject("error")?.optString("message") ?: "The reply failed")
                            if (event == "stopped") return@withContext
                            withContext(Dispatchers.Main.immediate) { onEvent(event, payload) }
                        }
                        event = "message"
                        data.clear()
                    } else if (line.startsWith("event:")) {
                        event = line.removePrefix("event:").trim()
                    } else if (line.startsWith("data:")) {
                        data.append(line.removePrefix("data:").removePrefix(" ")).append('\n')
                    }
                }
            }
        } finally {
            registration?.dispose()
        }
    }

    fun observe(onEvent: (String) -> Unit, onConnection: (Boolean) -> Unit): () -> Unit {
        val handler = Handler(Looper.getMainLooper())
        var stopped = false
        var socket: WebSocket? = null
        lateinit var connect: Runnable
        connect = Runnable {
            if (stopped) return@Runnable
            val url = apiUrl("/events").replaceFirst("https://", "wss://").replaceFirst("http://", "ws://")
            socket = http.newWebSocket(Request.Builder().url(url).build(), object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    handler.post { onConnection(true) }
                }
                override fun onMessage(webSocket: WebSocket, text: String) {
                    val type = runCatching { JSONObject(text).optString("type") }.getOrDefault("")
                    if (type.isNotBlank() && type != "ready") handler.post { onEvent(type) }
                }
                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    handler.post { onConnection(false) }
                    if (!stopped) handler.postDelayed(connect, 1500)
                }
                override fun onFailure(webSocket: WebSocket, error: Throwable, response: Response?) {
                    handler.post {
                        onConnection(false)
                        if (!stopped) handler.postDelayed(connect, 1500)
                    }
                }
            })
        }
        connect.run()
        return {
            stopped = true
            handler.removeCallbacks(connect)
            socket?.cancel()
        }
    }

    private fun apiUrl(path: String): String = "${baseUrl.trimEnd('/')}/api/v1${if (path.startsWith('/')) path else "/$path"}"

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()
    }
}

class SolmuApiException(message: String) : Exception(message)

internal fun JSONArray.objects(): List<JSONObject> = (0 until length()).mapNotNull { optJSONObject(it) }
internal fun JSONObject.string(key: String, fallback: String = ""): String = optString(key, fallback).takeUnless { it == "null" } ?: fallback
internal fun JSONObject.nullableString(key: String): String? = if (isNull(key)) null else string(key).takeIf(String::isNotBlank)
internal fun JSONObject.boolean(key: String, fallback: Boolean = false): Boolean = if (has(key)) optBoolean(key, fallback) else fallback
internal fun JSONObject.array(key: String): List<JSONObject> = optJSONArray(key)?.objects().orEmpty()
internal fun json(block: JSONObject.() -> JSONObject): JSONObject = JSONObject().block()
