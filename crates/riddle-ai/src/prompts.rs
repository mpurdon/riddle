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

use riddle_core::UiLevel;
use riddle_observability::{rwarn, Category};
use std::path::{Path, PathBuf};

const ESCALATION_V1: &str = include_str!("../prompts/escalation.v1.md");
const DIRECTOR_V1: &str = include_str!("../prompts/director.v1.md");
const EXPEDITION_V1: &str = include_str!("../prompts/expedition.v1.md");

/// The Director's register for a person at Essentials: plain words and the
/// narrowest tool. It answered a question about a newsletter with Signal
/// versions and route syntax to someone whose rail showed no Signals page
/// (17 Oct 2026). Rendered into `{{register}}` of the Director prompt.
const ESSENTIALS_REGISTER: &str = "The person uses Riddle at the Essentials level: they see their Finds, their mail, what was set aside and settings, and none of the rule machinery. Speak accordingly. Say \"rule\" where you would say Mesh, Signal, route or guard, and never name a Signal, a version or a Mesh's text unless they ask to see it. When a rule is needed, look for a pack first: propose `install_pack` for a pack that covers the kind of mail before writing a Mesh by hand, and write one only when no pack fits, describing it in one sentence as what it will do to which mail. Never propose a Signal (`propose_signal`, `test_signal`): when no existing Signal expresses a distinction, say what Riddle cannot yet tell apart and stop there. Keep answers to a few sentences: lead with what matters, leave out counts, versions and identifiers unless asked, and still show the numbers a simulation reports before any rule change, in plain words.";

/// The register at Everything and Developer: the vocabulary of the rest of
/// this prompt, used as it stands.
const FULL_REGISTER: &str = "The person uses Riddle at the Everything level or above and knows Meshes, Signals, Strata and packs by name; use them, with versions and numbers where they help.";

/// What the Director is told about who it is talking to, by level. The
/// level is the person's setting; `Developer` implies `Everything`.
pub fn director_register(level: UiLevel) -> &'static str {
    if level.shows(UiLevel::Everything) {
        FULL_REGISTER
    } else {
        ESSENTIALS_REGISTER
    }
}

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
    fn the_director_prompt_at_essentials_says_rule_and_never_proposes_a_signal() {
        let store = PromptStore::embedded();
        let plain = store.render(
            PromptName::Director,
            &[("register", director_register(UiLevel::Essentials))],
        );
        assert!(plain.text.contains("Essentials level"), "{}", plain.text);
        assert!(plain.text.contains("Never propose a Signal"));
        assert!(plain.text.contains("propose `install_pack`"));
        assert!(!plain.text.contains("Everything level or above"));
        for level in [UiLevel::Everything, UiLevel::Developer] {
            let full = store.render(
                PromptName::Director,
                &[("register", director_register(level))],
            );
            assert!(full.text.contains("Everything level or above"), "{level:?}");
            assert!(!full.text.contains("Essentials level"), "{level:?}");
            assert!(!full.text.contains("Never propose a Signal"), "{level:?}");
        }
        // The register is a section of the template, so an edited prompt keeps it.
        assert!(PromptName::Director.embedded().contains("{{register}}"));
        // The version is the template's: the register changes the text, not the hash.
        assert_eq!(plain.version, store.prompt_version(PromptName::Director));
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
