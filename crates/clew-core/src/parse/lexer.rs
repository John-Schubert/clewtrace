//! SysML v2 lexer for the CT-MOD-002 subset, following the KerML lexical grammar.
//!
//! Tokens keep byte spans into the source instead of copies, plus a 1-based line and column.
//! Columns count Unicode characters, so a tab or a `ß` is one column. `//` and `//* */` notes
//! are skipped; regular comments `/* */` are tokens because `doc` and `comment` use them.
//! The lexer never stops at an error: it records the error and carries on, so the parser can
//! report every problem in one pass.

use std::borrow::Cow;
use std::fmt;

// @implements CT-MOD-002
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A reserved word such as `package` or `requirement`.
    Keyword,
    /// A basic name: an ASCII letter or `_`, then ASCII letters, digits, or `_`.
    Name,
    /// A name in single quotes, such as `'CT-EFF-001'`.
    UnrestrictedName,
    Integer,
    /// A number with a fraction or an exponent, such as `2.5` or `1e5`.
    Real,
    String,
    /// A regular comment `/* ... */`, the body of `doc` and `comment`.
    Comment,
    /// Punctuation or an operator, such as `{`, `::`, or `:>>`.
    Symbol,
    /// Text that is not a valid token. A matching entry is in `Lexed::errors`.
    Error,
    /// End of input. Always the last token.
    Eof,
}

// @implements CT-MOD-002, CT-USE-007
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Byte offset of the first byte.
    pub start: usize,
    /// Byte offset one past the last byte.
    pub end: usize,
    /// 1-based line of the first character.
    pub line: u32,
    /// 1-based column of the first character, counted in characters.
    pub column: u32,
}

// @implements CT-MOD-002
impl Token {
    /// The token's source text.
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }

    /// The token's meaning: a quoted name or string without quotes and with escapes
    /// resolved, or a comment's body without `/*` and `*/`. Other tokens return their text.
    pub fn value<'a>(&self, src: &'a str) -> Cow<'a, str> {
        let text = self.text(src);
        match self.kind {
            TokenKind::UnrestrictedName | TokenKind::String => unescape(&text[1..text.len() - 1]),
            TokenKind::Comment => Cow::Borrowed(&text[2..text.len() - 2]),
            _ => Cow::Borrowed(text),
        }
    }

    /// True when this is the keyword or symbol `word`.
    pub fn is(&self, word: &str, src: &str) -> bool {
        matches!(self.kind, TokenKind::Keyword | TokenKind::Symbol) && self.text(src) == word
    }
}

// @implements CT-USE-007
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub help: &'static str,
}

// @implements CT-USE-007
impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: error: {}\n  help: {}",
            self.line, self.column, self.message, self.help
        )
    }
}

// @implements CT-MOD-002, CT-USE-007
#[derive(Debug, Default)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub errors: Vec<LexError>,
}

// @implements CT-MOD-002
/// SysML v2 reserved words, sorted for binary search. They can be used as names only in quotes.
pub const KEYWORDS: &[&str] = &[
    "about",
    "abstract",
    "accept",
    "action",
    "actor",
    "after",
    "alias",
    "all",
    "allocate",
    "allocation",
    "analysis",
    "and",
    "as",
    "assert",
    "assign",
    "assume",
    "at",
    "attribute",
    "bind",
    "binding",
    "by",
    "calc",
    "case",
    "comment",
    "concern",
    "connect",
    "connection",
    "constant",
    "constraint",
    "crosses",
    "decide",
    "def",
    "default",
    "defined",
    "dependency",
    "derived",
    "do",
    "doc",
    "else",
    "end",
    "entry",
    "enum",
    "event",
    "exhibit",
    "exit",
    "expose",
    "false",
    "filter",
    "first",
    "flow",
    "for",
    "fork",
    "frame",
    "from",
    "hastype",
    "if",
    "implies",
    "import",
    "in",
    "include",
    "individual",
    "inout",
    "interface",
    "istype",
    "item",
    "join",
    "language",
    "library",
    "locale",
    "loop",
    "merge",
    "message",
    "meta",
    "metadata",
    "nonunique",
    "not",
    "null",
    "objective",
    "occurrence",
    "of",
    "or",
    "ordered",
    "out",
    "package",
    "parallel",
    "part",
    "perform",
    "port",
    "private",
    "protected",
    "public",
    "redefines",
    "ref",
    "references",
    "render",
    "rendering",
    "rep",
    "require",
    "requirement",
    "return",
    "satisfy",
    "send",
    "snapshot",
    "specializes",
    "stakeholder",
    "standard",
    "state",
    "subject",
    "subsets",
    "succession",
    "terminate",
    "then",
    "timeslice",
    "to",
    "transition",
    "true",
    "until",
    "use",
    "variant",
    "variation",
    "verification",
    "verify",
    "via",
    "view",
    "viewpoint",
    "when",
    "while",
    "xor",
];

