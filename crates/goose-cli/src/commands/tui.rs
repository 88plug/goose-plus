use anyhow::Result;

pub async fn handle_tui(_args: Vec<String>) -> Result<()> {
    crate::tui::run_tui().await
}
