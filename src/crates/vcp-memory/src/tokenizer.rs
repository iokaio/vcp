// SPDX-License-Identifier: Apache-2.0
//! Versioned deterministic code terms; no English stemming or stopword loss.
use std::collections::BTreeSet;

pub const TOKENIZER_VERSION: &str = "vcp-code-and-prose/1";

pub fn prose_terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Full spellings and lowercase aliases coexist with underscore, camel-case,
/// acronym, numeric and qualified-name components. Exact fields remain separate.
pub fn code_terms(text: &str) -> Vec<String> {
    let mut terms = BTreeSet::new();
    for token in text.split(|c: char| !c.is_alphanumeric() && !"_./\\:-".contains(c)) {
        let token = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if token.is_empty() || !token.chars().any(char::is_alphanumeric) {
            continue;
        }
        add(&mut terms, token);
        for qualified in token
            .split(['.', '/', '\\', ':', '-'])
            .filter(|part| !part.is_empty())
        {
            add(&mut terms, qualified);
            for part in qualified.split('_').filter(|part| !part.is_empty()) {
                add(&mut terms, part);
                let chars: Vec<_> = part.char_indices().collect();
                let mut start = 0;
                for index in 1..chars.len() {
                    let previous = chars[index - 1].1;
                    let current = chars[index].1;
                    let following = chars.get(index + 1).map(|(_, c)| *c);
                    let boundary = (previous.is_lowercase() && current.is_uppercase())
                        || (previous.is_uppercase()
                            && current.is_uppercase()
                            && following.is_some_and(char::is_lowercase))
                        || (previous.is_numeric() != current.is_numeric());
                    if boundary {
                        add(&mut terms, &part[start..chars[index].0]);
                        start = chars[index].0;
                    }
                }
                add(&mut terms, &part[start..]);
            }
        }
    }
    terms.into_iter().collect()
}

fn add(terms: &mut BTreeSet<String>, term: &str) {
    if term.chars().any(char::is_alphanumeric) {
        terms.insert(term.to_owned());
        terms.insert(term.to_lowercase());
    }
}
