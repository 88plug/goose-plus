use crate::agents::extension_manager::ExtensionManager;
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::{effective_role, fix_conversation, Conversation};
use std::path::{Path, PathBuf};

const MIN_CONTEXT_FOR_MOIM: usize = 32_000;
const TURN_CONTEXT_TAG: &str = "turn-context";

const SYSTEM_PROMPT_BLOCK_TEMPLATE: &str = r#"# Turn Context

Each turn, a `<{turn_context_tag}>` block is prepended to the latest user message with current
operational context (time, working directory, compaction status, turn budget, extension context).
Use it to stay oriented; do not treat it as part of the user's request. When `<turn-budget>` runs
low, be more direct: reduce exploration, batch tool calls, and finish the user's task.
"#;

thread_local! {
    pub static SKIP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn system_prompt_block() -> Option<String> {
    if SKIP.with(|f| f.get()) {
        None
    } else {
        Some(SYSTEM_PROMPT_BLOCK_TEMPLATE.replace("{turn_context_tag}", TURN_CONTEXT_TAG))
    }
}

pub async fn inject_moim(
    session_id: &str,
    conversation: Conversation,
    extension_manager: &ExtensionManager,
    turns_taken: u32,
    max_turns: u32,
) -> Conversation {
    if SKIP.with(|f| f.get()) {
        return conversation;
    }

    let session = extension_manager
        .get_context()
        .session_manager
        .get_session(session_id, false)
        .await
        .ok();
    let provider_context_limit =
        extension_manager
            .get_provider()
            .try_lock()
            .ok()
            .and_then(|provider| {
                provider
                    .as_ref()
                    .map(|provider| provider.get_model_config().context_limit())
            });
    let session_context_limit = session.as_ref().and_then(|session| {
        session
            .model_config
            .as_ref()
            .map(|config| config.context_limit())
    });
    let context_limit = provider_context_limit.or(session_context_limit);
    if should_skip_moim(context_limit) {
        return conversation;
    }

    let working_dir = session
        .as_ref()
        .map(|session| session.working_dir.clone())
        .unwrap_or_else(|| PathBuf::from("."));
    let total_tokens = session
        .as_ref()
        .and_then(|session| session.usage.total_tokens);
    let compaction_threshold = crate::config::Config::global()
        .get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
        .unwrap_or(crate::context_mgmt::DEFAULT_COMPACTION_THRESHOLD);
    let extension_parts = extension_manager.collect_moim_parts(session_id).await;
    let moim = compose_moim(
        &working_dir,
        total_tokens,
        context_limit,
        compaction_threshold,
        turns_taken,
        max_turns,
        extension_parts,
    );

    let mut messages = conversation.messages().clone();
    let Some(idx) = messages
        .iter()
        .rposition(|m| m.is_agent_visible() && effective_role(m) == "user")
    else {
        return conversation;
    };

    let ends_with_assistant = messages
        .last()
        .is_some_and(|m| m.role == rmcp::model::Role::Assistant);

    if ends_with_assistant {
        // The conversation ends with an assistant message (e.g. a text-only
        // final reply with no tool call). Splicing MOIM into the earlier
        // user-effective message at `idx` would leave this assistant message
        // trailing, and fix_conversation's fix_lead_trail would then silently
        // drop it to satisfy the "must end with user" API constraint. Append
        // a new trailing user message instead, preserving the assistant content.
        messages.push(Message::user().with_text(moim));
    } else {
        let insert_idx = messages[idx]
            .content
            .iter()
            .take_while(|content| matches!(content, MessageContent::ToolResponse(_)))
            .count();
        messages[idx]
            .content
            .insert(insert_idx, MessageContent::text(moim));
    }

    let (fixed, issues) = fix_conversation(Conversation::new_unvalidated(messages));

    let has_unexpected_issues = issues.iter().any(|issue| {
        !issue.contains("Merged consecutive user messages")
            && !issue.contains("Merged consecutive assistant messages")
            && !issue.contains("Added placeholder to empty tool result")
            && !issue.contains("Trimmed trailing whitespace from assistant message")
            && !issue.contains("Removed trailing assistant message")
            && !issue.contains("Merged text content")
            && !issue.contains("Removed orphaned tool response")
            && !issue.contains("Removed orphaned tool request")
            && !issue.contains("Removed empty message")
            && !issue.contains("Removed leading assistant message")
    });

    if has_unexpected_issues {
        tracing::warn!("MOIM injection caused unexpected issues: {:?}", issues);
        return conversation;
    }

    fixed
}

fn should_skip_moim(context_limit: Option<usize>) -> bool {
    context_limit.is_some_and(|limit| limit < MIN_CONTEXT_FOR_MOIM)
}

fn compose_moim(
    working_dir: &Path,
    total_tokens: Option<i32>,
    context_limit: Option<usize>,
    compaction_threshold: f64,
    turns_taken: u32,
    max_turns: u32,
    extension_parts: Vec<String>,
) -> String {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:00");
    let mut lines = vec![
        open_tag(TURN_CONTEXT_TAG),
        tag("current-time", &timestamp.to_string()),
        tag("working-directory", &working_dir.display().to_string()),
    ];

    if let Some(value) =
        compaction_remaining_line(total_tokens, context_limit, compaction_threshold)
    {
        lines.push(tag("compaction", &value));
    }
    if let Some(value) = turn_budget_line(turns_taken, max_turns) {
        lines.push(tag("turn-budget", &value));
    }

    for part in extension_parts {
        if !part.trim().is_empty() {
            lines.push(String::new());
            lines.push(part);
        }
    }

    lines.push(close_tag(TURN_CONTEXT_TAG));
    lines.join("\n")
}

fn open_tag(name: &str) -> String {
    format!("<{name}>")
}

fn close_tag(name: &str) -> String {
    format!("</{name}>")
}

fn tag(name: &str, value: &str) -> String {
    format!("<{name}>{}</{name}>", escape_xml_text(value))
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn compaction_remaining_line(
    total_tokens: Option<i32>,
    context_limit: Option<usize>,
    threshold: f64,
) -> Option<String> {
    let total_tokens = total_tokens?;
    let context_limit = context_limit?;

    if total_tokens <= 0 || context_limit == 0 || threshold <= 0.0 || threshold >= 1.0 {
        return None;
    }

    let compaction_at = (context_limit as f64 * threshold) as i32;
    if compaction_at <= 0 || (total_tokens as f64 / compaction_at as f64) < 0.5 {
        return None;
    }

    Some(format!(
        "~{}k tokens remaining",
        (compaction_at - total_tokens).max(0) / 1000
    ))
}

fn turn_budget_line(turns_taken: u32, max_turns: u32) -> Option<String> {
    if max_turns == 0 || turns_taken.saturating_mul(2) < max_turns {
        return None;
    }

    Some(format!("{turns_taken}/{max_turns} used"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use rmcp::model::CallToolRequestParams;

    fn text_at(message: &crate::conversation::message::Message, index: usize) -> &str {
        message.content[index].as_text().unwrap()
    }

    fn is_moim(content: &MessageContent) -> bool {
        content
            .as_text()
            .is_some_and(|text| text.starts_with(&format!("<{}>\n", TURN_CONTEXT_TAG)))
    }

    #[tokio::test]
    async fn test_moim_prepended_to_latest_user_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let session = em
            .get_context()
            .session_manager
            .create_session(
                PathBuf::from("/test/dir"),
                "test".to_string(),
                crate::session::SessionType::User,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();

        let conv = Conversation::new_unvalidated(vec![
            Message::user().with_text("Hello"),
            Message::assistant().with_text("Hi"),
            Message::user().with_text("Bye"),
        ]);
        let result = inject_moim(&session.id, conv, &em, 0, 100).await;
        let msgs = result.messages();

        assert_eq!(msgs.len(), 3);
        assert_eq!(text_at(&msgs[0], 0), "Hello");
        assert_eq!(text_at(&msgs[1], 0), "Hi");
        assert!(is_moim(&msgs[2].content[0]));
        assert_eq!(text_at(&msgs[2], 1), "Bye");
    }

    #[tokio::test]
    async fn test_moim_injection_no_assistant() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let session = em
            .get_context()
            .session_manager
            .create_session(
                PathBuf::from("/test/dir"),
                "test".to_string(),
                crate::session::SessionType::User,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();

        let conv = Conversation::new_unvalidated(vec![Message::user().with_text("Hello")]);
        let result = inject_moim(&session.id, conv, &em, 0, 100).await;

        assert_eq!(result.messages().len(), 1);
        assert!(is_moim(&result.messages()[0].content[0]));
        assert_eq!(text_at(&result.messages()[0], 1), "Hello");
    }

    #[tokio::test]
    async fn test_moim_skips_user_messages_not_visible_to_agent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let session = em
            .get_context()
            .session_manager
            .create_session(
                PathBuf::from("/test/dir"),
                "test".to_string(),
                crate::session::SessionType::User,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();

        let conv = Conversation::new_unvalidated(vec![
            Message::user().with_text("agent visible"),
            Message::assistant().with_text("reply"),
            Message::user().with_text("user only").user_only(),
        ]);
        let result = inject_moim(&session.id, conv, &em, 0, 100).await;
        let msgs = result.messages();

        assert_eq!(msgs.len(), 2);
        assert!(is_moim(&msgs[0].content[0]));
        assert_eq!(text_at(&msgs[0], 1), "agent visible");
        assert_eq!(text_at(&msgs[1], 0), "user only");
        assert!(!msgs[1].is_agent_visible());
    }

    #[tokio::test]
    async fn test_moim_with_tool_calls() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let session = em
            .get_context()
            .session_manager
            .create_session(
                PathBuf::from("/test/dir"),
                "test".to_string(),
                crate::session::SessionType::User,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();

        let conv = Conversation::new_unvalidated(vec![
            Message::user().with_text("Search for something"),
            Message::assistant()
                .with_text("I'll search for you")
                .with_tool_request("search_1", Ok(CallToolRequestParams::new("search"))),
            Message::user()
                .with_tool_response("search_1", Ok(rmcp::model::CallToolResult::success(vec![]))),
        ]);

        let result = inject_moim(&session.id, conv, &em, 0, 100).await;
        let msgs = result.messages();

        assert_eq!(msgs.len(), 3);
        assert!(is_moim(&msgs[0].content[0]));
        assert_eq!(text_at(&msgs[0], 1), "Search for something");
        assert!(matches!(
            &msgs[2].content[0],
            MessageContent::ToolResponse(_)
        ));
        assert_eq!(msgs[2].content.len(), 1);
    }

    // Orphaned tool_use/tool_result blocks (e.g. from a cancelled turn) cause
    // persistent provider 400 errors. fix_conversation removes them, but MOIM
    // used to discard that fix because orphan removal wasn't in its allowlist,
    // returning the still-broken conversation to the provider every turn.
    #[tokio::test]
    async fn test_moim_fixes_orphaned_tool_request() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let session = em
            .get_context()
            .session_manager
            .create_session(
                PathBuf::from("/test/dir"),
                "test".to_string(),
                crate::session::SessionType::User,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();

        let conv = Conversation::new_unvalidated(vec![
            Message::user().with_text("Do something"),
            Message::assistant()
                .with_text("I'll call a tool")
                .with_tool_request("orphan_tool_1", Ok(CallToolRequestParams::new("some_tool"))),
        ]);

        let (_, issues) = fix_conversation(Conversation::new_unvalidated(conv.messages().clone()));
        assert!(
            issues
                .iter()
                .any(|i| i.contains("Removed orphaned tool request")),
            "fix_conversation alone should detect the orphan, but issues were: {:?}",
            issues
        );

        let result = inject_moim(&session.id, conv, &em, 0, 100).await;
        let msgs = result.messages();

        let has_orphan = msgs.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, MessageContent::ToolRequest(tr) if tr.id == "orphan_tool_1"))
        });
        assert!(
            !has_orphan,
            "Orphaned tool request should have been removed by MOIM's fix_conversation"
        );
        assert!(
            msgs.iter().any(|m| m.content.iter().any(is_moim)),
            "MOIM should have been injected after fixing the orphan"
        );
    }

    #[tokio::test]
    async fn test_moim_fixes_cancellation_orphan() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let session = em
            .get_context()
            .session_manager
            .create_session(
                PathBuf::from("/test/dir"),
                "test".to_string(),
                crate::session::SessionType::User,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();

        // Simulates what the agent produces after cancellation: a tool call was
        // issued, but the pre-allocated response message never got populated.
        let conv = Conversation::new_unvalidated(vec![
            Message::user().with_text("Search for something"),
            Message::assistant().with_text("I searched and found results"),
            Message::user().with_text("Now do something else"),
            Message::assistant()
                .with_text("I'll call a tool")
                .with_tool_request(
                    "cancelled_tool",
                    Ok(CallToolRequestParams::new("some_tool")),
                ),
            Message::user(),
        ]);

        let result = inject_moim(&session.id, conv, &em, 0, 100).await;
        let msgs = result.messages();

        let has_orphan = msgs.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, MessageContent::ToolRequest(tr) if tr.id == "cancelled_tool"))
        });
        assert!(
            !has_orphan,
            "Orphaned tool request should have been removed by MOIM's fix_conversation"
        );

        let has_empty = msgs.iter().any(|m| m.content.is_empty());
        assert!(
            !has_empty,
            "Empty user message should have been removed by MOIM's fix_conversation"
        );

        assert!(
            msgs.iter().any(|m| m.content.iter().any(is_moim)),
            "MOIM should have been injected after fixing the cancellation orphan"
        );
    }

    #[tokio::test]
    async fn test_moim_preserves_trailing_text_only_assistant_message() {
        // Regression test for upstream issue #7425: when the conversation ends
        // with a text-only assistant message (no tool call), MOIM is inserted
        // into an earlier user message, leaving the assistant message trailing.
        // fix_conversation's fix_lead_trail then silently drops any trailing
        // assistant message to satisfy the "must end with user" API constraint —
        // discarding the model's own final response text instead of preserving it.
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let session = em
            .get_context()
            .session_manager
            .create_session(
                PathBuf::from("/test/dir"),
                "test".to_string(),
                crate::session::SessionType::User,
                crate::config::GooseMode::Auto,
            )
            .await
            .unwrap();

        let conv = Conversation::new_unvalidated(vec![
            Message::user().with_text("clean up comments in check.sh"),
            Message::assistant()
                .with_text("Let me look at the file")
                .with_tool_request("tool_1", Ok(CallToolRequestParams::new("text_editor"))),
            Message::user()
                .with_tool_response("tool_1", Ok(rmcp::model::CallToolResult::success(vec![]))),
            Message::assistant().with_text("Now I'll clean this up..."),
        ]);

        let result = inject_moim(&session.id, conv, &em, 0, 100).await;
        let msgs = result.messages();

        assert!(
            msgs.iter().any(|m| m.content.iter().any(is_moim)),
            "MOIM should still be injected"
        );

        let has_trailing_assistant = msgs.iter().any(|m| {
            m.role == rmcp::model::Role::Assistant
                && m.content.iter().any(|c| {
                    c.as_text()
                        .is_some_and(|t| t.contains("Now I'll clean this up"))
                })
        });
        assert!(
            has_trailing_assistant,
            "trailing assistant message should be preserved, not silently dropped"
        );
    }
}
