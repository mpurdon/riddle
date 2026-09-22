//! Versioned, runtime-editable prompt files (§56). The three prompts ship
//! embedded in the binary; a file of the same name under the override
//! directory replaces the embedded text at load time, like a Signal
//! definition that the user has edited. The version recorded in traces and
//! conversations is a content hash of the template, so an edited prompt is
//! distinguishable from the shipped one without a manual bump.
//!
//! Placeholders are `{{name}}`. Rendering never fails: a missing placeholder
//! value renders as an empty string, and an unreadable override falls back
//! to the embedded prompt with a warning, because a bad edit must not take
//! the Director down.

use riddle_observability::{rwarn, Category};
use std::path::{Path, PathBuf};

const ESCALATION_V1: &str = include_str!("../prompts/escalation.v1.md");
const DIRECTOR_V1: &str = include_str!("../prompts/director.v1.md");
const EXPEDITION_V1: &str = include_str!("../prompts/expedition.v1.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptName {
    Escalation,
    Director,
    Expedition,
}

impl PromptName {
    pub const ALL: [PromptName; 3] = [
        PromptName::Escalation,
        PromptName::Director,
        PromptName::Expedition,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            PromptName::Escalation => "escalation.v1.md",
            PromptName::Director => "director.v1.md",
            PromptName::Expedition => "expedition.v1.md",
        }
    }
    pub fn embedded(self) -> &'static str {
        match self {
            PromptName::Escalation => ESCALATION_V1,
            PromptName::Director => DIRECTOR_V1,
            PromptName::Expedition => EXPEDITION_V1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptSource {
    Embedded,
    Override(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub name: PromptName,
    /// The template (or the rendered text after `render`).
    pub text: String,
    /// `<file name>@<first 12 hex of sha256 of the template>`.
    pub version: String,
    pub source: PromptSource,
}

#[derive(Debug, Clone, Default)]
pub struct PromptStore {
    override_dir: Option<PathBuf>,
}

impl PromptStore {
    /// Embedded prompts only.
    pub fn embedded() -> Self {
        Self { override_dir: None }
    }

    /// Prefer `dir/<file name>` when it exists and is readable.
    pub fn with_override_dir(dir: impl AsRef<Path>) -> Self {
        Self {
            override_dir: Some(dir.as_ref().to_path_buf()),
        }
    }

    pub fn override_dir(&self) -> Option<&Path> {
        self.override_dir.as_deref()
    }

    /// The template and its version.
    pub fn load(&self, name: PromptName) -> Prompt {
        if let Some(dir) = &self.override_dir {
            let path = dir.join(name.file_name());
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(text) if !text.trim().is_empty() => {
                        return Prompt {
                            name,
                            version: version_of(name, &text),
                            text,
                            source: PromptSource::Override(path),
                        };
                    }
                    Ok(_) => rwarn!(
                        Category::Llm,
                        "prompt override {} is empty; using embedded",
                        name.file_name()
                    ),
                    Err(e) => rwarn!(
                        Category::Llm,
                        "prompt override {} unreadable ({e}); using embedded",
                        name.file_name()
                    ),
                }
            }
        }
        let text = name.embedded().to_string();
        Prompt {
            name,
            version: version_of(name, &text),
            text,
            source: PromptSource::Embedded,
        }
    }

    /// The version alone, for traces.
    pub fn prompt_version(&self, name: PromptName) -> String {
        self.load(name).version
    }

    /// Load and substitute `{{key}}` placeholders. The version is the
    /// template's, not the rendered text's.
    pub fn render(&self, name: PromptName, placeholders: &[(&str, &str)]) -> Prompt {
        let mut p = self.load(name);
        p.text = render(&p.text, placeholders);
        p
    }
}

/// Substitute `{{key}}` placeholders; unknown placeholders render empty.
pub fn render(template: &str, placeholders: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in placeholders {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    // Any placeholder without a value becomes empty rather than leaking braces.
    while let Some(start) = out.find("{{") {
        match out[start..].find("}}") {
            Some(rel) => {
                let end = start + rel + 2;
                let inner = &out[start + 2..end - 2];
                if inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    out.replace_range(start..end, "");
                } else {
                    break;
                }
            }
            None => break,
        }
    }
    out
}

fn version_of(name: PromptName, text: &str) -> String {
    let hash = riddle_core::sha256_hex(text.as_bytes());
    format!("{}@{}", name.file_name(), &hash[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_prompts_render_their_placeholders_and_carry_a_content_hash() {
        let store = PromptStore::embedded();
        let p = store.render(
            PromptName::Director,
            &[("today", "2026-09-22"), ("tools", "- search")],
        );
        assert!(p.text.contains("Today is 2026-09-22."));
        assert!(p.text.contains("- search"));
        assert!(!p.text.contains("{{"), "no placeholder leaks: {}", p.text);
        assert!(p.version.starts_with("director.v1.md@"));
        assert_eq!(p.version, store.prompt_version(PromptName::Director));
        assert_eq!(p.source, PromptSource::Embedded);
    }

    #[test]
    fn an_override_file_replaces_the_embedded_prompt_and_changes_the_version() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("escalation.v1.md"), "Custom {{x}} prompt").unwrap();
        let store = PromptStore::with_override_dir(dir.path());
        let p = store.render(PromptName::Escalation, &[("x", "escalation")]);
        assert_eq!(p.text, "Custom escalation prompt");
        assert!(matches!(p.source, PromptSource::Override(_)));
        assert_ne!(
            p.version,
            PromptStore::embedded().prompt_version(PromptName::Escalation)
        );
        // Other prompts still come from the binary.
        assert_eq!(
            store.load(PromptName::Director).source,
            PromptSource::Embedded
        );
    }

    #[test]
    fn an_empty_override_falls_back_to_the_embedded_prompt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("director.v1.md"), "   \n").unwrap();
        let store = PromptStore::with_override_dir(dir.path());
        assert_eq!(
            store.load(PromptName::Director).source,
            PromptSource::Embedded
        );
    }

    #[test]
    fn every_embedded_prompt_states_that_untrusted_content_is_data() {
        for name in PromptName::ALL {
            let text = name.embedded().to_lowercase();
            assert!(text.contains("untrusted_mail"), "{:?}", name);
            assert!(
                text.contains("not instructions") || text.contains("never instructions"),
                "{:?}",
                name
            );
        }
        assert!(PromptName::Escalation
            .embedded()
            .contains("never decide what happens to the message"));
    }
}
