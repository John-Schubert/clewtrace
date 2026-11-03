//! Go to definition, find references, and completion over a resolved model (task 2.11).
//!
//! The editor asks where a name leads, what cites an element, and which names fit at the
//! cursor. The answers come from the model the core already built, so the editor needs no
//! second parser (CT-MOD-004). Each call reads the model and stops at a limit, so it stays
//! within the 100 ms response budget on a large project (CT-EFF-005).

use crate::model::{ElementId, ElementKind, Model, Reference, quote};
use crate::parse::ast::{Kind, UsageKind, Visibility};

// @implements CT-MOD-004, CT-EFF-005
/// One name the editor can offer at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// The name as the editor lists it, without quotes.
    pub label: String,
    /// The text to insert, quoted when the name is not a plain identifier.
    pub insert: String,
    /// What the name stands for, such as `part def` or `requirement`.
    pub detail: String,
}

// @implements CT-MOD-004, CT-EFF-005
impl Model {
    /// The element that the name at `line` and `column` of `path` leads to. A name that
    /// cites an element leads to it; a name in a declaration, spelled `word`, leads to the
    /// element declared on that line.
    pub fn definition_at(
        &self,
        path: &str,
        line: u32,
        column: u32,
        word: &str,
    ) -> Option<ElementId> {
        let file = self.files.iter().position(|f| f == path)?;
        let cited = self.references.iter().find(|r| {
            let pos = r.name.pos;
            let width = r
                .name
                .segments
                .iter()
                .map(|s| quote(s).chars().count() + 2)
                .sum::<usize>()
                - 2;
            r.file == file
                && pos.line == line
                && (pos.column..pos.column + width as u32).contains(&column)
        });
        if let Some(target) = cited.and_then(|r| r.target) {
            return Some(target);
        }
        if word.is_empty() {
            return None;
        }
        let declared = (self.elements.iter().enumerate())
            .find(|(_, e)| e.file == file && e.pos.line == line && e.is_named(word));
        declared.map(|(i, _)| ElementId(i))
    }

    /// Every cited name that resolves to `id`, in file and line order.
    pub fn references_to(&self, id: ElementId) -> Vec<&Reference> {
        let mut found: Vec<&Reference> = self
            .references
            .iter()
            .filter(|r| r.target == Some(id))
            .collect();
        found.sort_by_key(|r| (&self.files[r.file], r.name.pos));
        found
    }

    /// Up to `limit` names that fit at the cursor in `path` on `line`. `before` is the text
    /// of the line before the word being typed and `prefix` is the word so far. After
    /// `satisfy` or `verify` the names are requirements, after `::` the members of what it
    /// follows, and anywhere else the members of the namespaces around the cursor.
    pub fn complete(
        &self,
        path: &str,
        line: u32,
        before: &str,
        prefix: &str,
        limit: usize,
    ) -> Vec<Completion> {
        let mut found = Vec::new();
        let mut add = |label: &str, id: ElementId| {
            if found.len() < limit && !found.iter().any(|c: &Completion| c.label == label) {
                found.push(Completion {
                    label: label.to_string(),
                    insert: quote(label),
                    detail: self.element(id).kind.label().to_string(),
                });
            }
        };
        let fits = |label: &str| starts_with_ignoring_case(label, prefix);
        if wants_requirement(before) {
            for (i, e) in self.elements.iter().enumerate() {
                let is_requirement = e.kind == ElementKind::Usage(UsageKind::Of(Kind::Requirement));
                let label = e.short_name.as_deref().or(e.name.as_deref());
                if let Some(label) = label.filter(|l| is_requirement && fits(l)) {
                    add(label, ElementId(i));
                }
            }
        } else if let Some(owner) = before
            .strip_suffix("::")
            .and_then(|b| self.find(qualifier(b)))
        {
            for &m in &self.element(owner).members {
                let e = self.element(m);
                for key in [&e.name, &e.short_name].into_iter().flatten() {
                    if e.visibility == Visibility::Public && fits(key) {
                        add(key, m);
                    }
                }
            }
        } else {
            let mut at = self.innermost_at(path, line);
            while let Some(ns) = at {
                for &m in &self.element(ns).members {
                    let e = self.element(m);
                    for key in [&e.name, &e.short_name].into_iter().flatten() {
                        if fits(key) {
                            add(key, m);
                        }
                    }
                }
                at = self.element(ns).owner;
            }
        }
        found
    }

