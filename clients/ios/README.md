# Solmu iOS

Solmu for iOS is a native SwiftUI app. It connects to the Solmu backend over
REST and WebSocket, and streams replies directly from the backend. It does not
embed the web client.

## Build and run

The iOS app requires a Mac with Xcode. From this folder, generate the Xcode
project with [XcodeGen](https://github.com/yonaskolb/XcodeGen), then open it in
Xcode:

```sh
brew install xcodegen
xcodegen generate --spec project.yml
open Solmu.xcodeproj
```

Select the **Solmu** scheme and an iPhone simulator or connected iPhone. The
app remembers the backend address on the device. On the same Mac, use
`http://127.0.0.1:3000`; on a phone, use the Mac's LAN address, for example
`http://192.168.1.20:3000`. The backend must listen on that interface. See the
[iOS setup guide](../../docs/ios.md) for network settings.

## Use

The app shares conversations, Profile settings, tools, tasks, and workspace
configuration with the other Solmu clients. Select or create threads, stream
and stop replies, rename or delete threads, choose a model, compact history,
copy or export a conversation, and inspect its status and context. The tabs
provide Profile, paged Audit, Tasks, and workspace Skills, MCP, Plugins, and
Webhooks. Open pages update automatically as backend events arrive.

## Screenshot

![Solmu native iOS client](../../docs/screenshots/ios.png)
