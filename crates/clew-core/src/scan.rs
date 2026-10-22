//! Tag scanner (v0). Finds trace tags in source files and works out which lines they cover.
//! Rules: docs/TRACEABILITY.md. Rust and TypeScript item spans come from tree-sitter.
//! Svelte files use `@begin`/`@end` regions only in v0, and their bare `<script>` and
//! `<style>` wrapper lines are exempt.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use tree_sitter::{Node, Parser};

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    Rust,
    TypeScript,
    Svelte,
    /// Config, CI, and script files with `#` comments, traced by one `@file` tag.
    HashComments,
    /// A GitHub Actions workflow in `.github/workflows`: like `HashComments`, and a step may
    /// also carry an `@verifies` tag in the comment directly above its `- name:` line.
    Workflow,
}

// @implements CT-DEV-002, CT-TRC-006
impl Syntax {
    pub fn from_path(path: &Path) -> Option<Syntax> {
        let in_workflows = path
            .parent()
            .is_some_and(|p| p.ends_with(".github/workflows"));
        match path.extension()?.to_str()? {
            "rs" => Some(Syntax::Rust),
            "ts" => Some(Syntax::TypeScript),
            "svelte" => Some(Syntax::Svelte),
            "yml" | "yaml" if in_workflows => Some(Syntax::Workflow),
            "toml" | "yml" | "yaml" | "sh" | "ps1" => Some(Syntax::HashComments),
            _ => None,
        }
    }
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    Implements,
    Verifies,
    Begin,
    End,
    File,
}

// @implements CT-DEV-002, CT-TRC-006
impl TagKind {
    const ALL: [TagKind; 5] = [
        TagKind::Implements,
        TagKind::Verifies,
        TagKind::Begin,
        TagKind::End,
        TagKind::File,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            TagKind::Implements => "@implements",
            TagKind::Verifies => "@verifies",
            TagKind::Begin => "@begin",
            TagKind::End => "@end",
            TagKind::File => "@file",
        }
    }
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub kind: TagKind,
    pub ids: Vec<String>,
    pub line: usize,
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Item,
    Region,
    File,
    /// An import, `use`, or `mod` line carrying the union of the file's item tags.
    Import,
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub kind: ScopeKind,
    pub implements: Vec<String>,
    pub verifies: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    Exempt,
    Traced,
    Untraced,
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemKind {
    OrphanTag(TagKind),
    EmptyTag(TagKind),
    TagNotAllowed(TagKind),
    NestedBegin,
    UnclosedBegin,
    EndWithoutBegin,
    MissingFileTag,
    SyntaxError,
    /// A workflow `@verifies` tag that is not in the comment directly above a step's
    /// `- name:` line.
    NotAboveStep,
    /// A workflow step tagged `@verifies` whose failure does not fail the build, because
    /// the step or its job sets `continue-on-error` on the given 1-based line.
    ContinueOnError(usize),
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub kind: ProblemKind,
}

// @implements CT-DEV-002, CT-TRC-006
impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: ", self.line)?;
        match self.kind {
            ProblemKind::OrphanTag(kind) => {
                write!(f, "`{}` is not directly above an item", kind.keyword())
            }
            ProblemKind::EmptyTag(kind) => write!(f, "`{}` names no requirement", kind.keyword()),
            ProblemKind::TagNotAllowed(kind) => {
                write!(f, "`{}` is not allowed here", kind.keyword())
            }
            ProblemKind::NestedBegin => write!(f, "`@begin` inside an open region"),
            ProblemKind::UnclosedBegin => write!(f, "`@begin` without a matching `@end`"),
            ProblemKind::EndWithoutBegin => write!(f, "`@end` without a matching `@begin`"),
            ProblemKind::MissingFileTag => write!(f, "first comment has no `@file` tag"),
            ProblemKind::SyntaxError => write!(f, "syntax error, item spans may be wrong"),
            ProblemKind::NotAboveStep => {
                write!(
                    f,
                    "`@verifies` is not directly above a workflow step's `- name:` line"
                )
            }
            ProblemKind::ContinueOnError(line) => write!(
                f,
                "`continue-on-error` on line {line} lets this step fail without failing the build"
            ),
        }
    }
}

// @implements CT-DEV-002, CT-TRC-006
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileScan {
    pub tags: Vec<Tag>,
    pub scopes: Vec<Scope>,
    /// Status of each line; index 0 is line 1.
    pub lines: Vec<LineStatus>,
    pub problems: Vec<Problem>,
}

// @implements CT-DEV-002, CT-TRC-006
pub fn scan_source(syntax: Syntax, text: &str) -> FileScan {
    let mut scan = Scanner::new(text);
    match syntax {
        Syntax::Rust | Syntax::TypeScript => scan.tree(syntax, text),
        Syntax::Svelte => scan.svelte(text),
        Syntax::HashComments => scan.hash_comments(text, false),
        Syntax::Workflow => scan.hash_comments(text, true),
    }
    scan.finish()
}

/// Reads the tag at the start of one comment line, after its comment marker.
// @implements CT-TRC-006
fn tag_in_comment_line(line: &str) -> Option<(TagKind, Vec<String>)> {
    let mut body = line.trim_start();
    for marker in ["<!--", "///", "//!", "//", "/**", "/*", "#", "*"] {
        if let Some(rest) = body.strip_prefix(marker) {
            body = rest.trim_start();
            break;
        }
    }
    let kind = TagKind::ALL.into_iter().find(|k| {
        body.strip_prefix(k.keyword())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    })?;
    let mut rest = body[kind.keyword().len()..].trim_start();
    let mut ids = Vec::new();
    if kind != TagKind::End {
        loop {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(rest.len());
            if end == 0 {
                break;
            }
            ids.push(rest[..end].to_string());
            rest = rest[end..].trim_start();
            match rest.strip_prefix(',') {
                Some(next) => rest = next.trim_start(),
                None => break,
            }
        }
    }
    Some((kind, ids))
}

