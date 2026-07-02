use crossterm::event::{KeyCode, KeyModifiers};

use super::acp::ClientCommand;
use super::style::{provider_columns, terminal_width};
use super::{App, Role, View};

impl App {
    pub(super) fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        match self.view {
            View::Splash => self.handle_splash_key(code, modifiers),
            View::Chat => self.handle_chat_key(code, modifiers),
            View::Providers => self.handle_provider_key(code),
            View::Models => self.handle_model_key(code),
            View::Sessions => self.handle_sessions_key(code),
            View::Extensions => self.handle_extensions_key(code),
        }
    }

    fn handle_splash_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            (KeyCode::Enter, KeyModifiers::NONE) if !self.loading => {
                let input = self.take_input();
                if input.is_empty() {
                    return;
                }
                self.view = View::Chat;
                self.push_message(Role::User, input.clone());
                self.scrollback = 0;
                self.loading = true;
                self.status = "queued".into();
                let _ = self.cmd_tx.send(ClientCommand::SendPrompt(input));
            }
            (KeyCode::Backspace, _) => self.input_backspace(),
            (KeyCode::Delete, _) => self.input_delete(),
            (KeyCode::Left, _) => self.input_left(),
            (KeyCode::Right, _) => self.input_right(),
            (KeyCode::Home, _) => self.cursor = 0,
            (KeyCode::End, _) => self.cursor = self.input.len(),
            (KeyCode::Char(c), m)
                if !m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) =>
            {
                self.input.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            _ => {}
        }
    }

    fn handle_chat_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if self.handle_slash_command_key(code, modifiers) {
            return;
        }

        if self.expanded_tool_call {
            match code {
                KeyCode::Esc | KeyCode::Char(' ') => {
                    self.expanded_tool_call = false;
                    self.expanded_scroll = 0;
                    self.selected_tool_call = None;
                }
                KeyCode::Up => self.expanded_scroll = self.expanded_scroll.saturating_add(3),
                KeyCode::Down => self.expanded_scroll = self.expanded_scroll.saturating_sub(3),
                _ => {}
            }
            return;
        }

        match (code, modifiers) {
            (KeyCode::Char(' '), KeyModifiers::NONE) if self.selected_tool_call.is_some() => {
                self.expanded_tool_call = true;
                self.expanded_scroll = 0;
            }
            (KeyCode::Up, KeyModifiers::SHIFT) => self.move_tool_selection(-1),
            (KeyCode::Down, KeyModifiers::SHIFT) => self.move_tool_selection(1),
            (KeyCode::Esc, _) if self.show_help_menu => self.show_help_menu = false,
            (KeyCode::Esc, _) if self.selected_tool_call.is_some() => {
                self.selected_tool_call = None;
                self.expanded_scroll = 0;
            }
            (KeyCode::Tab, KeyModifiers::NONE) => self.autocomplete_slash(),
            (KeyCode::Enter, KeyModifiers::NONE) => {
                let input = self.take_input();
                if input.is_empty() {
                    return;
                }
                self.show_help_menu = false;
                if input.starts_with('/') {
                    self.handle_slash(&input);
                } else {
                    self.push_message(Role::User, input.clone());
                    self.scrollback = 0;
                    self.loading = true;
                    self.status = "queued".into();
                    let _ = self.cmd_tx.send(ClientCommand::SendPrompt(input));
                }
            }
            (KeyCode::Backspace, _) => {
                self.input_backspace();
                self.reset_slash_selection();
            }
            (KeyCode::Delete, _) => {
                self.input_delete();
                self.reset_slash_selection();
            }
            (KeyCode::Left, _) => self.input_left(),
            (KeyCode::Right, _) => self.input_right(),
            (KeyCode::Home, _) => self.cursor = 0,
            (KeyCode::End, _) => self.cursor = self.input.len(),
            (KeyCode::Char(c), m)
                if !m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) =>
            {
                self.input.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                self.reset_slash_selection();
            }
            _ => {}
        }
    }

    fn handle_provider_key(&mut self, code: KeyCode) {
        let count = self.filtered_providers().len();
        match code {
            KeyCode::Esc if !self.provider_search.is_empty() => {
                self.provider_search.clear();
                self.providers_selected = 0;
            }
            KeyCode::Esc => self.view = View::Chat,
            KeyCode::Left => self.providers_selected = self.providers_selected.saturating_sub(1),
            KeyCode::Right => {
                if count > 0 {
                    self.providers_selected = (self.providers_selected + 1).min(count - 1);
                }
            }
            KeyCode::Up => {
                self.providers_selected = self
                    .providers_selected
                    .saturating_sub(provider_columns(terminal_width()))
            }
            KeyCode::Down => {
                if count > 0 {
                    self.providers_selected = (self.providers_selected
                        + provider_columns(terminal_width()))
                    .min(count - 1);
                }
            }
            KeyCode::Enter => {
                let selected = self
                    .filtered_providers()
                    .get(self.providers_selected)
                    .copied()
                    .cloned();
                if let Some(provider) = selected {
                    self.pending_provider = Some(provider.id.clone());
                    self.models.clear();
                    self.model_search.clear();
                    self.models_selected = 0;
                    self.provider_search.clear();
                    self.loading = true;
                    self.status = "loading models".into();
                    let _ = self.cmd_tx.send(ClientCommand::ListProviderModels {
                        provider: provider.id,
                    });
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {
                self.provider_search.pop();
                self.providers_selected = 0;
            }
            KeyCode::Char(c) => {
                self.provider_search.push(c);
                self.providers_selected = 0;
            }
            _ => {}
        }
    }

    fn handle_model_key(&mut self, code: KeyCode) {
        let count = self.filtered_models().len();
        match code {
            KeyCode::Esc if !self.model_search.is_empty() => {
                self.model_search.clear();
                self.models_selected = 0;
            }
            KeyCode::Esc => {
                self.pending_provider = None;
                self.view = View::Chat;
            }
            KeyCode::Up => self.models_selected = self.models_selected.saturating_sub(1),
            KeyCode::Down => {
                if count > 0 {
                    self.models_selected = (self.models_selected + 1).min(count - 1);
                }
            }
            KeyCode::Enter => {
                let model = self
                    .filtered_models()
                    .get(self.models_selected)
                    .cloned()
                    .cloned();
                if let Some(model) = model {
                    let provider = self.pending_provider.take().or_else(|| {
                        self.providers
                            .iter()
                            .find(|p| p.models.contains(&model))
                            .map(|p| p.id.clone())
                    });
                    if let Some(provider) = provider {
                        let _ = self
                            .cmd_tx
                            .send(ClientCommand::SaveDefaults { provider, model });
                        self.model_search.clear();
                        self.loading = true;
                        self.status = "changing model".into();
                        self.view = View::Chat;
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {
                self.model_search.pop();
                self.models_selected = 0;
            }
            KeyCode::Char(c) => {
                self.model_search.push(c);
                self.models_selected = 0;
            }
            _ => {}
        }
    }

    fn handle_sessions_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.view = View::Chat,
            KeyCode::Char('n') | KeyCode::Enter => self.start_new_session(),
            KeyCode::Up => self.sessions_selected = self.sessions_selected.saturating_sub(1),
            KeyCode::Down => {
                if !self.sessions.is_empty() {
                    self.sessions_selected =
                        (self.sessions_selected + 1).min(self.sessions.len() - 1);
                }
            }
            _ => {}
        }
    }

    fn handle_extensions_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.view = View::Chat,
            KeyCode::Up => self.extensions_selected = self.extensions_selected.saturating_sub(1),
            KeyCode::Down => {
                if !self.extensions.is_empty() {
                    self.extensions_selected =
                        (self.extensions_selected + 1).min(self.extensions.len() - 1);
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(ext) = self.extensions.get_mut(self.extensions_selected) {
                    ext.enabled = !ext.enabled;
                    let _ = self.cmd_tx.send(ClientCommand::ToggleExtension {
                        key: ext.name.clone(),
                        enabled: ext.enabled,
                    });
                }
            }
            _ => {}
        }
    }

    fn take_input(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.input).trim().to_string()
    }

    #[allow(clippy::string_slice)]
    fn input_backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.input[..self.cursor]
            .chars()
            .last()
            .map(char::len_utf8)
            .unwrap_or(0);
        self.cursor -= prev;
        self.input.remove(self.cursor);
    }

    fn input_delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
        }
    }

    #[allow(clippy::string_slice)]
    fn input_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= self.input[..self.cursor]
                .chars()
                .last()
                .map(char::len_utf8)
                .unwrap_or(0);
        }
    }

    #[allow(clippy::string_slice)]
    fn input_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += self.input[self.cursor..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(0);
        }
    }
}
