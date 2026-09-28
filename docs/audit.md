# Audit

Audit shows saved tool calls from every conversation in one timeline, newest
first. Each row shows the tool, its status, conversation, and time. Expand a
row to see the arguments, result, call ID, and start and finish times.

- **Web:** choose **Audit** under Profile in the left sidebar, or open `/audit`.
  Older calls load automatically as you scroll. The list updates when a tool
  starts or finishes, when conversations change, and after reconnecting.
- **Desktop:** choose **Audit** under Profile. Use **Previous** and **Next** to
  browse pages; click a call for details.
- **CLI and Muxer:** run `/audit` in a Solmu terminal. Up and Down select a call,
  Enter opens its details, PageUp and PageDown change pages, and `j` and `k`
  scroll expanded details. Esc returns to the conversation.

Audit includes completed, failed, cancelled, interrupted, and in-progress calls.
It is saved in the backend database with each conversation. Deleting a
conversation also removes its tool calls from Audit.

The [API](api.md) provides cursor paging at `GET /api/v1/audit`. Each page has
`items` and a `next_cursor`; request the next page with `before=<next_cursor>`.
The default page size is 25 and the maximum is 100.

![CLI Audit](screenshots/cli-audit.png)

![Desktop Audit](screenshots/desktop-audit.png)

![Web Audit](screenshots/web-audit.png)

![Audit in Muxer](screenshots/muxer-audit.png)