// @implements CT-MOD-002
/// Symbols, longest first so the first match is the longest.
const SYMBOLS: &[&str] = &[
    ":>>", "::>", "!==", "===", "...", "::", ":>", ":=", "..", ".?", "->", "@@", "**", "!=", "==",
    "<=", ">=", "??", "{", "}", "(", ")", "[", "]", ";", ",", ":", "=", ".", "@", "#", "~", "?",
    "!", "<", ">", "+", "-", "*", "/", "%", "^", "|", "&",
];

// @implements CT-MOD-002, CT-USE-007
/// Splits SysML v2 text into tokens. Errors do not stop lexing; see `Lexed::errors`.
pub fn lex(src: &str) -> Lexed {
    let mut lexer = Lexer {
        src,
        pos: 0,
        line: 1,
        column: 1,
        out: Lexed {
            tokens: Vec::with_capacity(src.len() / 6),
            errors: Vec::new(),
        },
    };
    if src.starts_with('\u{feff}') {
        lexer.pos = '\u{feff}'.len_utf8();
    }
    lexer.run();
    lexer.out
}

// @implements CT-MOD-002
fn unescape(body: &str) -> Cow<'_, str> {
    if !body.contains('\\') {
        return Cow::Borrowed(body);
    }
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('b') => out.push('\u{8}'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('f') => out.push('\u{c}'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    Cow::Owned(out)
}

// @implements CT-MOD-002, CT-USE-007
/// Position of a character in the source: byte offset, line, and column.
#[derive(Clone, Copy)]
struct Mark {
    pos: usize,
    line: u32,
    column: u32,
}

// @implements CT-MOD-002, CT-USE-007
struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: u32,
    column: u32,
    out: Lexed,
}

// @implements CT-MOD-002, CT-USE-007
impl Lexer<'_> {
    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.mark();
            match c {
                ' ' | '\t' | '\u{c}' | '\r' | '\n' => {
                    self.bump();
                }
                '/' if self.byte_at(1) == Some(b'/') => self.note(start),
                '/' if self.byte_at(1) == Some(b'*') => self.comment(start),
                '\'' => self.quoted(start, '\'', TokenKind::UnrestrictedName),
                '"' => self.quoted(start, '"', TokenKind::String),
                '0'..='9' => self.number(start),
                c if c == '_' || c.is_alphabetic() => self.word(start),
                _ => self.symbol(start, c),
            }
        }
        let end = self.mark();
        self.push(TokenKind::Eof, end);
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn byte_at(&self, offset: usize) -> Option<u8> {
        self.src.as_bytes().get(self.pos + offset).copied()
    }

    fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            line: self.line,
            column: self.column,
        }
    }

    fn reset(&mut self, to: Mark) {
        self.pos = to.pos;
        self.line = to.line;
        self.column = to.column;
    }

    /// Consumes one character. CRLF, LF, and a lone CR each end a line.
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        let ends_line = c == '\n' || (c == '\r' && self.byte_at(0) != Some(b'\n'));
        if ends_line {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn bump_while(&mut self, keep: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&keep) {
            self.bump();
        }
    }

    /// Consumes through the next `*/`. Returns false at end of input.
    fn bump_past_comment_end(&mut self) -> bool {
        while let Some(c) = self.bump() {
            if c == '*' && self.byte_at(0) == Some(b'/') {
                self.bump();
                return true;
            }
        }
        false
    }

    fn push(&mut self, kind: TokenKind, start: Mark) {
        self.out.tokens.push(Token {
            kind,
            start: start.pos,
            end: self.pos,
            line: start.line,
            column: start.column,
        });
    }

    fn error(&mut self, at: Mark, message: String, help: &'static str) {
        self.out.errors.push(LexError {
            line: at.line,
            column: at.column,
            message,
            help,
        });
    }

    /// `// note` to end of line, or `//* note */`. Notes are not tokens.
    fn note(&mut self, start: Mark) {
        self.bump();
        self.bump();
        if self.byte_at(0) == Some(b'*') {
            self.bump();
            if !self.bump_past_comment_end() {
                self.error(
                    start,
                    "unterminated note".to_string(),
                    "close the note with `*/`",
                );
            }
        } else {
            self.bump_while(|c| c != '\n' && c != '\r');
        }
    }

    /// `/* comment */`. An unclosed comment runs to the end of input.
    fn comment(&mut self, start: Mark) {
        self.bump();
        self.bump();
        if self.bump_past_comment_end() {
            self.push(TokenKind::Comment, start);
        } else {
            self.error(
                start,
                "unterminated comment".to_string(),
                "close the comment with `*/`",
            );
            self.push(TokenKind::Error, start);
        }
    }
}