/// Zero-based last row a node occupies, not counting a trailing newline.
// @implements CT-DEV-002
fn last_row(node: Node) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

// @implements CT-DEV-002, CT-TRC-006
fn is_comment(kind: &str) -> bool {
    matches!(kind, "line_comment" | "block_comment" | "comment")
}

/// Whether a node can take an `@implements` or `@verifies` tag, and whether only `@verifies`.
// @implements CT-DEV-002, CT-TRC-006
fn taggable(syntax: Syntax, node: Node) -> Option<bool> {
    let kind = node.kind();
    let item = match syntax {
        Syntax::Rust => {
            matches!(
                kind,
                "function_item"
                    | "function_signature_item"
                    | "struct_item"
                    | "enum_item"
                    | "union_item"
                    | "trait_item"
                    | "impl_item"
                    | "const_item"
                    | "static_item"
                    | "type_item"
                    | "associated_type"
                    | "macro_definition"
                    | "foreign_mod_item"
            ) || (kind == "mod_item" && node.child_by_field_name("body").is_some())
        }
        _ => matches!(
            kind,
            "function_declaration"
                | "generator_function_declaration"
                | "class_declaration"
                | "abstract_class_declaration"
                | "method_definition"
                | "export_statement"
        ),
    };
    if item {
        return Some(false);
    }
    let test_block = syntax == Syntax::TypeScript
        && kind == "expression_statement"
        && node
            .named_child(0)
            .is_some_and(|c| c.kind() == "call_expression");
    test_block.then_some(true)
}

// @implements CT-DEV-002
fn is_import(syntax: Syntax, node: Node) -> bool {
    match syntax {
        Syntax::Rust => {
            matches!(node.kind(), "use_declaration" | "extern_crate_declaration")
                || (node.kind() == "mod_item" && node.child_by_field_name("body").is_none())
        }
        _ => node.kind() == "import_statement",
    }
}

// @implements CT-DEV-002, CT-TRC-006
fn is_attribute(kind: &str) -> bool {
    matches!(kind, "attribute_item" | "decorator")
}

/// Whether a Svelte line is only a `<script>`, `</script>`, `<style>`, or `</style>` tag.
/// These wrapper lines are exempt like blank lines.
// @implements CT-DEV-002
fn is_svelte_wrapper(line: &str) -> bool {
    let line = line.trim();
    let Some(inner) = line.strip_prefix('<').and_then(|l| l.strip_suffix('>')) else {
        return false;
    };
    if inner.contains(['<', '>']) {
        return false;
    }
    let name = inner.strip_prefix('/').unwrap_or(inner);
    ["script", "style"].into_iter().any(|tag| {
        name.strip_prefix(tag).is_some_and(|rest| {
            rest.is_empty() || (!inner.starts_with('/') && rest.starts_with(char::is_whitespace))
        })
    })
}