    /// The last element declared in `path` at or before `line`: where the cursor is, or the
    /// element just before it. Its owners are the namespaces around the cursor.
    fn innermost_at(&self, path: &str, line: u32) -> Option<ElementId> {
        let file = self.files.iter().position(|f| f == path)?;
        let last = (self.elements.iter().enumerate()).rfind(|(_, e)| {
            e.file == file
                && e.pos.line <= line
                && matches!(
                    e.kind,
                    ElementKind::Package | ElementKind::Definition(_) | ElementKind::Usage(_)
                )
        });
        Some(last.map_or(Model::ROOT, |(i, _)| ElementId(i)))
    }
}

// @implements CT-MOD-004
/// Whether the cursor follows `satisfy` or `verify`, with or without `requirement`.
fn wants_requirement(before: &str) -> bool {
    if !before.ends_with(char::is_whitespace) {
        return false;
    }
    let (word, rest) = last_word(before.trim_end());
    let word = if word == "requirement" {
        last_word(rest.trim_end()).0
    } else {
        word
    };
    word == "satisfy" || word == "verify"
}

// @implements CT-MOD-004
/// The identifier that ends `text`, and the text before it.
fn last_word(text: &str) -> (&str, &str) {
    let start = (text.char_indices().rev())
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map_or(text.len(), |(i, _)| i);
    (&text[start..], &text[..start])
}

// @implements CT-MOD-004
/// The qualified name that ends `text`, such as `P::Q` in `part x : P::Q`.
fn qualifier(text: &str) -> &str {
    let start = text.rfind(|c: char| c.is_whitespace() || "(<[,=;{:>".contains(c) && c != ':');
    let from = start.map_or(0, |i| {
        i + text[i..].chars().next().map_or(1, char::len_utf8)
    });
    &text[from..]
}

// @implements CT-MOD-004
fn starts_with_ignoring_case(label: &str, prefix: &str) -> bool {
    label.to_lowercase().starts_with(&prefix.to_lowercase())
}

