//! Recursive descent parser for the CT-MOD-002 subset of SysML v2.
//!
//! The parser never stops at an error. It records the error, skips to the end of the
//! broken member (the next `;`, a balanced `{ ... }`, or the next member keyword), and
//! carries on, so one pass reports every problem in a file. Lexer errors are merged into
//! the same list, sorted by position.

use std::fmt;

use super::ast::{
    Comment, Definition, Dependency, DependencyKind, Direction, Import, ImportKind, Kind, Member,
    Package, Pos, QualifiedName, Satisfy, Usage, UsageKind, Value, ValueKind, Verify, Visibility,
};
use super::lexer::{self, LexError, Token, TokenKind};

// @implements CT-USE-007
/// A lexer or parser error: what went wrong, where, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub help: &'static str,
}

// @implements CT-USE-007
impl SyntaxError {
    /// The error as `path:line:column: error: message` plus a `help:` line.
    pub fn render(&self, path: &str) -> String {
        format!("{path}:{self}")
    }
}

// @implements CT-USE-007
impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: error: {}\n  help: {}",
            self.line, self.column, self.message, self.help
        )
    }
}

// @implements CT-USE-007
impl From<LexError> for SyntaxError {
    fn from(e: LexError) -> Self {
        SyntaxError {
            line: e.line,
            column: e.column,
            message: e.message,
            help: e.help,
        }
    }
}

// @implements CT-MOD-002, CT-USE-007
/// The members of a file's root namespace, and every error found, sorted by position.
#[derive(Debug, Default)]
pub struct Parsed {
    pub members: Vec<Member>,
    pub errors: Vec<SyntaxError>,
}

// @implements CT-MOD-002, CT-USE-007
/// Parses SysML v2 text. Errors do not stop parsing; see `Parsed::errors`.
pub fn parse(src: &str) -> Parsed {
    let lexed = lexer::lex(src);
    let mut parser = Parser {
        src,
        tokens: lexed.tokens,
        at: 0,
        errors: Vec::new(),
    };
    let members = parser.members(None);
    let mut errors: Vec<SyntaxError> = lexed.errors.into_iter().map(SyntaxError::from).collect();
    errors.append(&mut parser.errors);
    errors.sort_by_key(|e| (e.line, e.column));
    Parsed { members, errors }
}

// @implements CT-MOD-002, CT-USE-007
const SUPPORTED: &str = "Clewtrace reads packages, imports, comments and doc, part, attribute, \
port, and requirement definitions and usages, metadata definitions, subject, satisfy, verify, \
and `#derivation`, `#refinement`, and `#evidence` dependencies";

// @implements CT-TRC-002, CT-USE-007
const DEPENDENCY_HELP: &str = "write `#derivation dependency a to b;` (a is derived from b), \
`#refinement dependency a to b;` (a refines b), or `#evidence dependency a to b;` (a is evidence \
for b)";

// @implements CT-MOD-002, CT-USE-007
/// Keywords that start a member. Recovery stops before them so one missing `;` does not
/// hide the next member.
const MEMBER_START: &[&str] = &[
    "abstract",
    "attribute",
    "comment",
    "dependency",
    "doc",
    "import",
    "in",
    "inout",
    "metadata",
    "out",
    "package",
    "part",
    "port",
    "private",
    "protected",
    "public",
    "requirement",
    "satisfy",
    "subject",
    "verify",
];

// @implements CT-MOD-002
/// Removes each line's leading `*` and surrounding spaces, and blank first and last lines.
fn clean_comment(raw: &str) -> String {
    let lines: Vec<&str> = raw
        .lines()
        .map(|line| {
            let line = line.trim();
            line.strip_prefix('*').map_or(line, str::trim_start)
        })
        .collect();
    let first = lines.iter().position(|l| !l.is_empty());
    let last = lines.iter().rposition(|l| !l.is_empty());
    match (first, last) {
        (Some(first), Some(last)) => lines[first..=last].join("\n"),
        _ => String::new(),
    }
}

// @implements CT-MOD-002, CT-USE-007
struct Parser<'a> {
    src: &'a str,
    /// Always ends with an `Eof` token, which `bump` never moves past.
    tokens: Vec<Token>,
    at: usize,
    errors: Vec<SyntaxError>,
}

// @implements CT-MOD-002, CT-USE-007
/// Token access, errors, and recovery.
impl<'a> Parser<'a> {
    fn peek(&self) -> Token {
        self.tokens[self.at]
    }

    fn peek_at(&self, offset: usize) -> Token {
        self.tokens[(self.at + offset).min(self.tokens.len() - 1)]
    }

    fn bump(&mut self) -> Token {
        let token = self.peek();
        if token.kind != TokenKind::Eof {
            self.at += 1;
        }
        token
    }

    fn is(&self, word: &str) -> bool {
        self.peek().is(word, self.src)
    }

    fn eat(&mut self, word: &str) -> bool {
        let found = self.is(word);
        if found {
            self.bump();
        }
        found
    }

