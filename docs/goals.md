# Goals

Goals let Solmu keep track of a multi-step objective across conversations and
restarts. Create one from any client by entering `/goal <objective>`, or ask
Solmu to keep working toward an objective; the agent can start a goal through
its `Goals` tool. A plain question or one-turn request does not need a goal.

Entering `/goal` lists saved goals. Each goal has a persistent ID and one of
four states: active, paused, completed, or cancelled. The agent receives active
goals as context on later replies and can update their status when work is
paused or finished. When Solmu starts a goal while handling a natural-language
request, it can begin work during that response. `/goal` records the objective
for follow-up replies. Goals do not launch a separate background worker.

Goals can also be created and managed through `GET /api/v1/goals`,
`POST /api/v1/goals`, and `PATCH /api/v1/goals/{id}`. Updates are announced to
connected clients over the backend event socket.
