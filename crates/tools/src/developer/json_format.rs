//! JSON Formatter. Uses its own small parser instead of `serde_json::Value`
//! so that a formatter never changes data: numbers and strings are written
//! back exactly as they were (no float rounding, no re-escaping), and key
//! order and duplicate keys are kept.

use saqviro_jobs::{ErrorKind, JobContext, JobError, JobInput, JobOutput, Params};

/// Nesting deeper than this is rejected instead of risking a stack overflow.
const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    Spaces(usize),
    Tab,
    Minify,
}

pub fn run(_ctx: &JobContext, input: JobInput, params: &Params) -> Result<JobOutput, JobError> {
    let JobInput::Text(src) = input else {
        return Err(JobError::new(
            ErrorKind::InvalidInput,
            "JSON Formatter takes text, not files",
        ));
    };
    let indent = match params.get("indent").and_then(|v| v.as_str()).unwrap_or("2") {
        "4" => Indent::Spaces(4),
        "tab" => Indent::Tab,
        "minify" => Indent::Minify,
        _ => Indent::Spaces(2),
    };
    let sort_keys = params.get("sort_keys").and_then(|v| v.as_bool()).unwrap_or(false);
    format(&src, indent, sort_keys)
        .map(JobOutput::Text)
        .map_err(|e| JobError::new(ErrorKind::InvalidInput, e))
}

/// Formats JSON text. Errors read like `line 3, column 8: expected a value`.
pub fn format(src: &str, indent: Indent, sort_keys: bool) -> Result<String, String> {
    let mut parser = Parser { src, pos: 0 };
    parser.skip_ws();
    if parser.pos == src.len() {
        return Err("input is empty".into());
    }
    let mut node = parser.value(0).map_err(|e| parser.located(e))?;
    parser.skip_ws();
    if parser.pos != src.len() {
        return Err(parser.located("unexpected text after the JSON value"));
    }
    if sort_keys {
        node.sort_keys();
    }
    let mut out = String::with_capacity(src.len() + src.len() / 4);
    node.write(indent, 0, &mut out);
    if indent != Indent::Minify {
        out.push('\n');
    }
    Ok(out)
}

enum Node<'a> {
    /// A number, string, `true`, `false` or `null`, exactly as written.
    Raw(&'a str),
    Array(Vec<Node<'a>>),
    Object(Vec<(&'a str, Node<'a>)>),
}

impl Node<'_> {
    fn sort_keys(&mut self) {
        match self {
            Node::Raw(_) => {}
            Node::Array(items) => items.iter_mut().for_each(Node::sort_keys),
            Node::Object(members) => {
                members.sort_by(|a, b| a.0.cmp(b.0));
                members.iter_mut().for_each(|(_, v)| v.sort_keys());
            }
        }
    }

    fn write(&self, indent: Indent, level: usize, out: &mut String) {
        let newline = |out: &mut String, level: usize| {
            if indent == Indent::Minify {
                return;
            }
            out.push('\n');
            for _ in 0..level {
                match indent {
                    Indent::Spaces(n) => out.extend(std::iter::repeat_n(' ', n)),
                    Indent::Tab => out.push('\t'),
                    Indent::Minify => {}
                }
            }
        };
        match self {
            Node::Raw(s) => out.push_str(s),
            Node::Array(items) if items.is_empty() => out.push_str("[]"),
            Node::Object(members) if members.is_empty() => out.push_str("{}"),
            Node::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, level + 1);
                    item.write(indent, level + 1, out);
                }
                newline(out, level);
                out.push(']');
            }
            Node::Object(members) => {
                out.push('{');
                for (i, (key, value)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, level + 1);
                    out.push_str(key);
                    out.push_str(if indent == Indent::Minify { ":" } else { ": " });
                    value.write(indent, level + 1, out);
                }
                newline(out, level);
                out.push('}');
            }
        }
    }
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8, what: &'static str) -> Result<(), &'static str> {
        self.skip_ws();
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(what)
        }
    }

    fn located(&self, message: &str) -> String {
        let before = &self.src[..self.pos];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        format!("line {line}, column {column}: {message}")
    }

    fn value(&mut self, depth: usize) -> Result<Node<'a>, &'static str> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => self.string().map(Node::Raw),
            Some(b'-' | b'0'..=b'9') => self.number().map(Node::Raw),
            Some(b't') => self.literal("true"),
            Some(b'f') => self.literal("false"),
            Some(b'n') => self.literal("null"),
            Some(_) => Err("expected a value"),
            None => Err("unexpected end of input"),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Node<'a>, &'static str> {
        if depth > MAX_DEPTH {
            return Err("nesting is too deep");
        }
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Node::Array(items));
        }
        loop {
            items.push(self.value(depth)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Node::Array(items));
                }
                _ => return Err("expected `,` or `]`"),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Node<'a>, &'static str> {
        if depth > MAX_DEPTH {
            return Err("nesting is too deep");
        }
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Node::Object(members));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err("expected a key in double quotes");
            }
            let key = self.string()?;
            self.expect(b':', "expected `:` after the key")?;
            members.push((key, self.value(depth)?));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Node::Object(members));
                }
                _ => return Err("expected `,` or `}`"),
            }
        }
    }

    fn string(&mut self) -> Result<&'a str, &'static str> {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek() {
                None => return Err("unterminated string"),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(&self.src[start..self.pos]);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => self.pos += 1,
                        Some(b'u') => {
                            let hex = self.src.as_bytes().get(self.pos + 1..self.pos + 5);
                            if !hex.is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) {
                                return Err("invalid \\u escape");
                            }
                            self.pos += 5;
                        }
                        _ => return Err("invalid escape in string"),
                    }
                }
                Some(0..0x20) => return Err("control character in string; use an escape like \\n"),
                Some(_) => self.pos += 1,
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }

    fn number(&mut self) -> Result<&'a str, &'static str> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err("invalid number"),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if self.digits() == 0 {
                return Err("expected digits after the decimal point");
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.digits() == 0 {
                return Err("expected digits in the exponent");
            }
        }
        Ok(&self.src[start..self.pos])
    }

    fn literal(&mut self, word: &'static str) -> Result<Node<'a>, &'static str> {
        if self.src[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(Node::Raw(word))
        } else {
            Err("expected a value")
        }
    }
}