    fn text(&self, token: Token) -> &'a str {
        token.text(self.src)
    }

    /// A member keyword, or the `#` of a dependency's metadata.
    fn is_member_start(&self, token: Token) -> bool {
        (token.kind == TokenKind::Keyword && MEMBER_START.contains(&self.text(token)))
            || token.is("#", self.src)
    }

    /// How a token reads in a message.
    fn describe(&self, token: Token) -> String {
        match token.kind {
            TokenKind::Eof => "the end of the file".to_string(),
            TokenKind::Comment => "a comment".to_string(),
            _ => format!("`{}`", self.text(token)),
        }
    }

    /// Records an error at `token`. Error tokens were already reported by the lexer, an
    /// error right after one is usually caused by it, and a second error at the same place
    /// adds nothing, so all three are dropped.
    fn error(&mut self, token: Token, message: String, help: &'static str) {
        let repeated = self
            .errors
            .last()
            .is_some_and(|e| (e.line, e.column) == (token.line, token.column));
        let after_error = self.at > 0 && self.tokens[self.at - 1].kind == TokenKind::Error;
        if token.kind == TokenKind::Error || after_error || repeated {
            return;
        }
        self.errors.push(SyntaxError {
            line: token.line,
            column: token.column,
            message,
            help,
        });
    }

    /// Reports an unexpected token where a member should start, then skips the member.
    fn unexpected_member(&mut self, token: Token) {
        let message = if token.kind == TokenKind::Keyword {
            format!("`{}` is not supported yet", self.text(token))
        } else {
            format!(
                "expected a member such as `part` or `package`, found {}",
                self.describe(token)
            )
        };
        self.error(token, message, SUPPORTED);
        self.skip_member();
    }

    /// Skips to the end of a broken member: through the next `;` or balanced `{ ... }`, or
    /// up to the next `}` or member keyword. Always consumes at least one token.
    fn skip_member(&mut self) {
        let mut first = true;
        loop {
            let token = self.peek();
            if token.kind == TokenKind::Eof
                || token.is("}", self.src)
                || (!first && self.is_member_start(token))
            {
                return;
            }
            first = false;
            self.bump();
            if token.is(";", self.src) {
                return;
            }
            if token.is("{", self.src) {
                self.skip_block(token);
                return;
            }
        }
    }

    /// Skips the rest of a block whose `{` was just consumed, through its `}`.
    fn skip_block(&mut self, open: Token) {
        let mut depth = 1;
        while depth > 0 {
            let token = self.bump();
            match token.kind {
                TokenKind::Eof => {
                    self.error(
                        open,
                        "`{` is never closed".to_string(),
                        "add `}` at the end of this body",
                    );
                    return;
                }
                _ if token.is("{", self.src) => depth += 1,
                _ if token.is("}", self.src) => depth -= 1,
                _ => {}
            }
        }
    }

    /// After a missing token, skips the rest of the member unless the parser already
    /// stands at the start of the next one.
    fn recover(&mut self) {
        let token = self.peek();
        if !(self.is_member_start(token) || token.is("}", self.src)) {
            self.skip_member();
        }
    }

    fn expect_semicolon(&mut self, after: &str) {
        if !self.eat(";") {
            let token = self.peek();
            let found = self.describe(token);
            self.error(
                token,
                format!("expected `;` after {after}, found {found}"),
                "end the declaration with `;`",
            );
            self.recover();
        }
    }
}

// @implements CT-MOD-002
/// Names and name lists.
impl Parser<'_> {
    /// A basic or quoted name, without quotes.
    fn name(&mut self) -> Option<String> {
        let token = self.peek();
        match token.kind {
            TokenKind::Name => Some(self.bump().text(self.src).to_string()),
            TokenKind::UnrestrictedName => Some(self.bump().value(self.src).into_owned()),
            _ => None,
        }
    }

    fn at_name(&self, offset: usize) -> bool {
        matches!(
            self.peek_at(offset).kind,
            TokenKind::Name | TokenKind::UnrestrictedName
        )
    }

    /// A name with `::` segments. Stops before `::*` and `::**`.
    fn qualified_name(&mut self) -> Option<QualifiedName> {
        let first = self.peek();
        let mut segments = vec![self.name()?];
        while self.is("::") && self.at_name(1) {
            self.bump();
            segments.extend(self.name());
        }
        Some(QualifiedName {
            pos: pos(first),
            segments,
        })
    }

    /// A qualified name that must follow `after`, or an error naming it.
    fn required_name(&mut self, after: Token) -> Option<QualifiedName> {
        let name = self.qualified_name();
        if name.is_none() {
            let token = self.peek();
            let message = format!(
                "expected a name after `{}`, found {}",
                self.text(after),
                self.describe(token)
            );
            self.error(
                token,
                message,
                "write a name or a qualified name, such as `Wheel` or `Parts::Wheel`",
            );
        }
        name
    }

    /// One or more qualified names separated by commas, after the operator `op`.
    fn name_list(&mut self, op: Token) -> Vec<QualifiedName> {
        let mut names = Vec::new();
        let mut after = op;
        loop {
            names.extend(self.required_name(after));
            if !self.is(",") {
                return names;
            }
            after = self.bump();
        }
    }

    /// An optional short name `<'ID'>` and an optional name.
    fn identification(&mut self) -> (Option<String>, Option<String>) {
        let mut short_name = None;
        if self.is("<") {
            self.bump();
            short_name = self.name();
            if short_name.is_none() || !self.eat(">") {
                let token = self.peek();
                let found = self.describe(token);
                self.error(
                    token,
                    format!("expected a short name like <'CT-EFF-001'>, found {found}"),
                    "write the short name as a name or quoted name between `<` and `>`",
                );
            }
        }
        (short_name, self.name())
    }

    /// `;` for an empty body, or `{ members }`.
    fn body(&mut self, after: &str) -> Vec<Member> {
        if self.eat(";") {
            return Vec::new();
        }
        if self.is("{") {
            let open = self.bump();
            return self.members(Some(open));
        }
        let token = self.peek();
        let found = self.describe(token);
        self.error(
            token,
            format!("expected `;` or `{{` after {after}, found {found}"),
            "end the declaration with `;`, or open a body with `{`",
        );
        self.recover();
        Vec::new()
    }
}

