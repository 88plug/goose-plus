use super::*;

/// Returns true if `key` (used with `SessionSystemPromptMode::Append`) is
/// considered client-authored and should be persisted to the session record.
///
/// Server-managed keys (`recipe`, `final_output`, `recipe_instructions`, etc.)
/// are rebuilt at agent-activation time from other session state, so they
/// must not be persisted via the client-driven path — clients that want their
/// append-mode key to survive a restart should prefix it with `client_`.
fn is_client_authored_key(key: &str) -> bool {
    key.starts_with("client_")
}

enum ClientSystemPromptOp {
    SetOverride(Option<String>),
    UpsertExtra(String, String),
    RemoveExtra(String),
}

impl GooseAcpAgent {
    pub(super) async fn on_update_working_dir(
        &self,
        req: UpdateWorkingDirRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let working_dir = req.working_dir.trim().to_string();
        if working_dir.is_empty() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("working directory cannot be empty"));
        }
        let path = std::path::PathBuf::from(&working_dir);
        validate_absolute_cwd(&path)?;
        let session_id = &req.session_id;

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        if path == session.working_dir {
            return Ok(EmptyResponse {});
        }

        self.session_manager
            .update(session_id)
            .working_dir(path)
            .apply()
            .await
            .internal_err_ctx("Failed to update session working directory")?;

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .internal_err_ctx("Failed to reload session")?;

        let agent = self.get_session_agent(session_id).await?;
        agent
            .restore_provider_from_session(&session)
            .await
            .internal_err_ctx("Failed to refresh provider from session")?;

        agent
            .extension_manager
            .update_working_dir(&session.working_dir)
            .await;

        Ok(EmptyResponse {})
    }

    pub(super) async fn on_set_session_system_prompt(
        &self,
        req: SetSessionSystemPromptRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim().to_string();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let agent = self.get_session_agent(&session_id).await?;
        let persist_op = match req.mode {
            SessionSystemPromptMode::Set => {
                if req.text.trim().is_empty() {
                    agent.clear_system_prompt_override().await;
                    Some(ClientSystemPromptOp::SetOverride(None))
                } else {
                    agent.override_system_prompt(req.text.clone()).await;
                    Some(ClientSystemPromptOp::SetOverride(Some(req.text)))
                }
            }
            SessionSystemPromptMode::Append => {
                let key = req
                    .key
                    .as_deref()
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .ok_or_else(|| {
                        agent_client_protocol::Error::invalid_params()
                            .data("key cannot be empty for append mode")
                    })?
                    .to_string();
                let clear = req.text.trim().is_empty();
                if clear {
                    agent.remove_system_prompt_extra(&key).await;
                } else {
                    agent
                        .extend_system_prompt(key.clone(), req.text.clone())
                        .await;
                }
                let op = if clear {
                    ClientSystemPromptOp::RemoveExtra(key.clone())
                } else {
                    ClientSystemPromptOp::UpsertExtra(key.clone(), req.text)
                };
                is_client_authored_key(&key).then_some(op)
            }
        };

        if let Some(op) = persist_op {
            let session = self
                .session_manager
                .get_session(&session_id, false)
                .await
                .internal_err()?;
            let mut state = session.client_system_prompt.unwrap_or_default();
            match op {
                ClientSystemPromptOp::SetOverride(value) => state.override_text = value,
                ClientSystemPromptOp::UpsertExtra(key, text) => {
                    state.extras.insert(key, text);
                }
                ClientSystemPromptOp::RemoveExtra(key) => {
                    state.extras.remove(&key);
                }
            }
            let next = (!state.is_empty()).then_some(state);
            self.session_manager
                .update(&session_id)
                .client_system_prompt(next)
                .apply()
                .await
                .internal_err()?;
        }

        Ok(EmptyResponse {})
    }

    pub(super) async fn on_delete_session(
        &self,
        req: DeleteSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .delete_session(&req.session_id)
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        let _ = self.agent_manager.remove_session(&req.session_id).await;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, agent_client_protocol::Error> {
        let data = self
            .session_manager
            .export_session(&req.session_id)
            .await
            .internal_err()?;
        Ok(ExportSessionResponse { data })
    }

    pub(super) async fn on_import_session(
        &self,
        req: ImportSessionRequest,
    ) -> Result<ImportSessionResponse, agent_client_protocol::Error> {
        let session = self
            .session_manager
            .import_session(&req.data, None)
            .await
            .internal_err()?;

        let msg_count = session.message_count as u64;

        Ok(ImportSessionResponse {
            session_id: session.id,
            title: Some(session.name),
            updated_at: Some(session.updated_at.to_rfc3339()),
            message_count: msg_count,
        })
    }

    pub(super) async fn on_get_session_info(
        &self,
        req: GetSessionInfoRequest,
    ) -> Result<GetSessionInfoResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let session = self
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        Ok(GetSessionInfoResponse {
            session: build_session_info(session),
        })
    }

    pub(super) async fn on_truncate_session_conversation(
        &self,
        req: TruncateSessionConversationRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        self.session_manager
            .truncate_conversation(session_id, req.truncate_from)
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .project_id(req.project_id)
            .apply()
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_rename_session(
        &self,
        req: RenameSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .user_provided_name(req.title)
            .apply()
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_archive_session(
        &self,
        req: ArchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .archived_at(Some(chrono::Utc::now()))
            .apply()
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        let _ = self.agent_manager.remove_session(&req.session_id).await;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .archived_at(None)
            .apply()
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_client_authored_key_accepts_client_prefixed_keys() {
        assert!(is_client_authored_key("client_persona"));
        assert!(is_client_authored_key("client_"));
    }

    #[test]
    fn is_client_authored_key_rejects_server_managed_keys() {
        assert!(!is_client_authored_key("recipe"));
        assert!(!is_client_authored_key("final_output"));
        assert!(!is_client_authored_key("recipe_instructions"));
        assert!(!is_client_authored_key(""));
    }
}