// @implements CT-MOD-002, CT-USE-007
impl Lexer<'_> {
    /// A quoted name or string. Both may span lines. When the closing quote is missing,
    /// lexing resumes at the end of the opening line so one stray quote does not swallow
    /// the rest of the file.
    fn quoted(&mut self, start: Mark, quote: char, kind: TokenKind) {
        self.bump();
        let mut bad_escapes = Vec::new();
        let mut line_end = None;
        loop {
            let here = self.mark();
            match self.bump() {
                None => {
                    self.reset(line_end.unwrap_or(here));
                    let (what, help) = if quote == '"' {
                        ("string", "close the string with `\"`")
                    } else {
                        ("quoted name", "close the name with `'`")
                    };
                    self.error(start, format!("unterminated {what}"), help);
                    self.push(TokenKind::Error, start);
                    return;
                }
                Some(c) if c == quote => break,
                Some('\n' | '\r') => {
                    line_end.get_or_insert(here);
                }
                Some('\\') => match self.peek() {
                    Some('b' | 't' | 'n' | 'f' | 'r' | '"' | '\'' | '\\') => {
                        self.bump();
                    }
                    other => bad_escapes.push((here, other)),
                },
                Some(_) => {}
            }
        }
        if bad_escapes.is_empty() {
            self.push(kind, start);
            return;
        }
        for (at, c) in bad_escapes {
            let shown = c.map(String::from).unwrap_or_default();
            self.error(
                at,
                format!("invalid escape sequence `\\{shown}`"),
                "use one of \\b \\t \\n \\f \\r \\\" \\' \\\\, or write \\\\ for a backslash",
            );
        }
        self.push(TokenKind::Error, start);
    }

    /// An integer, or a real with a fraction and/or an exponent. `1..2` stays a range.
    fn number(&mut self, start: Mark) {
        let digit = |c: char| c.is_ascii_digit();
        self.bump_while(digit);
        let mut kind = TokenKind::Integer;
        if self.byte_at(0) == Some(b'.') && self.byte_at(1).is_some_and(|b| b.is_ascii_digit()) {
            self.bump();
            self.bump_while(digit);
            kind = TokenKind::Real;
        }
        if matches!(self.byte_at(0), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.byte_at(1), Some(b'+' | b'-')));
            if self.byte_at(1 + sign).is_some_and(|b| b.is_ascii_digit()) {
                for _ in 0..=sign {
                    self.bump();
                }
                self.bump_while(digit);
                kind = TokenKind::Real;
            }
        }
        self.push(kind, start);
    }

    /// A keyword or basic name. A word with non-ASCII letters is one error, not one per letter.
    fn word(&mut self, start: Mark) {
        self.bump_while(|c| c.is_alphanumeric() || c == '_');
        let text = &self.src[start.pos..self.pos];
        let kind = if !text.is_ascii() {
            self.error(
                start,
                format!("name `{text}` has characters other than ASCII letters, digits, and `_`"),
                "put the name in single quotes, like 'Größe'",
            );
            TokenKind::Error
        } else if KEYWORDS.binary_search(&text).is_ok() {
            TokenKind::Keyword
        } else {
            TokenKind::Name
        };
        self.push(kind, start);
    }

    fn symbol(&mut self, start: Mark, c: char) {
        let rest = &self.src[self.pos..];
        if let Some(symbol) = SYMBOLS.iter().find(|s| rest.starts_with(**s)) {
            for _ in 0..symbol.len() {
                self.bump();
            }
            self.push(TokenKind::Symbol, start);
            return;
        }
        self.bump();
        let (message, help) = if c.is_whitespace() {
            (
                format!("unexpected whitespace character U+{:04X}", u32::from(c)),
                "replace it with a plain space; it often comes from text pasted from a word processor",
            )
        } else if c.is_control() {
            (
                format!("unexpected control character U+{:04X}", u32::from(c)),
                "delete it; it is invisible in most editors",
            )
        } else {
            (
                format!("unexpected character `{c}`"),
                "delete it, or put it inside a string \"...\" or a quoted name '...'",
            )
        };
        self.error(start, message, help);
        self.push(TokenKind::Error, start);
    }
}