// @implements CT-MOD-002
/// Members.
impl Parser<'_> {
    /// Members up to the `}` that closes `open`, or to the end of input for the root.
    fn members(&mut self, open: Option<Token>) -> Vec<Member> {
        let mut members = Vec::new();
        loop {
            let token = self.peek();
            if token.kind == TokenKind::Eof {
                if let Some(open) = open {
                    self.error(
                        open,
                        "`{` is never closed".to_string(),
                        "add `}` at the end of this body",
                    );
                }
                return members;
            }
            if token.is("}", self.src) {
                self.bump();
                if open.is_some() {
                    return members;
                }
                self.error(
                    token,
                    "unexpected `}` with no matching `{`".to_string(),
                    "delete it, or add the missing `{` above it",
                );
                continue;
            }
            // `member` always consumes a token here: the loop has already handled `}` and
            // the end of input, and `skip_member` consumes anything else.
            let before = self.at;
            members.extend(self.member());
            debug_assert!(self.at > before, "member() consumed no token");
        }
    }

    fn member(&mut self) -> Option<Member> {
        let start = self.peek();
        if start.kind == TokenKind::Comment {
            self.bump();
            return Some(Member::Comment(Comment {
                pos: pos(start),
                is_doc: false,
                short_name: None,
                name: None,
                about: Vec::new(),
                body: clean_comment(&start.value(self.src)),
            }));
        }
        let visibility = self.visibility();
        let modifier = self.peek();
        let direction = self.direction();
        let is_abstract = self.eat("abstract");
        let token = self.peek();
        let word = match token.kind {
            TokenKind::Keyword => self.text(token),
            _ => "",
        };
        let kind = match word {
            "part" => Some(Kind::Part),
            "attribute" => Some(Kind::Attribute),
            "port" => Some(Kind::Port),
            "requirement" => Some(Kind::Requirement),
            "metadata" => Some(Kind::Metadata),
            _ => None,
        };
        let is_def = kind.is_some() && self.peek_at(1).is("def", self.src);
        let is_feature = kind.is_some() || word == "subject";
        if (direction.is_some() && (is_def || !is_feature)) || (is_abstract && !is_feature) {
            let message = format!(
                "`{}` cannot be used before `{}`",
                self.text(modifier),
                self.text(token)
            );
            self.error(
                modifier,
                message,
                "remove it; `in`, `out`, and `inout` apply to usages, and `abstract` to definitions and usages",
            );
        }
        let pos = pos(start);
        if token.is("#", self.src) {
            return self.dependency(pos, visibility);
        }
        match word {
            "package" => Some(self.package(pos, visibility)),
            "import" => self.import(pos, visibility),
            "doc" | "comment" => self.comment(pos),
            "satisfy" => self.satisfy(pos),
            "verify" => self.verify(pos),
            "dependency" => {
                self.error(
                    token,
                    "a dependency needs `#derivation`, `#refinement`, or `#evidence` before it"
                        .to_string(),
                    DEPENDENCY_HELP,
                );
                self.skip_member();
                None
            }
            "subject" => {
                Some(self.usage(pos, visibility, direction, is_abstract, UsageKind::Subject))
            }
            _ => match kind {
                Some(kind) if is_def => Some(self.definition(pos, visibility, is_abstract, kind)),
                Some(Kind::Metadata) => {
                    let message = "`metadata` is supported only in `metadata def`".to_string();
                    self.error(token, message, SUPPORTED);
                    self.skip_member();
                    None
                }
                Some(kind) => {
                    Some(self.usage(pos, visibility, direction, is_abstract, UsageKind::Of(kind)))
                }
                None => {
                    self.unexpected_member(token);
                    None
                }
            },
        }
    }

    fn visibility(&mut self) -> Option<Visibility> {
        let visibility = match self.text(self.peek()) {
            "public" => Visibility::Public,
            "private" => Visibility::Private,
            "protected" => Visibility::Protected,
            _ => return None,
        };
        self.bump();
        Some(visibility)
    }

    fn direction(&mut self) -> Option<Direction> {
        let direction = match self.text(self.peek()) {
            "in" => Direction::In,
            "out" => Direction::Out,
            "inout" => Direction::InOut,
            _ => return None,
        };
        self.bump();
        Some(direction)
    }
}

// @implements CT-MOD-002
/// Packages, imports, and comments.
impl Parser<'_> {
    fn package(&mut self, pos: Pos, visibility: Option<Visibility>) -> Member {
        self.bump();
        let (short_name, name) = self.identification();
        let members = self.body("the package name");
        Member::Package(Package {
            pos,
            visibility,
            short_name,
            name,
            members,
        })
    }

    fn import(&mut self, pos: Pos, visibility: Option<Visibility>) -> Option<Member> {
        self.bump();
        let all = self.eat("all");
        let Some(target) = self.qualified_name() else {
            let token = self.peek();
            let found = self.describe(token);
            self.error(
                token,
                format!("expected a name to import, found {found}"),
                "write what to import, such as `import ScalarValues::*;`",
            );
            self.recover();
            return None;
        };
        let mut kind = ImportKind::Element;
        if self.is("::") && self.peek_at(1).is("*", self.src) {
            self.bump();
            self.bump();
            kind = ImportKind::Members;
        }
        if self.is("::") && self.peek_at(1).is("**", self.src) {
            self.bump();
            self.bump();
            kind = match kind {
                ImportKind::Members => ImportKind::MembersRecursive,
                _ => ImportKind::Recursive,
            };
        }
        self.expect_semicolon("the import");
        Some(Member::Import(Import {
            pos,
            visibility,
            all,
            target,
            kind,
        }))
    }

    fn comment(&mut self, pos: Pos) -> Option<Member> {
        let keyword = self.bump();
        let is_doc = self.text(keyword) == "doc";
        let (short_name, name) = self.identification();
        let mut about = Vec::new();
        if !is_doc && self.is("about") {
            let op = self.bump();
            about = self.name_list(op);
        }
        let token = self.peek();
        if token.kind != TokenKind::Comment {
            let message = format!(
                "expected the text of the {} as /* ... */, found {}",
                self.text(keyword),
                self.describe(token)
            );
            self.error(token, message, "write the text between `/*` and `*/`");
            self.recover();
            return None;
        }
        self.bump();
        Some(Member::Comment(Comment {
            pos,
            is_doc,
            short_name,
            name,
            about,
            body: clean_comment(&token.value(self.src)),
        }))
    }
}

// @implements CT-MOD-002
/// Definitions and usages.
impl Parser<'_> {
    fn definition(
        &mut self,
        pos: Pos,
        visibility: Option<Visibility>,
        is_abstract: bool,
        kind: Kind,
    ) -> Member {
        self.bump();
        self.bump();
        let (short_name, name) = self.identification();
        let mut specializes = Vec::new();
        if self.is(":>") || self.is("specializes") {
            let op = self.bump();
            specializes = self.name_list(op);
        }
        let members = self.body("the definition");
        Member::Definition(Definition {
            pos,
            visibility,
            kind,
            is_abstract,
            short_name,
            name,
            specializes,
            members,
        })
    }

    fn usage(
        &mut self,
        pos: Pos,
        visibility: Option<Visibility>,
        direction: Option<Direction>,
        is_abstract: bool,
        kind: UsageKind,
    ) -> Member {
        self.bump();
        let (short_name, name) = self.identification();
        let mut usage = Usage {
            pos,
            visibility,
            kind,
            direction,
            is_abstract,
            short_name,
            name,
            multiplicity: None,
            types: Vec::new(),
            subsets: Vec::new(),
            redefines: Vec::new(),
            value: None,
            members: Vec::new(),
        };
        loop {
            let op = self.peek();
            if self.is("[") && usage.multiplicity.is_none() {
                usage.multiplicity = self.multiplicity();
            } else if self.is(":") {
                self.bump();
                usage.types.extend(self.name_list(op));
            } else if self.is("defined") && self.peek_at(1).is("by", self.src) {
                self.bump();
                let by = self.bump();
                usage.types.extend(self.name_list(by));
            } else if self.is(":>") || self.is("subsets") {
                self.bump();
                usage.subsets.extend(self.name_list(op));
            } else if self.is(":>>") || self.is("redefines") {
                self.bump();
                usage.redefines.extend(self.name_list(op));
            } else {
                break;
            }
        }
        if self.is("=") || self.is(":=") {
            let op = self.bump();
            usage.value = self.value(op, false);
        } else if self.is("default") {
            let mut op = self.bump();
            if self.is("=") || self.is(":=") {
                op = self.bump();
            }
            usage.value = self.value(op, true);
        }
        usage.members = self.body("the usage");
        Member::Usage(usage)
    }

    /// `[1]`, `[0..*]`, and the like. Returns the text inside the brackets.
    fn multiplicity(&mut self) -> Option<String> {
        self.bump();
        let mut text = String::new();
        loop {
            let token = self.peek();
            if token.is("]", self.src) {
                self.bump();
                return Some(text);
            }
            let ends = [";", "{", "}"].iter().any(|w| token.is(w, self.src));
            if ends || token.kind == TokenKind::Eof {
                let found = self.describe(token);
                self.error(
                    token,
                    format!("expected `]` to close the multiplicity, found {found}"),
                    "write the multiplicity like [1], [0..1], or [1..*]",
                );
                return None;
            }
            text.push_str(self.bump().text(self.src));
        }
    }
}

