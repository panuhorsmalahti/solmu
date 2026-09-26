# LLM provider libraries for Solmu

Researched on 2026-09-26. No LLM dependency has been added yet.

Rust has several libraries covering the unified provider calls, streaming, and
tool calling that motivate using an AI SDK. The best fit depends on how much
agent orchestration we want the library to own.

| Library | Scope | Fit for Solmu |
| --- | --- | --- |
| [genai](https://github.com/jeremychone/rust-genai) | Common chat API across native provider protocols, streaming, vision, function calling, and structured output. Supports OpenAI, Anthropic, Gemini, Ollama, and others. | My initial recommendation for provider access while we build our own agent loop. |
| [Rig](https://github.com/0xPlaygrounds/rig) | Unified providers, streaming, tools, structured extraction, embeddings, vector stores, and agent workflows. | A strong alternative if we want a library to also provide agent orchestration and retrieval. |
| [async-openai](https://github.com/64bit/async-openai) | Unofficial client for OpenAI APIs, including Responses and streaming; configurable for compatible endpoints. | Useful when detailed OpenAI API coverage matters. Native Anthropic and Gemini access would require additional adapters. |

## Initial recommendation

Start with `genai` when we add the first provider call. Its shared chat interface
and native provider adapters match the immediate requirement. We can keep
conversation state, tool execution, cancellation, and frontend events under our
own control. This recommendation is a design judgment based on the documented
scope; we have not evaluated the libraries through live calls or benchmarks.

The genai README currently identifies `0.6` as the released line and describes
`0.7.0-beta` separately. Examples on the main branch include beta APIs; use docs
and examples matching the version selected at implementation time. Its focus is
common chat capabilities, rather than complete coverage of every provider API.

Rig is also worth considering before we implement a substantial agent runtime.
Its current documentation separates provider contracts in `rig-core` from agent
orchestration in `rig-agent`, exposed through the `rig` facade. The maintainers
explicitly anticipate breaking changes, so use version-matched documentation.

Provider capabilities differ. Before adopting either library, check the chosen
version against our actual models, especially streaming tool calls, structured
output, and preservation of reasoning metadata across tool turns.

These libraries cover backend concerns. Frontend communication will need a
separate API and event format once we choose client technologies.

Sources: [genai README](https://github.com/jeremychone/rust-genai#readme),
[Rig README](https://github.com/0xPlaygrounds/rig#readme), and
[async-openai README](https://github.com/64bit/async-openai#readme).