// @verifies CT-MOD-004, CT-EFF-005
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::Member;
    use crate::parse::parser::parse;

    fn model(files: &[(&str, &str)]) -> Model {
        let parsed: Vec<_> = files.iter().map(|(_, t)| parse(t)).collect();
        let inputs: Vec<(&str, &[Member])> = (files.iter().zip(&parsed))
            .map(|((p, _), parsed)| (*p, parsed.members.as_slice()))
            .collect();
        Model::build(&inputs)
    }

    const TEXT: &str = "package P {\n  part def Engine;\n  part def <'E2'> Motor :> Engine;\n  \
        requirement <'R1'> power;\n  requirement <'R2'> torque;\n  part car {\n    part e : Engine;\n    \
        satisfy 'R1' by e;\n    verify 'R2';\n  }\n}\n";

    // @verifies CT-MOD-004
    #[test]
    fn a_type_name_leads_to_its_definition() {
        let m = model(&[("m.sysml", TEXT)]);
        let id = m.definition_at("m.sysml", 7, 16, "Engine").unwrap();
        assert_eq!(m.qualified_name(id), "P::Engine");
        let id = m.definition_at("m.sysml", 3, 33, "Engine").unwrap();
        assert_eq!(m.qualified_name(id), "P::Engine");
    }

    // @verifies CT-MOD-004
    #[test]
    fn a_quoted_requirement_name_leads_to_the_requirement() {
        let m = model(&[("m.sysml", TEXT)]);
        let id = m.definition_at("m.sysml", 8, 14, "R1").unwrap();
        assert_eq!(m.qualified_name(id), "P::power");
        let id = m.definition_at("m.sysml", 9, 13, "R2").unwrap();
        assert_eq!(m.qualified_name(id), "P::torque");
    }

    // @verifies CT-MOD-004
    #[test]
    fn a_declared_name_leads_to_its_own_element() {
        let m = model(&[("m.sysml", TEXT)]);
        let id = m.definition_at("m.sysml", 3, 20, "Motor").unwrap();
        assert_eq!(m.qualified_name(id), "P::Motor");
        let id = m.definition_at("m.sysml", 4, 17, "R1").unwrap();
        assert_eq!(m.qualified_name(id), "P::power");
    }

    // @verifies CT-MOD-004
    #[test]
    fn nothing_leads_from_a_keyword_a_blank_or_an_unresolved_name() {
        let m = model(&[("m.sysml", "package P {\n  part x : Missing;\n}\n")]);
        assert_eq!(m.definition_at("m.sysml", 2, 4, "part"), None);
        assert_eq!(m.definition_at("m.sysml", 2, 14, "Missing"), None);
        assert_eq!(m.definition_at("m.sysml", 9, 1, ""), None);
        assert_eq!(m.definition_at("other.sysml", 2, 14, "Missing"), None);
    }

    // @verifies CT-MOD-004
    #[test]
    fn a_name_in_another_file_leads_to_that_file() {
        let m = model(&[
            ("a.sysml", "package A { part def Wheel; }\n"),
            (
                "b.sysml",
                "package B {\n  private import A::*;\n  part w : Wheel;\n}\n",
            ),
        ]);
        let id = m.definition_at("b.sysml", 3, 12, "Wheel").unwrap();
        assert_eq!(m.files[m.element(id).file], "a.sysml");
    }

    // @verifies CT-MOD-004
    #[test]
    fn references_list_every_citing_name_in_file_and_line_order() {
        let m = model(&[
            ("a.sysml", "package A { part def Wheel; }\n"),
            (
                "b.sysml",
                "package B {\n  private import A::*;\n  part w : Wheel;\n  part v : Wheel;\n}\n",
            ),
        ]);
        let wheel = m.find("A::Wheel").unwrap();
        let places: Vec<_> = (m.references_to(wheel).iter())
            .map(|r| (m.files[r.file].clone(), r.name.pos.line))
            .collect();
        assert_eq!(
            places,
            [("b.sysml".to_string(), 3), ("b.sysml".to_string(), 4)]
        );
        let m = model(&[("m.sysml", TEXT)]);
        let r1 = m.find("P::R1").unwrap();
        let roles: Vec<_> = m.references_to(r1).iter().map(|r| r.role).collect();
        assert_eq!(roles, [crate::model::Role::Requirement]);
    }

    fn labels(found: &[Completion]) -> Vec<&str> {
        found.iter().map(|c| c.label.as_str()).collect()
    }

    // @verifies CT-MOD-004
    #[test]
    fn requirement_names_complete_after_satisfy_and_verify_only() {
        let m = model(&[("m.sysml", TEXT)]);
        for before in ["    satisfy ", "    verify ", "    satisfy requirement "] {
            let found = m.complete("m.sysml", 9, before, "", 50);
            assert_eq!(labels(&found), ["R1", "R2"], "{before}");
            assert_eq!(found[0].insert, "R1");
            assert_eq!(found[0].detail, "requirement");
        }
        assert_eq!(
            labels(&m.complete("m.sysml", 9, "    satisfy ", "R2", 50)),
            ["R2"]
        );
        let elsewhere = m.complete("m.sysml", 9, "    part e : ", "", 50);
        assert!(
            labels(&elsewhere).contains(&"Engine"),
            "other places list every name"
        );
    }

    // @verifies CT-MOD-004
    #[test]
    fn requirement_ids_with_hyphens_insert_quoted() {
        let m = model(&[(
            "m.sysml",
            "requirement <'CT-X-001'> a;\npart p { verify \n}\n",
        )]);
        let found = m.complete("m.sysml", 2, "part p { verify ", "CT", 50);
        assert_eq!(found[0].label, "CT-X-001");
        assert_eq!(found[0].insert, "'CT-X-001'");
    }

    // @verifies CT-MOD-004
    #[test]
    fn names_in_scope_complete_from_the_enclosing_namespaces() {
        let m = model(&[
            ("a.sysml", "package Other { part def Hidden; }\n"),
            ("m.sysml", TEXT),
        ]);
        let found = m.complete("m.sysml", 7, "    part e : ", "En", 50);
        assert_eq!(labels(&found), ["Engine"]);
        assert_eq!(found[0].detail, "part def");
        let found = m.complete("m.sysml", 7, "    part e : ", "", 50);
        let names = labels(&found);
        assert!(names.contains(&"Engine") && names.contains(&"P") && names.contains(&"Other"));
        assert!(!names.contains(&"Hidden"));
    }

    // @verifies CT-MOD-004
    #[test]
    fn members_complete_after_a_qualified_prefix() {
        let m = model(&[("m.sysml", TEXT)]);
        let found = m.complete("m.sysml", 7, "    part e : P::", "E", 50);
        assert_eq!(labels(&found), ["Engine", "E2"]);
    }

    // @verifies CT-MOD-004, CT-EFF-005
    #[test]
    fn completion_stops_at_the_limit() {
        let mut text = String::from("package P {\n");
        for i in 0..500 {
            text.push_str(&format!("  part def D{i};\n"));
        }
        text.push_str("}\n");
        let m = model(&[("m.sysml", &text)]);
        assert_eq!(
            m.complete("m.sysml", 300, "  part x : ", "D", 200).len(),
            200
        );
    }
}
