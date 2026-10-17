// The start screen: play offline, host an online game, or join one by code.

use macroquad::prelude::*;
use protocol::relay::CODE_LEN;

use crate::draw::center_text;

pub enum Choice {
    Offline,
    Host,
    Join(String),
}

#[derive(Default)]
pub struct Menu {
    /// `Some` while the player is typing a room code.
    typing: Option<String>,
}

impl Menu {
    pub fn new() -> Menu {
        // Throw away letters typed on the previous screen so they do not
        // leak into the code field.
        while get_char_pressed().is_some() {}
        Menu::default()
    }

    pub fn update(&mut self) -> Option<Choice> {
        if let Some(code) = &mut self.typing {
            while let Some(c) = get_char_pressed() {
                if c.is_ascii_alphanumeric() && code.len() < CODE_LEN {
                    code.push(c.to_ascii_uppercase());
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                code.pop();
            }
            if is_key_pressed(KeyCode::Escape) {
                self.typing = None;
            } else if is_key_pressed(KeyCode::Enter) && code.len() == CODE_LEN {
                return Some(Choice::Join(code.clone()));
            }
            return None;
        }

        if is_key_pressed(KeyCode::P) {
            return Some(Choice::Offline);
        }
        if is_key_pressed(KeyCode::H) {
            return Some(Choice::Host);
        }
        if is_key_pressed(KeyCode::J) {
            while get_char_pressed().is_some() {} // the "j" itself
            self.typing = Some(String::new());
        }
        None
    }

    pub fn draw(&self) {
        center_text("NEON BOMBER", 200.0, 90.0, Color::new(0.1, 1.0, 0.95, 1.0));
        match &self.typing {
            None => {
                center_text("P   play offline against bots", 330.0, 34.0, WHITE);
                center_text("H   host an online game", 385.0, 34.0, WHITE);
                center_text("J   join an online game", 440.0, 34.0, WHITE);
            }
            Some(code) => {
                center_text("type the room code", 330.0, 34.0, GRAY);
                let shown = format!("{:_<width$}", code, width = CODE_LEN);
                center_text(&shown, 410.0, 80.0, WHITE);
                center_text("Enter: join     Esc: back", 470.0, 26.0, GRAY);
            }
        }
    }
}
