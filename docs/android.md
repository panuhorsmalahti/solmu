# Android

Solmu for Android is a native Kotlin and Jetpack Compose app. It connects
directly to the backend REST API, streams replies, and receives live changes
over WebSocket. No web page is embedded in the app.

## Connect

On the Android emulator, the default backend address is
`http://10.0.2.2:3000`. On a physical phone, enter the computer's LAN address,
such as `http://192.168.1.20:3000`. The app remembers the address on that phone.

For a phone to reach a backend on your computer, configure the backend to listen
on its LAN interface with `SOLMU_BIND_ADDR=0.0.0.0:3000`, and allow that port
through the computer's firewall. The backend currently has no login, so only
make it reachable on a network you trust. The default `127.0.0.1:3000` binding
continues to accept connections only from the same computer.

## Use

The conversation screen creates or resumes a thread, streams replies, and
supports stop, compaction, model selection, rename, delete, copy, and export.
The bottom navigation opens Profile, Audit, Tasks, and More. More includes
Skills, MCP, Plugins, and Webhooks. The app updates conversation lists and
other open pages automatically when the backend changes.

Android supports the same saved conversations, per-thread models, editable
Profile prompt, tool history, prompt-cache audit, scheduled tasks, webhook
management, skills, MCP servers, and Agent Plugins as the other clients. See
the guides for [Profile](profile.md), [Audit](audit.md), [Tasks](tasks.md),
[skills](skills.md), [MCP](mcp.md), [plugins](plugins.md), and
[webhooks](webhooks.md).

## Build

Open `clients/android` in Android Studio and run the `app` configuration on an
emulator or device. The project uses Android SDK 36, Java 17, and Gradle 8.13.
If Gradle is installed separately, build and install the debug app with:

```sh
gradle -p clients/android installDebug
```

The repository's Android end-to-end test runs the native app in an emulator
against a local API fixture. See [development](development.md) for the other
client test commands.

To install a published Android build, download the signed
`solmu-v<version>-android.apk` from [GitHub Releases](https://github.com/panuhorsmalahti/solmu/releases)
and open it on your device. See [release installation](releases.md#install) for
the first-time Android signing setup used by the release workflow.

![Solmu native Android client](screenshots/android.png)
