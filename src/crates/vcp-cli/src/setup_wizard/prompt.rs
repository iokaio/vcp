// SPDX-License-Identifier: Apache-2.0
use crate::credential::Secret;
use std::io::Write;

pub trait Prompter {
    fn say(&mut self, text: &str);
    fn line(&mut self, prompt: &str) -> Result<Option<String>, String>;
    fn secret(&mut self, prompt: &str) -> Result<Option<Secret>, String>;
}

pub struct Console;
impl Prompter for Console {
    fn say(&mut self, text: &str) {
        eprintln!("{text}");
    }
    fn line(&mut self, prompt: &str) -> Result<Option<String>, String> {
        eprint!("{prompt}");
        std::io::stderr()
            .flush()
            .map_err(|_| "console output unavailable")?;
        crate::terminal::read_line(&mut std::io::stdin().lock())
            .map(|line| line.map(|text| text.trim().to_owned()))
            .map_err(|_| "console input unavailable".into())
    }
    fn secret(&mut self, prompt: &str) -> Result<Option<Secret>, String> {
        crate::console_secret::read(prompt)
    }
}

pub fn answer(prompt: &mut impl Prompter, message: &str, default: &str) -> Result<String, String> {
    match prompt.line(message)? {
        None => Err("setup interrupted; run vcp setup to continue".into()),
        Some(value) if value.eq_ignore_ascii_case("cancel") => {
            Err("setup cancelled; configuration is not complete".into())
        }
        Some(value) if value.is_empty() => Ok(default.into()),
        Some(value) => Ok(value),
    }
}

pub fn yes(prompt: &mut impl Prompter, message: &str) -> Result<bool, String> {
    loop {
        match answer(prompt, message, "no")?.to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => prompt.say("Enter yes or no, or cancel to stop."),
        }
    }
}