// @implements CT-MOD-002
/// Feature values, satisfy, and verify.
impl Parser<'_> {
    /// The tokens after `op` up to `;`, `{`, `}`, or a member keyword.
    fn value(&mut self, op: Token, is_default: bool) -> Option<Value> {
        let first = self.at;
        loop {
            let token = self.peek();
            let ends = [";", "{", "}"].iter().any(|w| token.is(w, self.src));
            if ends || token.kind == TokenKind::Eof || self.is_member_start(token) {
                break;
            }
            self.bump();
        }
        let tokens = &self.tokens[first..self.at];
        let (Some(head), Some(tail)) = (tokens.first(), tokens.last()) else {
            let token = self.peek();
            let message = format!(
                "expected a value after `{}`, found {}",
                self.text(op),
                self.describe(token)
            );
            self.error(
                token,
                message,
                "write a value such as \"text\", 42, true, or a name",
            );
            return None;
        };
        let single = tokens.len() == 1;
        let kind = match head.kind {
            _ if !single => ValueKind::Expression,
            TokenKind::String => ValueKind::String,
            TokenKind::Integer => ValueKind::Integer,
            TokenKind::Real => ValueKind::Real,
            TokenKind::Name | TokenKind::UnrestrictedName => ValueKind::Name,
            _ if head.is("true", self.src) || head.is("false", self.src) => ValueKind::Boolean,
            _ => ValueKind::Expression,
        };
        let text = match kind {
            ValueKind::String | ValueKind::Name => head.value(self.src).into_owned(),
            _ => self.src[head.start..tail.end].to_string(),
        };
        Some(Value {
            pos: pos(*head),
            kind,
            text,
            is_default,
        })
    }

    fn satisfy(&mut self, pos: Pos) -> Option<Member> {
        let mut after = self.bump();
        if self.is("requirement") {
            after = self.bump();
        }
        let requirement = self.required_name(after);
        let mut by = None;
        if requirement.is_some() && self.is("by") {
            let op = self.bump();
            by = self.required_name(op);
        }
        let Some(requirement) = requirement else {
            self.recover();
            return None;
        };
        let members = self.body("the satisfy");
        Some(Member::Satisfy(Satisfy {
            pos,
            requirement,
            by,
            members,
        }))
    }

    fn verify(&mut self, pos: Pos) -> Option<Member> {
        let mut after = self.bump();
        if self.is("requirement") {
            after = self.bump();
        }
        let Some(requirement) = self.required_name(after) else {
            self.recover();
            return None;
        };
        let members = self.body("the verify");
        Some(Member::Verify(Verify {
            pos,
            requirement,
            members,
        }))
    }
}

// @implements CT-TRC-002, CT-USE-007
/// Dependencies that carry trace links.
impl Parser<'_> {
    /// `#kind dependency [<'id'> name from] a, b to c, d` and a body, starting at `#`.
    fn dependency(&mut self, pos: Pos, visibility: Option<Visibility>) -> Option<Member> {
        self.bump();
        let token = self.peek();
        let Some(meta) = self.name() else {
            let found = self.describe(token);
            self.error(
                token,
                format!("expected a metadata name after `#`, found {found}"),
                DEPENDENCY_HELP,
            );
            self.recover();
            return None;
        };
        let kind = match meta.as_str() {
            "derivation" => DependencyKind::Derivation,
            "refinement" => DependencyKind::Refinement,
            "evidence" => DependencyKind::Evidence,
            _ => {
                self.error(
                    token,
                    format!("`#{meta}` is not supported"),
                    DEPENDENCY_HELP,
                );
                self.skip_member();
                return None;
            }
        };
        let keyword = self.peek();
        if !keyword.is("dependency", self.src) {
            let found = self.describe(keyword);
            self.error(
                keyword,
                format!("expected `dependency` after `#{meta}`, found {found}"),
                DEPENDENCY_HELP,
            );
            self.recover();
            return None;
        }
        let mut after = self.bump();
        let (mut short_name, mut name) = (None, None);
        if self.is("<") || (self.at_name(0) && self.peek_at(1).is("from", self.src)) {
            (short_name, name) = self.identification();
        }
        if self.is("from") {
            after = self.bump();
        }
        let clients = self.name_list(after);
        if clients.is_empty() {
            self.recover();
            return None;
        }
        if !self.is("to") {
            let token = self.peek();
            let found = self.describe(token);
            self.error(
                token,
                format!("expected `to` after the dependency's sources, found {found}"),
                DEPENDENCY_HELP,
            );
            self.recover();
            return None;
        }
        let to = self.bump();
        let suppliers = self.name_list(to);
        if suppliers.is_empty() {
            self.recover();
            return None;
        }
        let members = self.body("the dependency");
        Some(Member::Dependency(Dependency {
            pos,
            visibility,
            kind,
            short_name,
            name,
            clients,
            suppliers,
            members,
        }))
    }
}

// @implements CT-MOD-002, CT-USE-007
fn pos(token: Token) -> Pos {
    Pos {
        line: token.line,
        column: token.column,
    }
}