/// The number of spaces before a line's first other character.
// @implements CT-DEV-003, CT-TRC-006
fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Whether a YAML line is blank or holds only a comment.
// @implements CT-DEV-003, CT-TRC-006
fn blank_or_comment(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// The last zero-based row of the YAML block that starts on `row`: the rows below it that
/// are indented further, up to the last one holding more than a comment.
// @implements CT-DEV-003, CT-TRC-006
fn block_end(lines: &[&str], row: usize) -> usize {
    let base = indent(lines[row]);
    let mut end = row;
    for (r, line) in lines.iter().enumerate().skip(row + 1) {
        if blank_or_comment(line) {
            continue;
        }
        if indent(line) <= base {
            break;
        }
        end = r;
    }
    end
}

/// The row in `first..=last` of a `continue-on-error` key at `key_indent` whose value is
/// anything but `false`.
// @implements CT-DEV-003, CT-TRC-006
fn continue_on_error(
    lines: &[&str],
    first: usize,
    last: usize,
    key_indent: usize,
) -> Option<usize> {
    (first..=last).find(|&r| {
        let line = lines[r];
        let value = line.trim_start().strip_prefix("continue-on-error:");
        let value = value.map(|v| v.split(" #").next().unwrap_or_default().trim());
        indent(line) == key_indent && value.is_some_and(|v| v != "false")
    })
}

/// The row of a `continue-on-error` key, set to anything but `false`, on the job that holds
/// the step whose `- name:` line is on `row`. `None` when there is none, or when the step is
/// not in a `steps:` list.
// @implements CT-DEV-003, CT-TRC-006
fn job_continue_on_error(lines: &[&str], row: usize) -> Option<usize> {
    let dash = indent(lines[row]);
    let steps = (0..row).rev().find(|&r| {
        !blank_or_comment(lines[r]) && (indent(lines[r]) < dash || lines[r].trim() == "steps:")
    })?;
    if lines[steps].trim() != "steps:" {
        return None;
    }
    let keys = indent(lines[steps]);
    let job = (0..steps)
        .rev()
        .find(|&r| !blank_or_comment(lines[r]) && indent(lines[r]) < keys)?;
    continue_on_error(lines, job + 1, block_end(lines, job), keys)
}

/// Working state for one file: tags, scopes, and which lines hold code.
// @implements CT-DEV-002, CT-TRC-006
struct Scanner {
    tags: Vec<Tag>,
    scopes: Vec<Scope>,
    has_code: Vec<bool>,
    problems: Vec<Problem>,
    /// Zero-based row spans of imports that inherit the file's item tags.
    imports: Vec<(usize, usize)>,
}

// @implements CT-DEV-002, CT-TRC-006
impl Scanner {
    fn new(text: &str) -> Scanner {
        Scanner {
            tags: Vec::new(),
            scopes: Vec::new(),
            has_code: vec![false; text.lines().count()],
            problems: Vec::new(),
            imports: Vec::new(),
        }
    }

    fn problem(&mut self, line: usize, kind: ProblemKind) {
        self.problems.push(Problem { line, kind });
    }

    fn mark_code(&mut self, first_row: usize, last_row: usize) {
        for row in first_row..=last_row {
            if let Some(flag) = self.has_code.get_mut(row) {
                *flag = true;
            }
        }
    }

    /// Records the tags in a comment's text, which starts on the given zero-based row.
    fn read_comment(&mut self, text: &str, first_row: usize) -> Vec<usize> {
        let mut found = Vec::new();
        for (offset, line) in text.lines().enumerate() {
            if let Some((kind, ids)) = tag_in_comment_line(line) {
                found.push(self.tags.len());
                self.tags.push(Tag {
                    kind,
                    ids,
                    line: first_row + offset + 1,
                });
            }
        }
        found
    }

    fn tree(&mut self, syntax: Syntax, text: &str) {
        let language = match syntax {
            Syntax::Rust => tree_sitter_rust::LANGUAGE,
            _ => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        };
        let mut parser = Parser::new();
        parser
            .set_language(&language.into())
            .expect("bundled grammar matches the tree-sitter version");
        let tree = parser
            .parse(text, None)
            .expect("parsing without a timeout or cancel flag always returns a tree");

        // Tag indices of each comment, keyed by the comment's start byte.
        let mut comment_tags: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut items = Vec::new();
        let mut syntax_error = None;
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if (node.is_error() || node.is_missing()) && syntax_error.is_none() {
                syntax_error = Some(node.start_position().row + 1);
            }
            if is_comment(node.kind()) {
                let found = self.read_comment(&text[node.byte_range()], node.start_position().row);
                comment_tags.insert(node.start_byte(), found);
                continue;
            }
            if node.child_count() == 0 {
                if node.start_byte() < node.end_byte() {
                    self.mark_code(node.start_position().row, last_row(node));
                }
                continue;
            }
            if let Some(verifies_only) = taggable(syntax, node) {
                items.push((node, verifies_only));
            }
            if is_import(syntax, node) {
                self.imports
                    .push((node.start_position().row, last_row(node)));
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        if let Some(line) = syntax_error {
            self.problem(line, ProblemKind::SyntaxError);
        }

        let mut attached = BTreeSet::new();
        for (item, verifies_only) in items {
            self.attach(item, verifies_only, &comment_tags, &mut attached);
        }
        for index in 0..self.tags.len() {
            let tag = &self.tags[index];
            let kind = match tag.kind {
                TagKind::Implements | TagKind::Verifies if !attached.contains(&index) => {
                    ProblemKind::OrphanTag(tag.kind)
                }
                TagKind::File => ProblemKind::TagNotAllowed(TagKind::File),
                _ => continue,
            };
            self.problem(tag.line, kind);
        }
        self.regions();
    }

    /// Finds the tags in the comments and attributes directly above an item.
    fn attach(
        &mut self,
        item: Node,
        verifies_only: bool,
        comment_tags: &BTreeMap<usize, Vec<usize>>,
        attached: &mut BTreeSet<usize>,
    ) {
        let mut start_row = item.start_position().row;
        let mut next_row = start_row;
        let mut implements = Vec::new();
        let mut verifies = Vec::new();
        let mut cursor = item.prev_sibling();
        while let Some(prev) = cursor {
            let kind = prev.kind();
            let attribute = is_attribute(kind);
            if !(attribute || is_comment(kind)) || last_row(prev) + 1 < next_row {
                break;
            }
            let before = prev.prev_sibling();
            let trailing = before.is_some_and(|b| {
                !is_comment(b.kind())
                    && !is_attribute(b.kind())
                    && last_row(b) == prev.start_position().row
            });
            if trailing {
                break;
            }
            if attribute {
                start_row = prev.start_position().row;
            }
            for &index in comment_tags.get(&prev.start_byte()).into_iter().flatten() {
                let tag = &self.tags[index];
                let target = match tag.kind {
                    TagKind::Implements if !verifies_only => &mut implements,
                    TagKind::Verifies => &mut verifies,
                    _ => continue,
                };
                target.extend(tag.ids.iter().cloned());
                attached.insert(index);
            }
            next_row = prev.start_position().row;
            cursor = before;
        }
        if !implements.is_empty() || !verifies.is_empty() {
            self.scopes.push(Scope {
                kind: ScopeKind::Item,
                implements,
                verifies,
                start_line: start_row + 1,
                end_line: last_row(item) + 1,
            });
        }
    }

    /// Turns `@begin`/`@end` pairs into region scopes.
    fn regions(&mut self) {
        let mut order: Vec<usize> = (0..self.tags.len()).collect();
        order.sort_by_key(|&i| self.tags[i].line);
        let mut open: Option<usize> = None;
        for index in order {
            let (kind, line) = (self.tags[index].kind, self.tags[index].line);
            match (kind, open) {
                (TagKind::Begin, Some(_)) => self.problem(line, ProblemKind::NestedBegin),
                (TagKind::Begin, None) => open = Some(index),
                (TagKind::End, None) => self.problem(line, ProblemKind::EndWithoutBegin),
                (TagKind::End, Some(begin)) => {
                    let begin = &self.tags[begin];
                    self.scopes.push(Scope {
                        kind: ScopeKind::Region,
                        implements: begin.ids.clone(),
                        verifies: Vec::new(),
                        start_line: begin.line,
                        end_line: line,
                    });
                    open = None;
                }
                _ => {}
            }
        }
        if let Some(begin) = open {
            self.problem(self.tags[begin].line, ProblemKind::UnclosedBegin);
        }
    }

    /// Svelte v0: finds comments line by line and allows only regions.
    fn svelte(&mut self, text: &str) {
        // The closing marker of a block comment left open by an earlier line.
        let mut open: Option<&str> = None;
        for (row, line) in text.lines().enumerate() {
            if open.is_none() && is_svelte_wrapper(line) {
                continue;
            }
            let mut rest = line;
            let mut code = false;
            loop {
                if let Some(close) = open {
                    let (comment, after) = match rest.find(close) {
                        Some(at) => (&rest[..at], Some(&rest[at + close.len()..])),
                        None => (rest, None),
                    };
                    self.read_comment(comment, row);
                    match after {
                        Some(after) => {
                            rest = after;
                            open = None;
                        }
                        None => break,
                    }
                    continue;
                }
                if rest.trim_start().starts_with("//") {
                    self.read_comment(rest, row);
                    break;
                }
                let opener = ["<!--", "/*"]
                    .into_iter()
                    .filter_map(|o| rest.find(o).map(|at| (at, o)))
                    .min();
                match opener {
                    Some((at, marker)) => {
                        code |= !rest[..at].trim().is_empty();
                        rest = &rest[at + marker.len()..];
                        open = Some(if marker == "<!--" { "-->" } else { "*/" });
                    }
                    None => {
                        code |= !rest.trim().is_empty();
                        break;
                    }
                }
            }
            if code {
                self.mark_code(row, row);
            }
        }
        for index in 0..self.tags.len() {
            let tag = &self.tags[index];
            if !matches!(tag.kind, TagKind::Begin | TagKind::End) {
                self.problem(tag.line, ProblemKind::TagNotAllowed(tag.kind));
            }
        }
        self.regions();
    }

    /// Config, CI, and script files: one `@file` tag in the first comment covers the file.
    /// Reads a file with `#` comments. In a workflow, `@verifies` tags in the comment directly
    /// above a step's `- name:` line make a verifies scope over that step.
    fn hash_comments(&mut self, text: &str, workflow: bool) {
        let lines: Vec<&str> = text.lines().collect();
        let mut in_first_comment = true;
        let mut seen_comment = false;
        let mut file_ids = None;
        // `@verifies` tags in the comment block being read, waiting for a step below it.
        let mut waiting: Vec<usize> = Vec::new();
        for (row, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                in_first_comment &= !seen_comment;
                self.orphan_steps(&mut waiting);
                continue;
            }
            if !trimmed.starts_with('#') {
                self.mark_code(row, row);
                in_first_comment = false;
                if !waiting.is_empty() && trimmed.starts_with("- name:") {
                    self.step_scope(&lines, row, &waiting);
                    waiting.clear();
                }
                self.orphan_steps(&mut waiting);
                continue;
            }
            if row == 0 && trimmed.starts_with("#!") {
                continue;
            }
            seen_comment = true;
            for index in self.read_comment(trimmed, row) {
                let tag = &self.tags[index];
                if tag.kind == TagKind::File && in_first_comment && file_ids.is_none() {
                    file_ids = Some(tag.ids.clone());
                } else if workflow && tag.kind == TagKind::Verifies {
                    waiting.push(index);
                } else {
                    self.problem(tag.line, ProblemKind::TagNotAllowed(tag.kind));
                }
            }
        }
        self.orphan_steps(&mut waiting);
        match file_ids {
            Some(ids) => self.scopes.push(Scope {
                kind: ScopeKind::File,
                implements: ids,
                verifies: Vec::new(),
                start_line: 1,
                end_line: self.has_code.len(),
            }),
            None if self.has_code.contains(&true) => self.problem(1, ProblemKind::MissingFileTag),
            None => {}
        }
    }

    /// Reports each waiting workflow `@verifies` tag as not above a step.
    fn orphan_steps(&mut self, waiting: &mut Vec<usize>) {
        for index in waiting.drain(..) {
            self.problem(self.tags[index].line, ProblemKind::NotAboveStep);
        }
    }

    /// Adds a verifies scope for the workflow step whose `- name:` line is on `row`, with the
    /// IDs of the `waiting` tags, unless the step or its job sets `continue-on-error`.
    fn step_scope(&mut self, lines: &[&str], row: usize, waiting: &[usize]) {
        let end = block_end(lines, row);
        let dash = indent(lines[row]);
        let after_dash = &lines[row].trim_start()[1..];
        let key_indent = dash + 1 + indent(after_dash);
        let lenient = continue_on_error(lines, row + 1, end, key_indent)
            .or_else(|| job_continue_on_error(lines, row));
        if let Some(at) = lenient {
            for &index in waiting {
                let kind = ProblemKind::ContinueOnError(at + 1);
                self.problem(self.tags[index].line, kind);
            }
            return;
        }
        self.scopes.push(Scope {
            kind: ScopeKind::Item,
            implements: Vec::new(),
            verifies: waiting
                .iter()
                .flat_map(|&i| self.tags[i].ids.clone())
                .collect(),
            start_line: row + 1,
            end_line: end + 1,
        });
    }

    fn finish(mut self) -> FileScan {
        for index in 0..self.tags.len() {
            let tag = &self.tags[index];
            if tag.ids.is_empty() && tag.kind != TagKind::End {
                self.problem(tag.line, ProblemKind::EmptyTag(tag.kind));
            }
        }
        self.scopes
            .retain(|s| !s.implements.is_empty() || !s.verifies.is_empty());

        let mut covered = vec![false; self.has_code.len()];
        let line_count = covered.len();
        for scope in &self.scopes {
            covered[scope.start_line - 1..scope.end_line.min(line_count)].fill(true);
        }
        let mut implements = BTreeSet::new();
        let mut verifies = BTreeSet::new();
        for scope in self.scopes.iter().filter(|s| s.kind == ScopeKind::Item) {
            implements.extend(scope.implements.iter().cloned());
            verifies.extend(scope.verifies.iter().cloned());
        }
        if !implements.is_empty() || !verifies.is_empty() {
            for &(first, last) in &self.imports {
                if covered[first] {
                    continue;
                }
                covered[first..=last].fill(true);
                self.scopes.push(Scope {
                    kind: ScopeKind::Import,
                    implements: implements.iter().cloned().collect(),
                    verifies: verifies.iter().cloned().collect(),
                    start_line: first + 1,
                    end_line: last + 1,
                });
            }
        }

        let lines = self
            .has_code
            .iter()
            .zip(&covered)
            .map(|(&code, &covered)| match (code, covered) {
                (false, _) => LineStatus::Exempt,
                (true, true) => LineStatus::Traced,
                (true, false) => LineStatus::Untraced,
            })
            .collect();
        self.problems.sort_by_key(|p| p.line);
        FileScan {
            tags: self.tags,
            scopes: self.scopes,
            lines,
            problems: self.problems,
        }
    }
}

// @implements CT-DEV-002, CT-TRC-006
impl FileScan {
    /// Line numbers (1-based) of code lines outside every tagged scope.
    pub fn untraced_lines(&self) -> Vec<usize> {
        let untraced = self.lines.iter().enumerate();
        untraced
            .filter(|(_, status)| **status == LineStatus::Untraced)
            .map(|(index, _)| index + 1)
            .collect()
    }

    /// Every requirement ID whose scope covers the given 1-based line.
    pub fn ids_for_line(&self, line: usize) -> BTreeSet<&str> {
        self.scopes
            .iter()
            .filter(|s| (s.start_line..=s.end_line).contains(&line))
            .flat_map(|s| s.implements.iter().chain(&s.verifies))
            .map(String::as_str)
            .collect()
    }
}

// @verifies CT-DEV-002, CT-TRC-006
#[cfg(test)]
mod tests {
    use super::*;
    use LineStatus::{Exempt, Traced, Untraced};

    fn rust(text: &str) -> FileScan {
        scan_source(Syntax::Rust, text)
    }

    fn ids(scan: &FileScan, line: usize) -> Vec<&str> {
        scan.ids_for_line(line).into_iter().collect()
    }

    fn problem_kinds(scan: &FileScan) -> Vec<(usize, ProblemKind)> {
        scan.problems.iter().map(|p| (p.line, p.kind)).collect()
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn implements_tag_covers_every_line_of_the_item() {
        let scan = rust("// @implements CT-TRC-004\npub fn f() -> u8 {\n    1\n}\n");
        assert_eq!(scan.lines, vec![Exempt, Traced, Traced, Traced]);
        assert_eq!(
            scan.scopes,
            vec![Scope {
                kind: ScopeKind::Item,
                implements: vec!["CT-TRC-004".into()],
                verifies: vec![],
                start_line: 2,
                end_line: 4,
            }]
        );
        assert!(scan.problems.is_empty());
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn verifies_tag_sees_through_attributes_and_doc_comments() {
        let scan = rust(
            "// @verifies CT-TRC-004\n/// Docs.\n#[test]\n#[ignore]\nfn t() {\n    assert!(true);\n}\n",
        );
        assert_eq!(scan.scopes.len(), 1);
        assert_eq!(scan.scopes[0].verifies, vec!["CT-TRC-004".to_string()]);
        assert_eq!((scan.scopes[0].start_line, scan.scopes[0].end_line), (3, 7));
        assert_eq!(
            scan.lines,
            vec![Exempt, Exempt, Traced, Traced, Traced, Traced, Traced]
        );
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn every_item_kind_takes_a_tag() {
        let text = "\
// @implements CT-MOD-001
struct S {
    a: u8,
}
// @implements CT-MOD-001
enum E {
    A,
}
// @implements CT-MOD-001
trait T {
    fn m(&self);
}
// @implements CT-MOD-001
impl T for S {
    fn m(&self) {}
}
// @implements CT-MOD-001
const C: u8 = 1;
// @implements CT-MOD-001
static V: u8 = 2;
// @implements CT-MOD-001
type A = u8;
// @implements CT-MOD-001
macro_rules! m {
    () => {};
}
";
        let scan = rust(text);
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
        assert!(
            scan.untraced_lines().is_empty(),
            "{:?}",
            scan.untraced_lines()
        );
        assert_eq!(scan.scopes.len(), 8);
    }

    // @verifies CT-DEV-002
    #[test]
    fn untagged_item_is_untraced() {
        let scan = rust("fn f() {\n    1;\n}\n");
        assert_eq!(scan.untraced_lines(), vec![1, 2, 3]);
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn tag_separated_by_a_blank_line_is_an_orphan() {
        let scan = rust("// @implements CT-TRC-004\n\nfn f() {}\n");
        assert_eq!(
            problem_kinds(&scan),
            vec![(1, ProblemKind::OrphanTag(TagKind::Implements))]
        );
        assert_eq!(scan.untraced_lines(), vec![3]);
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn trailing_comment_on_a_code_line_does_not_tag_the_next_item() {
        let scan = rust(
            "// @implements CT-TRC-004\nconst A: u8 = 1; // @implements CT-EFF-001\nfn f() {}\n",
        );
        assert_eq!(scan.untraced_lines(), vec![3]);
        assert_eq!(
            problem_kinds(&scan),
            vec![(2, ProblemKind::OrphanTag(TagKind::Implements))]
        );
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn nested_items_inherit_and_add_tags() {
        let text = "\
// @implements CT-MOD-001
impl S {
    fn plain() {}

    // @implements CT-MOD-002
    fn tagged() {}
}
";
        let scan = rust(text);
        assert_eq!(ids(&scan, 3), vec!["CT-MOD-001"]);
        assert_eq!(ids(&scan, 6), vec!["CT-MOD-001", "CT-MOD-002"]);
        assert!(scan.untraced_lines().is_empty());
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn tagged_child_does_not_cover_an_untagged_parent() {
        let scan = rust("impl S {\n    // @implements CT-MOD-001\n    fn f() {}\n}\n");
        assert_eq!(scan.untraced_lines(), vec![1, 4]);
        assert_eq!(scan.lines[2], Traced);
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn imports_inherit_the_union_of_item_tags() {
        let text = "\
use std::fmt;
mod other;
extern crate core;

// @implements CT-TRC-004
fn a() {}
// @verifies CT-EFF-007
fn b() {}
";
        let scan = rust(text);
        assert!(scan.untraced_lines().is_empty());
        for line in 1..=3 {
            assert_eq!(ids(&scan, line), vec!["CT-EFF-007", "CT-TRC-004"]);
        }
    }

    // @verifies CT-DEV-002
    #[test]
    fn imports_alone_never_pass() {
        let scan = rust("use std::fmt;\nmod other;\n");
        assert_eq!(scan.untraced_lines(), vec![1, 2]);
    }

    // @verifies CT-DEV-002
    #[test]
    fn blank_and_comment_lines_are_exempt() {
        let text = "\
//! Module docs.

/* A block
   comment. */
// @implements CT-TRC-004
fn f() {
    // inside
    let _x = 1; // trailing
}
";
        let scan = rust(text);
        assert_eq!(
            scan.lines,
            vec![
                Exempt, Exempt, Exempt, Exempt, Exempt, Traced, Exempt, Traced, Traced
            ]
        );
    }

    // @verifies CT-DEV-002
    #[test]
    fn lines_inside_a_multi_line_string_are_code() {
        let scan = rust("const S: &str = \"a\n\nb\";\n");
        assert_eq!(scan.lines, vec![Untraced, Untraced, Untraced]);
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn begin_end_region_covers_module_level_code() {
        let text = "\
// @begin CT-DEV-009
#![allow(dead_code)]
std::thread_local! {
    static X: u8 = 1;
}
// @end
#![allow(unused)]
";
        let scan = rust(text);
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
        assert_eq!(scan.untraced_lines(), vec![7]);
        assert_eq!(ids(&scan, 3), vec!["CT-DEV-009"]);
        assert_eq!(scan.scopes[0].kind, ScopeKind::Region);
        assert_eq!((scan.scopes[0].start_line, scan.scopes[0].end_line), (1, 6));
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn region_markers_must_pair() {
        let scan = rust(
            "// @end\n// @begin CT-DEV-009\n// @begin CT-DEV-009\n// @end\n// @begin CT-DEV-009\n",
        );
        assert_eq!(
            problem_kinds(&scan),
            vec![
                (1, ProblemKind::EndWithoutBegin),
                (3, ProblemKind::NestedBegin),
                (5, ProblemKind::UnclosedBegin),
            ]
        );
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn tag_without_ids_is_reported_and_covers_nothing() {
        let scan = rust("// @implements\nfn f() {}\n");
        assert_eq!(
            problem_kinds(&scan),
            vec![(1, ProblemKind::EmptyTag(TagKind::Implements))]
        );
        assert_eq!(scan.untraced_lines(), vec![2]);
    }

    // @verifies CT-TRC-006
    #[test]
    fn reads_comma_separated_ids_and_ignores_trailing_words() {
        let scan = rust("/* @implements CT-TRC-004, CT-EFF-007 coverage report */\nfn f() {}\n");
        assert_eq!(
            scan.tags,
            vec![Tag {
                kind: TagKind::Implements,
                ids: vec!["CT-TRC-004".into(), "CT-EFF-007".into()],
                line: 1,
            }]
        );
    }

    // @verifies CT-TRC-006
    #[test]
    fn tag_words_in_prose_are_not_tags() {
        let scan =
            rust("// Tag tests with @verifies and @implementsX.\n// @endpoint docs\nfn f() {}\n");
        assert!(scan.tags.is_empty());
        assert_eq!(scan.untraced_lines(), vec![3]);
    }

    // @verifies CT-DEV-002
    #[test]
    fn file_tag_is_not_allowed_in_source_files() {
        let scan = rust("// @file CT-DEV-009\nfn f() {}\n");
        assert_eq!(
            problem_kinds(&scan),
            vec![(1, ProblemKind::TagNotAllowed(TagKind::File))]
        );
        assert_eq!(scan.untraced_lines(), vec![2]);
    }

    // @verifies CT-DEV-002
    #[test]
    fn syntax_errors_are_reported() {
        let scan = rust("// @implements CT-TRC-004\nfn f( {\n");
        assert!(problem_kinds(&scan).contains(&(2, ProblemKind::SyntaxError)));
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn typescript_items_imports_and_test_blocks() {
        let text = "\
import { a } from './a';

// @implements CT-USE-002
export function openCommandPalette(): void {
  a();
}

// @implements CT-USE-002
export const SHORTCUT = 'Ctrl+K';

// @implements CT-USE-002
class Palette {
  open(): void {}

  // @implements CT-USE-004
  close(): void {}
}

// @verifies CT-USE-002
describe('palette', () => {
  it('opens', () => {});
});

// @implements CT-USE-002
mount();
";
        let scan = scan_source(Syntax::TypeScript, text);
        assert_eq!(ids(&scan, 1), vec!["CT-USE-002", "CT-USE-004"]);
        assert_eq!(ids(&scan, 5), vec!["CT-USE-002"]);
        assert_eq!(ids(&scan, 9), vec!["CT-USE-002"]);
        assert_eq!(ids(&scan, 13), vec!["CT-USE-002"]);
        assert_eq!(ids(&scan, 16), vec!["CT-USE-002", "CT-USE-004"]);
        assert_eq!(ids(&scan, 21), vec!["CT-USE-002"]);
        assert_eq!(scan.untraced_lines(), vec![25]);
        assert_eq!(
            problem_kinds(&scan),
            vec![(24, ProblemKind::OrphanTag(TagKind::Implements))]
        );
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn svelte_uses_regions_only() {
        let text = "\
<script lang=\"ts\">
  // @begin CT-TRC-003
  import { traceMatrix } from '$lib/api';
  let rows = $state([]);
  // @end
</script>

<!-- @begin CT-USE-001 -->
<p>{rows.length}</p>
<!-- @end -->
<!--
  A comment
-->
<style>
  /* @implements CT-USE-001 */
  p { margin: 0; }
</style>
";
        let scan = scan_source(Syntax::Svelte, text);
        assert_eq!(ids(&scan, 3), vec!["CT-TRC-003"]);
        assert_eq!(ids(&scan, 9), vec!["CT-USE-001"]);
        assert_eq!(scan.lines[11], Exempt);
        assert_eq!(scan.untraced_lines(), vec![16]);
        for line in [1, 6, 14, 17] {
            assert_eq!(scan.lines[line - 1], Exempt, "line {line}");
        }
        assert_eq!(
            problem_kinds(&scan),
            vec![(15, ProblemKind::TagNotAllowed(TagKind::Implements))]
        );
    }

    // @verifies CT-DEV-002
    #[test]
    fn only_bare_svelte_wrapper_lines_are_exempt() {
        let text = "<script>\n<script context=\"module\" lang=\"ts\">\n</script>\n<style lang=\"css\">\n</style>\n<script>let a = 1;</script>\n<scripts>\n<p>\n";
        let scan = scan_source(Syntax::Svelte, text);
        assert_eq!(scan.untraced_lines(), vec![6, 7, 8]);
    }

    // @verifies CT-DEV-002, CT-TRC-006
    #[test]
    fn file_tag_in_first_comment_covers_the_whole_file() {
        let scan = scan_source(
            Syntax::HashComments,
            "#!/bin/sh\n# @file CT-DEV-004, CT-PLT-001\n# More words.\n\nname: ci\n",
        );
        assert!(scan.problems.is_empty());
        assert_eq!(scan.lines, vec![Exempt, Exempt, Exempt, Exempt, Traced]);
        assert_eq!(ids(&scan, 5), vec!["CT-DEV-004", "CT-PLT-001"]);
        assert_eq!(scan.scopes[0].kind, ScopeKind::File);
    }

    // @verifies CT-DEV-002
    #[test]
    fn file_tag_must_be_in_the_first_comment() {
        let scan = scan_source(
            Syntax::HashComments,
            "# Config.\nname: ci\n# @file CT-DEV-004\n",
        );
        assert_eq!(
            problem_kinds(&scan),
            vec![
                (1, ProblemKind::MissingFileTag),
                (3, ProblemKind::TagNotAllowed(TagKind::File)),
            ]
        );
        assert_eq!(scan.untraced_lines(), vec![2]);
    }

    /// A workflow whose `build` job has the given job-level lines and steps.
    // @verifies CT-DEV-003, CT-TRC-006
    fn workflow(job: &str, steps: &str) -> FileScan {
        let text = format!(
            "# @file CT-DEV-004\nname: ci\njobs:\n  build:\n    runs-on: x\n{job}    steps:\n{steps}"
        );
        scan_source(Syntax::Workflow, &text)
    }

    // @verifies CT-DEV-003, CT-TRC-006
    #[test]
    fn workflow_verifies_tag_covers_the_step_below_it() {
        let scan = workflow(
            "",
            "      - uses: x\n\
             \x20     # Fails on a big installer.\n\
             \x20     # @verifies CT-EFF-001, CT-PLT-001\n\
             \x20     - name: Size\n\
             \x20       continue-on-error: false # never\n\
             \x20       run: |\n\
             \x20         exit 1\n\
             \n\
             \x20     # Next step.\n\
             \x20     - name: Upload\n",
        );
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
        let steps: Vec<&Scope> = (scan.scopes.iter())
            .filter(|s| s.kind == ScopeKind::Item)
            .collect();
        assert_eq!(
            steps,
            [&Scope {
                kind: ScopeKind::Item,
                implements: vec![],
                verifies: vec!["CT-EFF-001".into(), "CT-PLT-001".into()],
                start_line: 10,
                end_line: 13,
            }]
        );
        assert_eq!(
            ids(&scan, 12),
            vec!["CT-DEV-004", "CT-EFF-001", "CT-PLT-001"]
        );
    }

    // @verifies CT-DEV-003, CT-TRC-006
    #[test]
    fn workflow_verifies_tag_must_be_directly_above_a_step() {
        let scan = workflow(
            "",
            "      # @verifies CT-EFF-001\n\
             \n\
             \x20     - name: Gap\n\
             \x20     # @verifies CT-EFF-001\n\
             \x20     - uses: x\n\
             \x20       # @implements CT-EFF-001\n\
             \x20       with: {}\n\
             \x20     # @verifies CT-EFF-001\n",
        );
        assert_eq!(
            problem_kinds(&scan),
            vec![
                (7, ProblemKind::NotAboveStep),
                (10, ProblemKind::NotAboveStep),
                (12, ProblemKind::TagNotAllowed(TagKind::Implements)),
                (14, ProblemKind::NotAboveStep),
            ]
        );
        assert!(scan.scopes.iter().all(|s| s.kind == ScopeKind::File));
        let other = scan_source(
            Syntax::HashComments,
            "# @file CT-DEV-004\n# @verifies CT-EFF-001\n- name: x\n",
        );
        assert_eq!(
            problem_kinds(&other),
            vec![(2, ProblemKind::TagNotAllowed(TagKind::Verifies))]
        );
    }

    // @verifies CT-DEV-003, CT-TRC-006
    #[test]
    fn a_step_that_may_fail_cannot_verify() {
        let tagged = "      # @verifies CT-EFF-001\n      - name: Size\n";
        let step = format!("{tagged}        continue-on-error: true\n        run: exit 1\n");
        let scan = workflow("", &step);
        assert_eq!(
            problem_kinds(&scan),
            vec![(7, ProblemKind::ContinueOnError(9))]
        );
        assert!(scan.scopes.iter().all(|s| s.kind == ScopeKind::File));
        let scan = workflow("    continue-on-error: ${{ matrix.x }}\n", tagged);
        assert_eq!(
            problem_kinds(&scan),
            vec![(8, ProblemKind::ContinueOnError(6))]
        );
        let scan = workflow("    continue-on-error: false\n", tagged);
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
        // Steps listed at the indent of `steps:`, and a `- name:` outside any `steps:`.
        let compact = "# @file CT-DEV-004\njobs:\n  a:\n    continue-on-error: true\n    steps:\n\
                       \x20   # @verifies CT-EFF-001\n    - name: x\n";
        let scan = scan_source(Syntax::Workflow, compact);
        assert_eq!(
            problem_kinds(&scan),
            vec![(6, ProblemKind::ContinueOnError(4))]
        );
        let loose = "# @file CT-DEV-004\nlist:\n  # @verifies CT-EFF-001\n  - name: x\n";
        let scan = scan_source(Syntax::Workflow, loose);
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
        assert_eq!(scan.scopes.len(), 2);
        let top = "# @verifies CT-EFF-001\n- name: x\n";
        let scan = scan_source(Syntax::Workflow, top);
        assert_eq!(problem_kinds(&scan), vec![(1, ProblemKind::MissingFileTag)]);
    }

    // @verifies CT-DEV-003, CT-TRC-006
    #[test]
    fn workflow_problems_say_what_is_wrong() {
        let not_above = Problem {
            line: 3,
            kind: ProblemKind::NotAboveStep,
        };
        assert_eq!(
            not_above.to_string(),
            "line 3: `@verifies` is not directly above a workflow step's `- name:` line"
        );
        let lenient = Problem {
            line: 3,
            kind: ProblemKind::ContinueOnError(5),
        };
        assert_eq!(
            lenient.to_string(),
            "line 3: `continue-on-error` on line 5 lets this step fail without failing the build"
        );
    }

    // @verifies CT-DEV-002
    #[test]
    fn picks_syntax_from_the_file_extension() {
        let cases = [
            ("a/b.rs", Some(Syntax::Rust)),
            ("a/b.ts", Some(Syntax::TypeScript)),
            ("a/b.svelte", Some(Syntax::Svelte)),
            ("Cargo.toml", Some(Syntax::HashComments)),
            ("ci.yml", Some(Syntax::HashComments)),
            ("ci.yaml", Some(Syntax::HashComments)),
            (".github/workflows/ci.yml", Some(Syntax::Workflow)),
            ("x/.github/workflows/release.yaml", Some(Syntax::Workflow)),
            (".github/workflows/notes.toml", Some(Syntax::HashComments)),
            (".github/ci.yml", Some(Syntax::HashComments)),
            ("x.sh", Some(Syntax::HashComments)),
            ("x.ps1", Some(Syntax::HashComments)),
            ("README.md", None),
        ];
        for (path, want) in cases {
            assert_eq!(Syntax::from_path(Path::new(path)), want, "{path}");
        }
    }
}
