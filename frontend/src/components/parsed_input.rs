use crate::generated::css_classes::C;
use crate::util::simple_ev;
use seed::prelude::*;
use seed::{Attrs, C, attrs, div, empty, input};
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct ParsedInput<T> {
    text: String,
    parsed: Option<T>,
    input_kind: &'static str,
    error_message: Option<&'static str>,
    empty_message: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub enum ParsedInputMsg {
    Input(String),
    FocusIn,
    FocusOut,
}

impl<T> ParsedInput<T>
where
    T: FromStr + ToString,
{
    pub fn new() -> Self {
        Self {
            text: String::new(),
            parsed: "".parse().ok(),
            input_kind: "text",
            error_message: None,
            empty_message: None,
        }
    }

    pub fn new_with_text<S: ToString>(text: S) -> Self {
        let text = text.to_string();
        ParsedInput {
            parsed: text.parse().ok(),
            text,
            ..Self::new()
        }
    }

    pub fn new_with_value(value: T) -> Self {
        ParsedInput {
            text: value.to_string(),
            parsed: Some(value),
            ..Self::new()
        }
    }

    pub fn update(&mut self, msg: ParsedInputMsg) {
        match msg {
            ParsedInputMsg::Input(text) => {
                self.parsed = text.parse().ok();
                self.text = text;
            }
            ParsedInputMsg::FocusIn | ParsedInputMsg::FocusOut => {}
        }
    }

    pub fn set_value(&mut self, value: T) {
        self.text = value.to_string();
        self.parsed = Some(value);
    }
}

impl<T> ParsedInput<T> {
    pub fn with_error_message(self, error_message: &'static str) -> Self {
        ParsedInput {
            error_message: Some(error_message),
            ..self
        }
    }

    #[allow(dead_code)]
    pub fn with_empty_message(self, empty_message: &'static str) -> Self {
        ParsedInput {
            empty_message: Some(empty_message),
            ..self
        }
    }

    pub fn with_input_kind(self, input_kind: &'static str) -> Self {
        ParsedInput { input_kind, ..self }
    }

    pub fn view(&self, attrs: Attrs) -> Node<ParsedInputMsg> {
        div![
            C![C.parsed_input],
            input![
                attrs,
                C![C.parsed_input_text],
                attrs! { At::Value => &self.text },
                attrs! { At::Type => self.input_kind },
                input_ev(Ev::Input, ParsedInputMsg::Input),
                simple_ev(Ev::Custom("focusin".into()), ParsedInputMsg::FocusIn),
                simple_ev(Ev::Custom("focusout".into()), ParsedInputMsg::FocusOut),
            ],
            match (&self.parsed, self.error_message, self.empty_message) {
                (None, _, Some(msg)) if self.text.is_empty() => div![C![C.parsed_input_error], msg],
                (None, Some(msg), _) if !self.text.is_empty() =>
                    div![C![C.parsed_input_error], msg],
                _ => empty![],
            }
        ]
    }

    pub fn parsed(&self) -> Option<&T> {
        self.parsed.as_ref()
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}