// @verifies CT-MOD-002, CT-USE-007
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-MOD-002
    fn kinds_and_texts(src: &str) -> Vec<(TokenKind, String)> {
        let lexed = lex(src);
        assert!(lexed.errors.is_empty(), "{:?}", lexed.errors);
        lexed
            .tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| (t.kind, t.text(src).to_string()))
            .collect()
    }

    // @verifies CT-MOD-002
    #[test]
    fn lexes_every_construct_of_the_initial_subset() {
        let src = "package P { private import Q::*; doc /* d */ comment c about x /* c */\n\
               part def A; part a : A; attribute w : Real; port def Pt; port p : Pt;\n\
               requirement def R; requirement <'R-1'> r : R { subject s; }\n\
               satisfy r by a; verify r; }";
        use TokenKind::*;
        let got = kinds_and_texts(src);
        let texts: Vec<&str> = got.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(
            &texts[..12],
            &[
                "package", "P", "{", "private", "import", "Q", "::", "*", ";", "doc", "/* d */",
                "comment"
            ]
        );
        for word in [
            "package",
            "private",
            "import",
            "doc",
            "comment",
            "about",
            "part",
            "def",
            "attribute",
            "port",
            "requirement",
            "subject",
            "satisfy",
            "by",
            "verify",
        ] {
            assert!(
                got.contains(&(Keyword, word.to_string())),
                "{word} is a keyword"
            );
        }
        for name in [
            "P", "Q", "A", "a", "Real", "Pt", "R", "r", "s", "c", "x", "w", "p",
        ] {
            assert!(got.contains(&(Name, name.to_string())), "{name} is a name");
        }
        assert!(got.contains(&(UnrestrictedName, "'R-1'".to_string())));
        assert!(got.contains(&(Comment, "/* c */".to_string())));
        assert!(got.iter().all(|(k, _)| *k != Error));
    }

    // @verifies CT-MOD-002, CT-USE-007
    #[test]
    fn reports_exact_line_and_column() {
        // Tabs and non-ASCII letters count as one column each; CRLF and lone CR end a line.
        let src = "part\r\n\tdef /* Größe */ x;\rport\n  'a' \"s\"";
        let lexed = lex(src);
        assert!(lexed.errors.is_empty());
        let pos: Vec<(String, u32, u32)> = lexed
            .tokens
            .iter()
            .map(|t| (t.text(src).to_string(), t.line, t.column))
            .collect();
        assert_eq!(
            pos,
            vec![
                ("part".into(), 1, 1),
                ("def".into(), 2, 2),
                ("/* Größe */".into(), 2, 6),
                ("x".into(), 2, 18),
                (";".into(), 2, 19),
                ("port".into(), 3, 1),
                ("'a'".into(), 4, 3),
                ("\"s\"".into(), 4, 7),
                ("".into(), 4, 10),
            ]
        );
        assert_eq!(lexed.tokens.last().unwrap().kind, TokenKind::Eof);
    }

    // @verifies CT-MOD-002
    #[test]
    fn skips_notes_and_keeps_regular_comments() {
        let src = "a // note /* not a comment\nb //* multi\nline note */ c /* kept\n * body */";
        use TokenKind::*;
        assert_eq!(
            kinds_and_texts(src),
            vec![
                (Name, "a".into()),
                (Name, "b".into()),
                (Name, "c".into()),
                (Comment, "/* kept\n * body */".into()),
            ]
        );
        let lexed = lex(src);
        assert_eq!(lexed.tokens[3].value(src), " kept\n * body ");
        assert_eq!((lexed.tokens[2].line, lexed.tokens[2].column), (3, 14));
    }

    // @verifies CT-MOD-002
    #[test]
    fn unescapes_names_and_strings() {
        let src = r#"'CT-EFF-001' 'it\'s' "say \"hi\"\t\\" plain"#;
        let lexed = lex(src);
        assert!(lexed.errors.is_empty());
        let values: Vec<String> = lexed
            .tokens
            .iter()
            .map(|t| t.value(src).into_owned())
            .collect();
        assert_eq!(
            values,
            vec!["CT-EFF-001", "it's", "say \"hi\"\t\\", "plain", ""]
        );
        assert!(matches!(lexed.tokens[0].value(src), Cow::Borrowed(_)));
        assert_eq!(unescape(r"\b\t\n\f\r\q"), "\u{8}\t\n\u{c}\rq");
        // A trailing backslash cannot come from the lexer, which treats `\'` as an escape.
        assert_eq!(unescape("a\\"), "a");
    }

    // @verifies CT-MOD-002
    #[test]
    fn lexes_numbers_without_eating_ranges() {
        use TokenKind::*;
        assert_eq!(
            kinds_and_texts("0 42 3.14 1e5 2.5E-3 [1..2] x.y 7.e"),
            vec![
                (Integer, "0".into()),
                (Integer, "42".into()),
                (Real, "3.14".into()),
                (Real, "1e5".into()),
                (Real, "2.5E-3".into()),
                (Symbol, "[".into()),
                (Integer, "1".into()),
                (Symbol, "..".into()),
                (Integer, "2".into()),
                (Symbol, "]".into()),
                (Name, "x".into()),
                (Symbol, ".".into()),
                (Name, "y".into()),
                (Integer, "7".into()),
                (Symbol, ".".into()),
                (Name, "e".into()),
            ]
        );
        assert_eq!(
            kinds_and_texts("1e 2E+"),
            vec![
                (Integer, "1".into()),
                (Name, "e".into()),
                (Integer, "2".into()),
                (Name, "E".into()),
                (Symbol, "+".into()),
            ]
        );
    }

    // @verifies CT-MOD-002
    #[test]
    fn matches_the_longest_symbol() {
        let got = kinds_and_texts(":>> ::> :: :> := : -> .. . <'x'> !== != == === @@ @ ** *");
        let texts: Vec<&str> = got.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                ":>>", "::>", "::", ":>", ":=", ":", "->", "..", ".", "<", "'x'", ">", "!==", "!=",
                "==", "===", "@@", "@", "**", "*"
            ]
        );
    }

    // @verifies CT-MOD-002
    #[test]
    fn tells_keywords_from_names() {
        use TokenKind::*;
        assert_eq!(
            kinds_and_texts("part partName _part Part part_1 requirement"),
            vec![
                (Keyword, "part".into()),
                (Name, "partName".into()),
                (Name, "_part".into()),
                (Name, "Part".into()),
                (Name, "part_1".into()),
                (Keyword, "requirement".into()),
            ]
        );
        assert!(
            KEYWORDS.windows(2).all(|w| w[0] < w[1]),
            "KEYWORDS must stay sorted"
        );
        let lexed = lex("part");
        assert!(lexed.tokens[0].is("part", "part"));
        assert!(!lexed.tokens[0].is("port", "part"));
    }

    // @verifies CT-USE-007
    #[test]
    fn errors_say_what_where_and_how_to_fix() {
        let src = "a \"open\nb 'bad\\q' c § d\u{a0}e\n/* never closed";
        let lexed = lex(src);
        let found: Vec<(u32, u32, &str)> = lexed
            .errors
            .iter()
            .map(|e| (e.line, e.column, e.message.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                (1, 3, "unterminated string"),
                (2, 7, "invalid escape sequence `\\q`"),
                (2, 13, "unexpected character `§`"),
                (2, 16, "unexpected whitespace character U+00A0"),
                (3, 1, "unterminated comment"),
            ]
        );
        assert!(lexed.errors.iter().all(|e| !e.help.is_empty()));
        assert_eq!(
            lexed.errors[0].to_string(),
            "1:3: error: unterminated string\n  help: close the string with `\"`"
        );
        // Recovery: lexing resumes on the line after an unterminated string.
        let names: Vec<&str> = lexed
            .tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Name)
            .map(|t| t.text(src))
            .collect();
        assert_eq!(names, vec!["a", "b", "c", "d", "e"]);
        let bad = lexed
            .tokens
            .iter()
            .find(|t| t.text(src) == "'bad\\q'")
            .unwrap();
        assert_eq!(bad.kind, TokenKind::Error);
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_an_unterminated_note() {
        let lexed = lex("a //* never closed\nb");
        assert_eq!(
            lexed.errors,
            [LexError {
                line: 1,
                column: 3,
                message: "unterminated note".into(),
                help: "close the note with `*/`",
            }]
        );
        assert_eq!(lexed.tokens.len(), 2, "the note runs to the end of input");
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_a_control_character() {
        let src = "a\u{7}b";
        let lexed = lex(src);
        assert_eq!(
            lexed.errors,
            [LexError {
                line: 1,
                column: 2,
                message: "unexpected control character U+0007".into(),
                help: "delete it; it is invisible in most editors",
            }]
        );
        assert_eq!(lexed.tokens[1].kind, TokenKind::Error);
        assert_eq!(lexed.tokens[2].text(src), "b");
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_an_unterminated_quoted_name() {
        let src = "part 'open\npart b;";
        let lexed = lex(src);
        assert_eq!(lexed.errors.len(), 1);
        let error = &lexed.errors[0];
        assert_eq!((error.line, error.column), (1, 6));
        assert_eq!(error.message, "unterminated quoted name");
        assert_eq!(error.help, "close the name with `'`");
        assert_eq!(lexed.tokens[1].kind, TokenKind::Error);
        assert_eq!(lexed.tokens[1].text(src), "'open");
        // Recovery: lexing resumes on the next line.
        assert_eq!(lexed.tokens[2].text(src), "part");
        assert_eq!(lexed.tokens[2].line, 2);
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_a_non_ascii_name_once() {
        let src = "part Größe;";
        let lexed = lex(src);
        assert_eq!(lexed.errors.len(), 1);
        assert_eq!((lexed.errors[0].line, lexed.errors[0].column), (1, 6));
        assert!(lexed.errors[0].help.contains("single quotes"));
        assert_eq!(lexed.tokens[1].text(src), "Größe");
        assert_eq!(lexed.tokens[1].kind, TokenKind::Error);
        assert!(lex("part 'Größe';").errors.is_empty());
    }

    // @verifies CT-MOD-002
    #[test]
    fn skips_byte_order_mark() {
        let src = "\u{feff}package";
        let lexed = lex(src);
        assert!(lexed.errors.is_empty());
        assert_eq!(lexed.tokens[0].text(src), "package");
        assert_eq!((lexed.tokens[0].line, lexed.tokens[0].column), (1, 1));
    }

    // @verifies CT-MOD-002, CT-MOD-001
    #[test]
    fn lexes_the_project_requirements_without_errors() {
        let src = include_str!("../../../../model/requirements.sysml");
        let lexed = lex(src);
        assert!(lexed.errors.is_empty(), "{:?}", lexed.errors);
        let ids = lexed
            .tokens
            .iter()
            .filter(|t| t.kind == TokenKind::UnrestrictedName && t.value(src).starts_with("CT-"))
            .count();
        assert_eq!(ids, src.matches("<'CT-").count());
        assert!(ids > 50);
    }
}
