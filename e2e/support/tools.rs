use super::*;

pub fn calls(body: &Value) -> Option<Vec<Value>> {
    let messages = body["messages"].as_array()?;
    let (index, message) = messages.iter().enumerate().rev().find(|(_, message)| {
        message["role"] == "user"
            && !message["content"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(|part| part["type"] == "tool_result"))
    })?;
    if messages[index + 1..].iter().any(|message| {
        message["role"] == "tool"
            || message["content"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(|part| part["type"] == "tool_result"))
    }) {
        return None;
    }
    let text = message["content"].as_str().map(str::to_owned).or_else(|| {
        message["content"].as_array().map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
    })?;
    if text == "Run a long task" {
        return Some(vec![
            json!({"name":"Bash","arguments":{"command":"sleep 20"}}),
        ]);
    }
    if text == "TOOLS" || text == "Inspect this workspace" {
        return Some(vec![
            json!({"name":"Write","arguments":{"path":"hello.txt","content":"Hello tools\n"}}),
            json!({"name":"Read","arguments":{"path":"hello.txt"}}),
            json!({"name":"Edit","arguments":{"path":"hello.txt","old_string":"Hello","new_string":"Welcome"}}),
            json!({"name":"Glob","arguments":{"pattern":"**/*.txt"}}),
            json!({"name":"Grep","arguments":{"pattern":"Welcome"}}),
            json!({"name":"Bash","arguments":{"command":"printf 'Workspace ready'"}}),
        ]);
    }
    serde_json::from_str(text.strip_prefix("TOOLS ")?).ok()
}
pub fn response(kind: &'static str, calls: Vec<Value>) -> Response {
    let stream = async_stream::stream! {
        if kind=="anthropic" {
            yield Ok::<_,Infallible>(Event::default().event("message_start").data(json!({"type":"message_start","message":{"id":"tools","type":"message","role":"assistant","content":[],"model":"test-model","usage":{"input_tokens":1,"output_tokens":0}}}).to_string()));
        }
        for (index,call) in calls.into_iter().enumerate() {
            let id=format!("call-{index}");
            if kind=="anthropic" {
                yield Ok(Event::default().event("content_block_start").data(json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":id,"name":call["name"],"input":{}}}).to_string()));
                yield Ok(Event::default().event("content_block_delta").data(json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":call["arguments"].to_string()}}).to_string()));
                yield Ok(Event::default().event("content_block_stop").data(json!({"type":"content_block_stop","index":index}).to_string()));
            } else {
                yield Ok(Event::default().data(json!({"id":"tools","object":"chat.completion.chunk","created":0,"model":"test-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":index,"id":id,"type":"function","function":{"name":call["name"],"arguments":call["arguments"].to_string()}}]},"finish_reason":null}]}).to_string()));
            }
        }
        if kind=="anthropic" {
            yield Ok(Event::default().event("message_delta").data("{\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":2}}"));
            yield Ok(Event::default().event("message_stop").data("{\"type\":\"message_stop\"}"));
        } else {
            yield Ok(Event::default().data(json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}).to_string()));
            yield Ok(Event::default().data("[DONE]"));
        }
    };
    Sse::new(stream).into_response()
}
