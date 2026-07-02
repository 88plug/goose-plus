use crossterm::event::{KeyCode, KeyModifiers};

use super::acp::ClientCommand;
use super::{App, Role, View};

#[derive(Clone, Copy)]
pub(super) struct SlashCommand {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
}

pub(super) const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "/help",
        description: "show this command menu",
    },
    SlashCommand {
        name: "/extensions",
        description: "manage configured extensions",
    },
    SlashCommand {
        name: "/provider",
        description: "choose the active provider",
    },
    SlashCommand {
        name: "/model",
        description: "choose the active model",
    },
    SlashCommand {
        name: "/sessions",
        description: "view recent sessions",
    },
    SlashCommand {
        name: "/clear",
        description: "clear the current chat history",
    },
    SlashCommand {
        name: "/new",
        description: "start a new session",
    },
    SlashCommand {
        name: "/quit",
        description: "exit goose",
    },
];

pub(super) fn matching_slash_commands(input: &str) -> Vec<SlashCommand> {
    let query = input.split_whitespace().next().unwrap_or(input);
    SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|command| command.name.starts_with(query))
        .collect()
}

impl App {
    pub(super) fn handle_slash_command_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if modifiers != KeyModifiers::NONE {
            return false;
        }

        let commands = self.slash_commands();
        if commands.is_empty() {
            return false;
        }

        match code {
            KeyCode::Up => {
                self.slash_selected = self.slash_selected.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                self.slash_selected = (self.slash_selected + 1).min(commands.len() - 1);
                true
            }
            KeyCode::Enter => {
                let command = commands[self.slash_selected.min(commands.len() - 1)];
                self.input.clear();
                self.cursor = 0;
                self.slash_selected = 0;
                self.show_help_menu = false;
                self.handle_slash(command.name);
                true
            }
            _ => false,
        }
    }

    pub(super) fn slash_commands(&self) -> Vec<SlashCommand> {
        if !self.input.starts_with('/') {
            return Vec::new();
        }
        if self.input.trim() == "/help" {
            SLASH_COMMANDS.to_vec()
        } else {
            matching_slash_commands(&self.input)
        }
    }

    pub(super) fn reset_slash_selection(&mut self) {
        self.slash_selected = 0;
    }

    pub(super) fn handle_slash(&mut self, input: &str) {
        match input.split_whitespace().next().unwrap_or_default() {
            "/help" => {
                self.show_help_menu = true;
            }
            "/sessions" => {
                let _ = self.cmd_tx.send(ClientCommand::ListSessions);
            }
            "/extensions" => {
                let _ = self.cmd_tx.send(ClientCommand::ListExtensions);
            }
            "/provider" => {
                let _ = self.cmd_tx.send(ClientCommand::ListProviders);
                self.view = View::Providers;
            }
            "/model" => {
                self.ensure_models();
                self.model_search.clear();
                self.models_selected = 0;
                self.view = View::Models;
            }
            "/clear" => self.clear_chat(),
            "/new" => self.start_new_session(),
            "/quit" => self.should_quit = true,
            cmd => self.push_message(Role::System, format!("Unknown command: {cmd}. Type /help")),
        }
    }

    pub(super) fn autocomplete_slash(&mut self) {
        let matches = self.slash_commands();
        if matches.len() == 1 {
            self.input = format!("{} ", matches[0].name);
            self.cursor = self.input.len();
            self.reset_slash_selection();
        }
    }
}
