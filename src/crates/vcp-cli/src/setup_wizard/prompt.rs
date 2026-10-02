// SPDX-License-Identifier: Apache-2.0
//! Prompt seam for guided setup. The console implementation writes prompts and
//! progress to stderr; the scripted one drives tests and records a transcript.
use crate::credential::Secret;
use std::collections::VecDeque;

pub trait Prompter {
    /// Show a message.
    fn say(&mut self, text: &str);
    /// Read one visible line, trimmed. `None` means end of input.
    fn line(&mut self, prompt: &str) -> Result<Option<String>, String>;
    /// Read one hidden line. `None` means cancelled or empty.
    fn secret(&mut self, prompt: &str) -> Result<Option<Secret>, String>;
}

pub struct ConsolePrompter;

impl Prompter for ConsolePrompter {
    fn say(&mut self, text: &str) {
        eprintln!("{text}");
    }

    fn line(&mut self, prompt: &str) -> Result<Option<String>, String> {
        use std::io::Write;
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
        let mut input = std::io::stdin().lock();
        crate::terminal::read_line(&mut input)
            .map(|line| line.map(|text| text.trim().to_owned()))
            .map_err(|error| error.to_string())
    }

    fn secret(&mut self, prompt: &str) -> Result<Option<Secret>, String> {
        #[cfg(windows)]
        return crate::console_secret::read(prompt);
        #[cfg(not(windows))]
        {
            let _ = prompt;
            Err("hidden key entry requires Windows; set OPENROUTER_API_KEY instead".into())
        }
    }
}

/// Answers in order; running out of answers is end of input.
pub struct ScriptedPrompter {
    answers: VecDeque<String>,
    pub transcript: Vec<String>,
}

impl ScriptedPrompter {
    pub fn new<I: IntoIterator<Item = S>, S: Into<String>>(answers: I) -> Self {
        Self {
            answers: answers.into_iter().map(Into::into).collect(),
            transcript: Vec::new(),
        }
    }

    pub fn text(&self) -> String {
        self.transcript.join("\n")
    }
}

impl Prompter for ScriptedPrompter {
    fn say(&mut self, text: &str) {
        self.transcript.push(text.to_owned());
    }

    fn line(&mut self, prompt: &str) -> Result<Option<String>, String> {
        self.transcript.push(prompt.to_owned());
        Ok(self
            .answers
            .pop_front()
            .map(|answer| answer.trim().to_owned()))
    }

    fn secret(&mut self, prompt: &str) -> Result<Option<Secret>, String> {
        self.transcript.push(format!("{prompt}[hidden]"));
        match self.answers.pop_front() {
            None => Ok(None),
            Some(answer) if answer.is_empty() => Ok(None),
            Some(answer) => Secret::new(answer).map(Some),
        }
    }
}