// @verifies CT-MOD-002, CT-USE-007
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-MOD-002
    fn parse_ok(src: &str) -> Vec<Member> {
        let parsed = parse(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        parsed.members
    }

    // @verifies CT-USE-007
    fn errors(src: &str) -> Vec<(u32, u32, String)> {
        parse(src)
            .errors
            .into_iter()
            .map(|e| (e.line, e.column, e.message))
            .collect()
    }

    // @verifies CT-MOD-002
    fn only(src: &str) -> Member {
        let mut members = parse_ok(src);
        assert_eq!(members.len(), 1, "{members:?}");
        members.remove(0)
    }

    // @verifies CT-MOD-002
    fn usage(src: &str) -> Usage {
        match only(src) {
            Member::Usage(u) => u,
            other => panic!("expected a usage, got {other:?}"),
        }
    }

    // @verifies CT-MOD-002
    fn names(list: &[QualifiedName]) -> Vec<String> {
        list.iter().map(QualifiedName::joined).collect()
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_packages() {
        let src = "package <'P1'> 'Top Level' {\n  public package Inner;\n  package { }\n}";
        let Member::Package(top) = only(src) else {
            panic!("expected a package")
        };
        assert_eq!(top.pos, Pos { line: 1, column: 1 });
        assert_eq!(top.short_name.as_deref(), Some("P1"));
        assert_eq!(top.name.as_deref(), Some("Top Level"));
        assert_eq!(top.visibility, None);
        assert_eq!(top.members.len(), 2);
        let Member::Package(inner) = &top.members[0] else {
            panic!("expected a package")
        };
        assert_eq!(inner.visibility, Some(Visibility::Public));
        assert_eq!(inner.name.as_deref(), Some("Inner"));
        assert_eq!(inner.pos, Pos { line: 2, column: 3 });
        assert!(inner.members.is_empty());
        let Member::Package(anonymous) = &top.members[1] else {
            panic!("expected a package")
        };
        assert_eq!(anonymous.name, None);
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_imports() {
        let src = "import A; private import all B::C::*; protected import D::**; import E::*::**;\
                   import 'F G'::H;";
        let imports: Vec<(Option<Visibility>, bool, String, ImportKind)> = parse_ok(src)
            .into_iter()
            .map(|m| match m {
                Member::Import(i) => (i.visibility, i.all, i.target.joined(), i.kind),
                other => panic!("expected an import, got {other:?}"),
            })
            .collect();
        assert_eq!(
            imports,
            vec![
                (None, false, "A".into(), ImportKind::Element),
                (
                    Some(Visibility::Private),
                    true,
                    "B::C".into(),
                    ImportKind::Members
                ),
                (
                    Some(Visibility::Protected),
                    false,
                    "D".into(),
                    ImportKind::Recursive
                ),
                (None, false, "E".into(), ImportKind::MembersRecursive),
                (None, false, "F G::H".into(), ImportKind::Element),
            ]
        );
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_comments_and_doc() {
        let src = "doc /* One line. */\n\
                   comment <'C1'> note about A, B::c /*\n * First.\n *\n * Second.\n */\n\
                   /* bare */\n\
                   doc /**/";
        let comments: Vec<Comment> = parse_ok(src)
            .into_iter()
            .map(|m| match m {
                Member::Comment(c) => c,
                other => panic!("expected a comment, got {other:?}"),
            })
            .collect();
        assert_eq!(comments.len(), 4);
        assert!(comments[0].is_doc);
        assert_eq!(comments[0].body, "One line.");
        let note = &comments[1];
        assert!(!note.is_doc);
        assert_eq!(note.short_name.as_deref(), Some("C1"));
        assert_eq!(note.name.as_deref(), Some("note"));
        assert_eq!(names(&note.about), ["A", "B::c"]);
        assert_eq!(note.body, "First.\n\nSecond.");
        assert_eq!(note.pos, Pos { line: 2, column: 1 });
        assert!(!comments[2].is_doc);
        assert_eq!(comments[2].body, "bare");
        assert_eq!(comments[2].pos, Pos { line: 7, column: 1 });
        assert_eq!(comments[3].body, "");
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_definitions_of_each_kind() {
        let src = "part def Vehicle;\n\
                   abstract attribute def <'A1'> Mass :> Real, Base::Quantity;\n\
                   port def Fuel specializes Flow { doc /* d */ }\n\
                   requirement def <'R'> Req { attribute phase : Integer; }";
        let defs: Vec<Definition> = parse_ok(src)
            .into_iter()
            .map(|m| match m {
                Member::Definition(d) => d,
                other => panic!("expected a definition, got {other:?}"),
            })
            .collect();
        let kinds: Vec<Kind> = defs.iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            [Kind::Part, Kind::Attribute, Kind::Port, Kind::Requirement]
        );
        assert_eq!(defs[0].name.as_deref(), Some("Vehicle"));
        assert!(!defs[0].is_abstract);
        assert!(defs[1].is_abstract);
        assert_eq!(defs[1].short_name.as_deref(), Some("A1"));
        assert_eq!(names(&defs[1].specializes), ["Real", "Base::Quantity"]);
        assert_eq!(names(&defs[2].specializes), ["Flow"]);
        assert_eq!(defs[2].members.len(), 1);
        assert_eq!(defs[3].short_name.as_deref(), Some("R"));
        assert!(matches!(defs[3].members[0], Member::Usage(_)));
        assert_eq!(defs[3].pos, Pos { line: 4, column: 1 });
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_usages_of_each_kind() {
        let src = "part car : Vehicle { part wheel [4] : Wheel; }\n\
                   attribute mass : Mass;\n\
                   in port fuelIn : Fuel;\n\
                   requirement <'CT-EFF-001'> size : ProductRequirement { subject s : Vehicle; }";
        let members = parse_ok(src);
        let usages: Vec<&Usage> = members
            .iter()
            .map(|m| match m {
                Member::Usage(u) => u,
                other => panic!("expected a usage, got {other:?}"),
            })
            .collect();
        let kinds: Vec<UsageKind> = usages.iter().map(|u| u.kind).collect();
        assert_eq!(
            kinds,
            [
                UsageKind::Of(Kind::Part),
                UsageKind::Of(Kind::Attribute),
                UsageKind::Of(Kind::Port),
                UsageKind::Of(Kind::Requirement)
            ]
        );
        let Member::Usage(wheel) = &usages[0].members[0] else {
            panic!("expected a usage")
        };
        assert_eq!(wheel.multiplicity.as_deref(), Some("4"));
        assert_eq!(names(&wheel.types), ["Wheel"]);
        assert_eq!(usages[2].direction, Some(Direction::In));
        assert_eq!(usages[3].short_name.as_deref(), Some("CT-EFF-001"));
        assert_eq!(usages[3].name.as_deref(), Some("size"));
        let Member::Usage(subject) = &usages[3].members[0] else {
            panic!("expected a usage")
        };
        assert_eq!(subject.kind, UsageKind::Subject);
        assert_eq!(subject.name.as_deref(), Some("s"));
        assert_eq!(members[3].pos(), Pos { line: 4, column: 1 });
        assert_eq!(members[3].members().len(), 1);
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_usage_relationships_in_any_order() {
        let u = usage("out abstract part p : A, B [0..*] :> q subsets r :>> s redefines t;");
        assert_eq!(u.direction, Some(Direction::Out));
        assert!(u.is_abstract);
        assert_eq!(u.multiplicity.as_deref(), Some("0..*"));
        assert_eq!(names(&u.types), ["A", "B"]);
        assert_eq!(names(&u.subsets), ["q", "r"]);
        assert_eq!(names(&u.redefines), ["s", "t"]);
        let u = usage("inout attribute x defined by T;");
        assert_eq!(u.direction, Some(Direction::InOut));
        assert_eq!(names(&u.types), ["T"]);
        assert_eq!(u.effective_name(), Some("x"));
        let u = usage("attribute :>> verificationMethod = \"Test\";");
        assert_eq!(u.name, None);
        assert_eq!(u.effective_name(), Some("verificationMethod"));
        assert_eq!(usage("part;").effective_name(), None);
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_feature_values() {
        let value = |src: &str| usage(src).value.unwrap();
        let cases = [
            (
                "attribute a = \"say \\\"hi\\\"\";",
                ValueKind::String,
                "say \"hi\"",
            ),
            ("attribute a := 42;", ValueKind::Integer, "42"),
            ("attribute a default 2.5;", ValueKind::Real, "2.5"),
            ("attribute a = 'red light';", ValueKind::Name, "red light"),
            ("attribute a = true;", ValueKind::Boolean, "true"),
            ("attribute a = false;", ValueKind::Boolean, "false"),
            (
                "attribute a = Colors::red;",
                ValueKind::Expression,
                "Colors::red",
            ),
            (
                "attribute a default = 1 + 2 { }",
                ValueKind::Expression,
                "1 + 2",
            ),
            ("attribute a = null;", ValueKind::Expression, "null"),
        ];
        for (src, kind, text) in cases {
            let v = value(src);
            assert_eq!((v.kind, v.text.as_str()), (kind, text), "{src}");
        }
        assert!(!value("attribute a = 1;").is_default);
        assert!(value("attribute a default 1;").is_default);
        assert!(value("attribute a default := 1;").is_default);
        assert_eq!(
            value("attribute a = 1;").pos,
            Pos {
                line: 1,
                column: 15
            }
        );
    }

    // @verifies CT-MOD-002
    #[test]
    fn parses_satisfy_and_verify() {
        let src = "satisfy requirement Reqs::r1 by car;\n\
                   satisfy r2 { doc /* why */ }\n\
                   verify requirement r1;\n\
                   verify r2 { }";
        let members = parse_ok(src);
        let Member::Satisfy(s1) = &members[0] else {
            panic!("expected satisfy")
        };
        assert_eq!(s1.requirement.joined(), "Reqs::r1");
        assert_eq!(
            s1.by.as_ref().map(QualifiedName::joined).as_deref(),
            Some("car")
        );
        assert_eq!(
            s1.requirement.pos,
            Pos {
                line: 1,
                column: 21
            }
        );
        let Member::Satisfy(s2) = &members[1] else {
            panic!("expected satisfy")
        };
        assert_eq!(s2.by, None);
        assert_eq!(s2.members.len(), 1);
        let Member::Verify(v1) = &members[2] else {
            panic!("expected verify")
        };
        assert_eq!(v1.requirement.joined(), "r1");
        let Member::Verify(v2) = &members[3] else {
            panic!("expected verify")
        };
        assert_eq!(v2.requirement.joined(), "r2");
        assert_eq!(members[3].pos(), Pos { line: 4, column: 1 });
        assert!(members[3].members().is_empty());
        assert!(members[1].members().len() == 1 && members[0].members().is_empty());
    }

    // @verifies CT-TRC-002
    #[test]
    fn parses_derivation_refinement_and_evidence_dependencies() {
        let src = "#derivation dependency child to parent;\n\
                   #refinement dependency <'D1'> design from a, P::b to spec, other { doc /* why */ }\n\
                   #evidence dependency from report to r1;";
        let members = parse_ok(src);
        let Member::Dependency(d1) = &members[0] else {
            panic!("expected a dependency")
        };
        assert_eq!(d1.kind, DependencyKind::Derivation);
        assert_eq!((d1.short_name.as_deref(), d1.name.as_deref()), (None, None));
        assert_eq!(names(&d1.clients), ["child"]);
        assert_eq!(names(&d1.suppliers), ["parent"]);
        assert_eq!(members[0].pos(), Pos { line: 1, column: 1 });
        let Member::Dependency(d2) = &members[1] else {
            panic!("expected a dependency")
        };
        assert_eq!(d2.kind, DependencyKind::Refinement);
        assert_eq!(d2.short_name.as_deref(), Some("D1"));
        assert_eq!(d2.name.as_deref(), Some("design"));
        assert_eq!(names(&d2.clients), ["a", "P::b"]);
        assert_eq!(names(&d2.suppliers), ["spec", "other"]);
        assert_eq!(members[1].members().len(), 1);
        let Member::Dependency(d3) = &members[2] else {
            panic!("expected a dependency")
        };
        assert_eq!(d3.kind, DependencyKind::Evidence);
        assert_eq!(names(&d3.clients), ["report"]);
        assert_eq!(d3.name, None);
    }

    // @verifies CT-TRC-002, CT-USE-007
    #[test]
    fn reports_dependencies_without_a_supported_kind_or_ends() {
        assert_eq!(
            errors("dependency a to b;\npart ok;"),
            [(
                1,
                1,
                "a dependency needs `#derivation`, `#refinement`, or `#evidence` before it"
                    .to_string()
            )]
        );
        assert_eq!(
            errors("#trace dependency a to b;"),
            [(1, 2, "`#trace` is not supported".to_string())]
        );
        assert_eq!(
            errors("#refinement part a;"),
            [(
                1,
                13,
                "expected `dependency` after `#refinement`, found `part`".to_string()
            )]
        );
        assert_eq!(
            errors("# dependency a to b;"),
            [(
                1,
                3,
                "expected a metadata name after `#`, found `dependency`".to_string()
            )]
        );
        assert_eq!(
            errors("#evidence dependency a b;"),
            [(
                1,
                24,
                "expected `to` after the dependency's sources, found `b`".to_string()
            )]
        );
        assert_eq!(
            errors("#evidence dependency a to;"),
            [(1, 26, "expected a name after `to`, found `;`".to_string())]
        );
        assert_eq!(
            errors("#evidence dependency to b;"),
            [(
                1,
                22,
                "expected a name after `dependency`, found `to`".to_string()
            )]
        );
        assert_eq!(
            errors("#evidence dependency d from to b;"),
            [(
                1,
                29,
                "expected a name after `from`, found `to`".to_string()
            )]
        );
        assert_eq!(
            errors("#evidence dependency a to b\npart c;"),
            [(
                2,
                1,
                "expected `;` or `{` after the dependency, found `part`".to_string()
            )]
        );
        let parsed = parse("#trace dependency a to b;\npart ok;");
        assert_eq!(parsed.members.len(), 1);
        // A missing `;` before a dependency does not hide it.
        let parsed = parse("part a\n#evidence dependency x to y;");
        assert_eq!(parsed.errors.len(), 1);
        assert!(matches!(parsed.members[1], Member::Dependency(_)));
    }

    // @verifies CT-MOD-002
    #[test]
    fn member_accessors_cover_every_kind() {
        let src = "package P; import A; doc /* d */ part def D; part u; satisfy r; verify r; \
                   #evidence dependency a to b;";
        let members = parse_ok(src);
        let columns: Vec<u32> = members.iter().map(|m| m.pos().column).collect();
        assert_eq!(columns, [1, 12, 22, 34, 46, 54, 65, 75]);
        assert!(members.iter().all(|m| m.members().is_empty()));
    }

    // @verifies CT-MOD-002, CT-MOD-001
    #[test]
    fn parses_the_project_requirements_without_errors() {
        let src = include_str!("../../../../model/requirements.sysml");
        let members = parse_ok(src);
        let Member::Package(root) = &members[0] else {
            panic!("expected the root package")
        };
        assert_eq!(root.name.as_deref(), Some("ClewtraceRequirements"));
        assert!(matches!(root.members[1], Member::Import(_)));
    }

    // @verifies CT-MOD-002, CT-TRC-002
    #[test]
    fn parses_metadata_definitions_with_an_optional_short_name() {
        let src = "metadata def <derivation> Derivation;\n\
                   metadata def Plain { doc /* d */ }";
        let defs: Vec<Definition> = parse_ok(src)
            .into_iter()
            .map(|m| match m {
                Member::Definition(d) => d,
                other => panic!("expected a definition, got {other:?}"),
            })
            .collect();
        assert!(defs.iter().all(|d| d.kind == Kind::Metadata));
        assert_eq!(defs[0].short_name.as_deref(), Some("derivation"));
        assert_eq!(defs[0].name.as_deref(), Some("Derivation"));
        assert_eq!(defs[1].short_name, None);
        assert_eq!(defs[1].members.len(), 1);
    }

    // @verifies CT-MOD-002, CT-USE-007
    #[test]
    fn reports_metadata_usages_as_unsupported_and_recovers() {
        let parsed = parse("metadata m : Derivation about x;\npart ok;\n");
        let found: Vec<(u32, u32, &str, &str)> = (parsed.errors.iter())
            .map(|e| (e.line, e.column, e.message.as_str(), e.help))
            .collect();
        assert_eq!(
            found,
            [(
                1,
                1,
                "`metadata` is supported only in `metadata def`",
                SUPPORTED
            )]
        );
        assert_eq!(parsed.members.len(), 1);
        assert!(matches!(&parsed.members[0], Member::Usage(u) if u.name.as_deref() == Some("ok")));
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_unsupported_and_unexpected_members_and_recovers() {
        let src = "action a { part x; }\nfoo bar\npart ok;\n42;\nprivate /* c */\npart b;\nprivate";
        let parsed = parse(src);
        let found: Vec<(u32, u32, &str)> = parsed
            .errors
            .iter()
            .map(|e| (e.line, e.column, e.message.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                (1, 1, "`action` is not supported yet"),
                (
                    2,
                    1,
                    "expected a member such as `part` or `package`, found `foo`"
                ),
                (
                    4,
                    1,
                    "expected a member such as `part` or `package`, found `42`"
                ),
                (
                    5,
                    9,
                    "expected a member such as `part` or `package`, found a comment"
                ),
                (
                    7,
                    8,
                    "expected a member such as `part` or `package`, found the end of the file"
                ),
            ]
        );
        assert!(parsed.errors.iter().all(|e| e.help == SUPPORTED));
        let names: Vec<&str> = parsed
            .members
            .iter()
            .filter_map(|m| match m {
                Member::Usage(u) => u.name.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["ok", "b"]);
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_unbalanced_braces() {
        assert_eq!(
            errors("part a { part b;"),
            [(1, 8, "`{` is never closed".to_string())]
        );
        assert_eq!(
            errors("action a { { }"),
            [
                (1, 1, "`action` is not supported yet".to_string()),
                (1, 10, "`{` is never closed".to_string())
            ]
        );
        assert_eq!(
            errors("part a; }\npart b;"),
            [(1, 9, "unexpected `}` with no matching `{`".to_string())]
        );
        // A skipped member stops before the `}` that closes its parent.
        let parsed = parse("part a { action x }\npart b;");
        assert_eq!(parsed.errors.len(), 1);
        assert_eq!(parsed.members.len(), 2);
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_missing_semicolons_and_bodies() {
        assert_eq!(
            errors("import A\npart b;"),
            [(
                2,
                1,
                "expected `;` after the import, found `part`".to_string()
            )]
        );
        assert_eq!(
            errors("import A B;\npart c;"),
            [(
                1,
                10,
                "expected `;` after the import, found `B`".to_string()
            )]
        );
        assert_eq!(
            errors("part def A\npart b;"),
            [(
                2,
                1,
                "expected `;` or `{` after the definition, found `part`".to_string()
            )]
        );
        // The stray `}` is reported once, not again as unmatched.
        assert_eq!(
            errors("package P }"),
            [(
                1,
                11,
                "expected `;` or `{` after the package name, found `}`".to_string()
            )]
        );
        assert_eq!(
            errors("part x 5; part y;"),
            [(
                1,
                8,
                "expected `;` or `{` after the usage, found `5`".to_string()
            )]
        );
        assert_eq!(
            errors("satisfy r by a\n"),
            [(
                2,
                1,
                "expected `;` or `{` after the satisfy, found the end of the file".to_string()
            )]
        );
        assert_eq!(
            errors("verify r )"),
            [(
                1,
                10,
                "expected `;` or `{` after the verify, found `)`".to_string()
            )]
        );
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_missing_names() {
        let cases = [
            ("import ;", "expected a name to import, found `;`"),
            ("import all *;", "expected a name to import, found `*`"),
            ("part def A :> ;", "expected a name after `:>`, found `;`"),
            ("part a : A, ;", "expected a name after `,`, found `;`"),
            (
                "part a defined by ;",
                "expected a name after `by`, found `;`",
            ),
            ("satisfy ;", "expected a name after `satisfy`, found `;`"),
            (
                "satisfy requirement {}",
                "expected a name after `requirement`, found `{`",
            ),
            ("satisfy r by ;", "expected a name after `by`, found `;`"),
            ("verify ;", "expected a name after `verify`, found `;`"),
            (
                "comment about ; ",
                "expected a name after `about`, found `;`",
            ),
        ];
        for (src, message) in cases {
            let found = errors(src);
            assert_eq!(found.len(), 1, "{src}: {found:?}");
            assert_eq!(found[0].2, message, "{src}");
        }
        let parsed = parse("import ;\nsatisfy ;\nverify ;");
        assert!(parsed.errors.iter().all(|e| !e.help.is_empty()));
        assert!(parsed.members.is_empty());
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_bad_short_names_multiplicities_values_and_comments() {
        let cases = [
            (
                "part <> a;",
                7,
                "expected a short name like <'CT-EFF-001'>, found `>`",
            ),
            (
                "part <'x' a;",
                11,
                "expected a short name like <'CT-EFF-001'>, found `a`",
            ),
            (
                "part a [1..* : A;",
                17,
                "expected `]` to close the multiplicity, found `;`",
            ),
            (
                "part a [1",
                10,
                "expected `]` to close the multiplicity, found the end of the file",
            ),
            (
                "attribute a = ;",
                15,
                "expected a value after `=`, found `;`",
            ),
            (
                "attribute a default {}",
                21,
                "expected a value after `default`, found `{`",
            ),
            (
                "doc note;",
                9,
                "expected the text of the doc as /* ... */, found `;`",
            ),
            (
                "comment c about x part y;",
                19,
                "expected the text of the comment as /* ... */, found `part`",
            ),
        ];
        for (src, column, message) in cases {
            let found = errors(src);
            assert_eq!(found, [(1, column, message.to_string())], "{src}");
        }
        // The broken short name, multiplicity, or value does not lose the usage.
        assert_eq!(parse("part a [1 }").members.len(), 1);
        assert!(matches!(
            parse("doc x part y;").members[..],
            [Member::Usage(_)]
        ));
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_misplaced_modifiers() {
        let cases = [
            ("in part def A;", "`in` cannot be used before `part`"),
            ("out package P;", "`out` cannot be used before `package`"),
            (
                "abstract import A;",
                "`abstract` cannot be used before `import`",
            ),
        ];
        for (src, message) in cases {
            let parsed = parse(src);
            let found: Vec<&str> = parsed.errors.iter().map(|e| e.message.as_str()).collect();
            assert_eq!(found, [message], "{src}");
            assert_eq!(parsed.errors[0].column, 1);
            assert_eq!(parsed.members.len(), 1, "{src}");
        }
        assert!(
            parse("abstract part def A; abstract part a; in subject s;")
                .errors
                .is_empty()
        );
    }

    // @verifies CT-USE-007
    #[test]
    fn merges_lexer_errors_without_repeating_them() {
        let src = "part Größe;\npart a = \"open\npart b; § part c;";
        let parsed = parse(src);
        let found: Vec<(u32, u32)> = parsed.errors.iter().map(|e| (e.line, e.column)).collect();
        assert_eq!(found, [(1, 6), (2, 10), (3, 9)]);
        assert!(parsed.errors[0].help.contains("single quotes"));
        let names: Vec<Option<&str>> = parsed
            .members
            .iter()
            .map(|m| match m {
                Member::Usage(u) => u.name.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(names, [None, Some("a"), Some("b"), Some("c")]);
    }

    // @verifies CT-USE-007
    #[test]
    fn recovers_from_every_truncation() {
        // Every prefix of a file is a broken file; parsing each must end and keep the
        // members that are complete.
        let src = "package P { private import A::*::**; doc /* d */
                   part def <'D'> V :> T { attribute :>> m default := 1 + 2; }
                   in part w [0..*] : V subsets x redefines y = 'z';
                   comment c about a, b /* c */ satisfy requirement R by w; verify R; }";
        for end in 0..=src.len() {
            if src.is_char_boundary(end) {
                let parsed = parse(&src[..end]);
                assert!(parsed.members.len() <= 1, "{end}");
            }
        }
        assert!(parse(src).errors.is_empty());
    }

    // @verifies CT-USE-007
    #[test]
    fn errors_render_with_path_line_and_column() {
        let parsed = parse("part a\n  part def ;\n");
        let rendered: Vec<String> = parsed
            .errors
            .iter()
            .map(|e| e.render("model/x.sysml"))
            .collect();
        assert_eq!(
            rendered[0],
            "model/x.sysml:2:3: error: expected `;` or `{` after the usage, found `part`\n  \
             help: end the declaration with `;`, or open a body with `{`"
        );
        assert_eq!(
            parsed.errors[0].to_string(),
            "2:3: error: expected `;` or `{` after the usage, found `part`\n  \
             help: end the declaration with `;`, or open a body with `{`"
        );
        let from_lexer = SyntaxError::from(LexError {
            line: 1,
            column: 2,
            message: "m".into(),
            help: "h",
        });
        assert_eq!(from_lexer.render("f"), "f:1:2: error: m\n  help: h");
    }
}
