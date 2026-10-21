//! Minimal JSON reader and string writer for the files `clew notices` reads (`cargo metadata`
//! output and `package-lock.json`) and the SBOM it writes. Standard library only.

use std::fmt;

/// A parsed JSON value. Numbers keep their source text, since no caller does arithmetic.
// @implements CT-DEV-014
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

// @implements CT-DEV-014, CT-USE-007
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub offset: usize,
    pub message: String,
}

// @implements CT-DEV-014, CT-USE-007
impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid JSON at byte {}: {}", self.offset, self.message)
    }
}

// @implements CT-DEV-014
impl Json {
    /// The value of `key` when this is an object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The items of an array, or an empty slice for anything else.
    pub fn items(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            _ => &[],
        }
    }

    /// The fields of an object, or an empty slice for anything else.
    pub fn fields(&self) -> &[(String, Json)] {
        match self {
            Json::Obj(fields) => fields,
            _ => &[],
        }
    }
}

/// Parses one JSON document.
// @implements CT-DEV-014
pub fn parse(text: &str) -> Result<Json, JsonError> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    let value = parser.value(0)?;
    parser.skip_space();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("unexpected text after the value"));
    }
    Ok(value)
}

/// `text` as a quoted JSON string.
// @implements CT-DEV-014
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Nesting deeper than this is rejected, so a hostile file cannot overflow the stack.
// @implements CT-DEV-014
const MAX_DEPTH: usize = 128;

// @implements CT-DEV-014
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

// @implements CT-DEV-014
impl Parser<'_> {
    fn error(&self, message: &str) -> JsonError {
        JsonError {
            offset: self.pos,
            message: message.to_string(),
        }
    }

    fn skip_space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, literal: &str) -> bool {
        if self.bytes[self.pos..].starts_with(literal.as_bytes()) {
            self.pos += literal.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        self.skip_space();
        match self.bytes.get(self.pos) {
            None => Err(self.error("unexpected end of input")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'-' | b'0'..=b'9') => Ok(self.number()),
            Some(_) if self.eat("null") => Ok(Json::Null),
            Some(_) if self.eat("true") => Ok(Json::Bool(true)),
            Some(_) if self.eat("false") => Ok(Json::Bool(false)),
            Some(_) => Err(self.error("expected a value")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.pos += 1;
        let mut fields = Vec::new();
        self.skip_space();
        if self.eat("}") {
            return Ok(Json::Obj(fields));
        }
        loop {
            self.skip_space();
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err(self.error("expected a quoted key"));
            }
            let key = self.string()?;
            self.skip_space();
            if !self.eat(":") {
                return Err(self.error("expected `:`"));
            }
            fields.push((key, self.value(depth + 1)?));
            self.skip_space();
            if self.eat("}") {
                return Ok(Json::Obj(fields));
            }
            if !self.eat(",") {
                return Err(self.error("expected `,` or `}`"));
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.eat("]") {
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_space();
            if self.eat("]") {
                return Ok(Json::Arr(items));
            }
            if !self.eat(",") {
                return Err(self.error("expected `,` or `]`"));
            }
        }
    }

    fn number(&mut self) -> Json {
        let start = self.pos;
        while let Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
        Json::Num(String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned())
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .ok_or_else(|| self.error("bad \\u escape"))?;
        self.pos += 4;
        Ok(digits)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let Some(&b) = self.bytes.get(self.pos) else {
                return Err(self.error("unterminated string"));
            };
            self.pos += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.bytes.get(self.pos) else {
                        return Err(self.error("unterminated string"));
                    };
                    self.pos += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code) && self.eat("\\u") {
                                let low = self.hex4()?;
                                code =
                                    0x10000 + ((code - 0xD800) << 10) + (low.wrapping_sub(0xDC00));
                            }
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => return Err(self.error("bad escape")),
                    };
                    let mut buf = [0; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                _ => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| self.error("string is not UTF-8"))
    }
}

// @verifies CT-DEV-014
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-DEV-014
    #[test]
    fn parses_nested_values() {
        let doc = parse(r#" {"a": [1, -2.5e3, true, false, null], "b": {"c": "x\"é😀\n"}} "#)
            .expect("valid JSON parses");
        let a = doc.get("a").expect("a").items();
        assert_eq!(a.len(), 5);
        assert_eq!(a[1], Json::Num("-2.5e3".to_string()));
        assert_eq!(a[2].as_bool(), Some(true));
        assert_eq!(a[4], Json::Null);
        let c = doc.get("b").and_then(|b| b.get("c")).and_then(Json::as_str);
        assert_eq!(c, Some("x\"é😀\n"));
        assert_eq!(parse("[]"), Ok(Json::Arr(vec![])));
    }

    // @verifies CT-DEV-014, CT-USE-007
    #[test]
    fn rejects_bad_input_with_an_offset() {
        for bad in ["", "{", "[1,]", "{\"a\" 1}", "\"open", "[1] 2", "nul"] {
            assert!(parse(bad).is_err(), "`{bad}` parsed");
        }
        let deep = "[".repeat(MAX_DEPTH + 2);
        assert!(parse(&deep).is_err());
        let e = parse("[1, x]").unwrap_err();
        assert_eq!(e.offset, 4);
        assert!(e.to_string().contains("byte 4"));
    }

    // @verifies CT-DEV-014
    #[test]
    fn quoted_strings_round_trip() {
        let text = "a \"b\" \\ c\n\t\u{1}é";
        let quoted = quote(text);
        assert_eq!(quoted, "\"a \\\"b\\\" \\\\ c\\n\\t\\u0001é\"");
        assert_eq!(parse(&quoted), Ok(Json::Str(text.to_string())));
    }
}
