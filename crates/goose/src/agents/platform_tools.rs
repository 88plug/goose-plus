use indoc::indoc;
use rmcp::model::{Tool, ToolAnnotations};
use rmcp::object;
pub const PLATFORM_MANAGE_SCHEDULE_TOOL_NAME: &str = "platform__manage_schedule";

pub fn manage_schedule_tool() -> Tool {
    Tool::new(
        PLATFORM_MANAGE_SCHEDULE_TOOL_NAME.to_string(),
        indoc! {r#"
            Manage goose's internal scheduled recipe execution.

            Actions:
            - "list": List all goose scheduled jobs
            - "create": Create a new goose scheduled job from a recipe file
            - "run_now": Execute a goose scheduled job immediately
            - "pause": Pause a goose scheduled job
            - "unpause": Resume a paused goose scheduled job
            - "delete": Remove a goose scheduled job
            - "kill": Terminate a currently running goose scheduled job
            - "inspect": Get details about a running goose scheduled job
            - "sessions": List execution history for a goose scheduled job
            - "session_content": Get the full content (messages) of a specific session
        "#}
        .to_string(),
        object!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "run_now", "pause", "unpause", "delete", "kill", "inspect", "sessions", "session_content"]
                },
                "job_id": {"type": "string", "description": "Job identifier for operations on existing jobs"},
                "recipe_path": {"type": "string", "description": "Path to recipe file for create action"},
                "cron_expression": {"type": "string", "description": "A cron expression for create action. Supports both 5-field (minute hour day month weekday) and 6-field (second minute hour day month weekday) formats. 5-field expressions are automatically converted to 6-field by prepending '0' for seconds."},
                "limit": {"type": "integer", "description": "Limit for sessions list", "default": 50},
                "session_id": {"type": "string", "description": "Session identifier for session_content action"}
            }
        }),
    ).annotate(ToolAnnotations::with_title("Manage scheduled recipes".to_string()).read_only(false).destructive(true).idempotent(false).open_world(false))
}

pub const PLATFORM_A2A_CALL_TOOL_NAME: &str = "platform__a2a_call_remote_agent";

pub fn a2a_call_remote_agent_tool() -> Tool {
    Tool::new(
        PLATFORM_A2A_CALL_TOOL_NAME.to_string(),
        indoc! {r#"
            Delegate a task to a remote agent that speaks the A2A (Agent2Agent) protocol
            and return its reply.

            Provide the remote agent's base URL — its Agent Card is fetched from
            <base>/.well-known/agent-card.json — and the message to send. The call is
            blocking (no streaming) and returns the remote agent's text response.
        "#}
        .to_string(),
        object!({
            "type": "object",
            "required": ["agent_url", "message"],
            "properties": {
                "agent_url": {"type": "string", "description": "Base URL of the remote A2A agent, e.g. https://host or https://host/a2a"},
                "message": {"type": "string", "description": "The message or task to send to the remote agent"}
            }
        }),
    )
    .annotate(
        ToolAnnotations::with_title("Call a remote A2A agent".to_string())
            .read_only(false)
            .destructive(false)
            .idempotent(false)
            .open_world(true),
    )
}
