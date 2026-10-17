//! ISO 10303-21 (STEP physical file) reading and writing for IFC.
//! The reader is bounded: file size, entity count, nesting depth and string
//! length limits turn hostile input into errors instead of panics or exhaustion.
use std::collections::BTreeMap;

pub const MAX_FILE_BYTES: usize = 512 << 20;
pub const MAX_ENTITIES: usize = 5_000_000;
pub const MAX_DEPTH: usize = 32;
pub const MAX_STRING: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Derived,
    Integer(i64),
    Real(f64),
    String(String),
    Enum(String),
    Ref(u64),
    List(Vec<Value>),
    /// Typed value such as IFCLABEL('x').
    Typed(String, Box<Value>),
}

impl Value {
    pub fn as_ref(&self) -> Option<u64> {
        match self {
            Self::Ref(r) => Some(*r),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(v) => Some(*v),
            Self::Integer(v) => Some(*v as f64),
            Self::Typed(_, v) => v.as_f64(),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            Self::Typed(_, v) => v.as_str(),
            _ => None,
        }
    }
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Self::Enum(e) => Some(e),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub kind: String,
    pub args: Vec<Value>,
}

#[derive(Clone, Debug, Default)]
pub struct File {
    pub schema: Vec<String>,
    pub entities: BTreeMap<u64, Entity>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

type Result<T> = std::result::Result<T, String>;

impl Reader<'_> {
    fn error<T>(&self, message: &str) -> Result<T> {
        Err(format!("STEP: {message} (байт {})", self.at))
    }
    fn skip(&mut self) -> Result<()> {
        loop {
            while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
                self.at += 1;
            }
            if self.bytes[self.at..].starts_with(b"/*") {
                match self.bytes[self.at + 2..]
                    .windows(2)
                    .position(|w| w == b"*/")
                {
                    Some(end) => self.at += end + 4,
                    None => return self.error("незакрытый комментарий"),
                }
            } else {
                return Ok(());
            }
        }
    }
    fn peek(&mut self) -> Result<u8> {
        self.skip()?;
        self.bytes
            .get(self.at)
            .copied()
            .ok_or_else(|| "STEP: неожиданный конец файла".into())
    }
    fn expect(&mut self, c: u8) -> Result<()> {
        if self.peek()? != c {
            return self.error(&format!("ожидался символ «{}»", c as char));
        }
        self.at += 1;
        Ok(())
    }
    fn keyword(&mut self) -> Result<String> {
        self.skip()?;
        let start = self.at;
        while self.at < self.bytes.len()
            && (self.bytes[self.at].is_ascii_alphanumeric()
                || self.bytes[self.at] == b'_'
                || self.bytes[self.at] == b'-')
        {
            self.at += 1;
        }
        if start == self.at {
            return self.error("ожидалось ключевое слово");
        }
        Ok(String::from_utf8_lossy(&self.bytes[start..self.at]).to_ascii_uppercase())
    }
    fn string(&mut self) -> Result<String> {
        self.expect(b'\'')?;
        let mut raw = Vec::new();
        loop {
            let Some(&c) = self.bytes.get(self.at) else {
                return self.error("незакрытая строка");
            };
            self.at += 1;
            if c == b'\'' {
                if self.bytes.get(self.at) == Some(&b'\'') {
                    raw.push(b'\'');
                    self.at += 1;
                    continue;
                }
                break;
            }
            raw.push(c);
            if raw.len() > MAX_STRING {
                return self.error("строка слишком длинная");
            }
        }
        decode(&raw)
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            return self.error("слишком глубокая вложенность");
        }
        match self.peek()? {
            b'$' => {
                self.at += 1;
                Ok(Value::Null)
            }
            b'*' => {
                self.at += 1;
                Ok(Value::Derived)
            }
            b'\'' => Ok(Value::String(self.string()?)),
            b'.' => {
                self.at += 1;
                let name = self.keyword()?;
                self.expect(b'.')?;
                Ok(Value::Enum(name))
            }
            b'#' => {
                self.at += 1;
                let start = self.at;
                while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_digit() {
                    self.at += 1;
                }
                std::str::from_utf8(&self.bytes[start..self.at])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .map(Value::Ref)
                    .ok_or_else(|| "STEP: неверная ссылка".into())
            }
            b'(' => {
                self.at += 1;
                let mut items = Vec::new();
                if self.peek()? == b')' {
                    self.at += 1;
                    return Ok(Value::List(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    if items.len() > MAX_ENTITIES {
                        return self.error("слишком длинный список");
                    }
                    match self.peek()? {
                        b',' => self.at += 1,
                        b')' => {
                            self.at += 1;
                            return Ok(Value::List(items));
                        }
                        _ => return self.error("ожидалась «,» или «)»"),
                    }
                }
            }
            b'"' => {
                // Binary literal: kept as a string of hex digits.
                self.at += 1;
                let start = self.at;
                while self.at < self.bytes.len() && self.bytes[self.at] != b'"' {
                    self.at += 1;
                }
                let text = String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned();
                self.expect(b'"')?;
                Ok(Value::String(text))
            }
            c if c == b'-' || c == b'+' || c.is_ascii_digit() => {
                let start = self.at;
                self.at += 1;
                while self.at < self.bytes.len()
                    && (self.bytes[self.at].is_ascii_digit()
                        || b".eE+-".contains(&self.bytes[self.at]))
                {
                    self.at += 1;
                }
                let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or("");
                if text.contains(['.', 'e', 'E']) {
                    text.parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .map(Value::Real)
                        .ok_or_else(|| format!("STEP: неверное число «{text}»"))
                } else {
                    text.parse::<i64>()
                        .map(Value::Integer)
                        .map_err(|_| format!("STEP: неверное целое «{text}»"))
                }
            }
            c if c.is_ascii_alphabetic() => {
                let name = self.keyword()?;
                self.expect(b'(')?;
                self.at -= 1;
                let inner = self.value(depth + 1)?;
                let mut items = match inner {
                    Value::List(items) => items,
                    other => vec![other],
                };
                let value = if items.len() == 1 {
                    items.remove(0)
                } else {
                    Value::List(items)
                };
                Ok(Value::Typed(name, Box::new(value)))
            }
            _ => self.error("неожиданный символ"),
        }
    }
}

fn hex(bytes: &[u8]) -> Option<u32> {
    u32::from_str_radix(std::str::from_utf8(bytes).ok()?, 16).ok()
}

/// Decode STEP string escapes: \\, \X\HH, \X2\HHHH..\X0\, \X4\HHHHHHHH..\X0\.
pub fn decode(raw: &[u8]) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    let bad = || "STEP: неверная escape-последовательность".to_string();
    while i < raw.len() {
        if raw[i] != b'\\' {
            let start = i;
            while i < raw.len() && raw[i] != b'\\' {
                i += 1;
            }
            out.push_str(&String::from_utf8_lossy(&raw[start..i]));
            continue;
        }
        let rest = &raw[i..];
        if rest.starts_with(b"\\\\") {
            out.push('\\');
            i += 2;
        } else if rest.starts_with(b"\\X2\\") || rest.starts_with(b"\\X4\\") {
            let width = if rest[2] == b'2' { 4 } else { 8 };
            i += 4;
            let end = raw[i..]
                .windows(4)
                .position(|w| w == b"\\X0\\")
                .ok_or_else(bad)?;
            let body = &raw[i..i + end];
            if !body.len().is_multiple_of(width) {
                return Err(bad());
            }
            let units: Vec<u32> = body
                .chunks(width)
                .map(hex)
                .collect::<Option<_>>()
                .ok_or_else(bad)?;
            if width == 4 {
                let units: Vec<u16> = units.iter().map(|u| *u as u16).collect();
                out.push_str(&String::from_utf16(&units).map_err(|_| bad())?);
            } else {
                for u in units {
                    out.push(char::from_u32(u).ok_or_else(bad)?);
                }
            }
            i += end + 4;
        } else if rest.starts_with(b"\\X\\") && rest.len() >= 5 {
            out.push(char::from_u32(hex(&rest[3..5]).ok_or_else(bad)?).ok_or_else(bad)?);
            i += 5;
        } else if rest.starts_with(b"\\S\\") && rest.len() >= 4 {
            out.push(char::from_u32(rest[3] as u32 + 128).ok_or_else(bad)?);
            i += 4;
        } else if rest.starts_with(b"\\P") && rest.len() >= 4 && rest[3] == b'\\' {
            i += 4;
        } else {
            out.push('\\');
            i += 1;
        }
    }
    Ok(out)
}

/// Encode a string literal with apostrophes doubled and non-ASCII as \X2\.
pub fn encode(text: &str) -> String {
    let mut out = String::from("'");
    let mut wide: Vec<u16> = Vec::new();
    let flush = |out: &mut String, wide: &mut Vec<u16>| {
        if !wide.is_empty() {
            out.push_str("\\X2\\");
            for u in wide.drain(..) {
                out.push_str(&format!("{u:04X}"));
            }
            out.push_str("\\X0\\");
        }
    };
    for c in text.chars() {
        if c.is_ascii() && !c.is_ascii_control() {
            flush(&mut out, &mut wide);
            match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        } else {
            let mut buffer = [0u16; 2];
            wide.extend_from_slice(c.encode_utf16(&mut buffer));
        }
    }
    flush(&mut out, &mut wide);
    out.push('\'');
    out
}

/// STEP real: always with a decimal point.
pub fn real(value: f64) -> String {
    if !value.is_finite() {
        return "0.".into();
    }
    let v = if value.abs() < 1e-12 { 0. } else { value };
    let s = format!("{v:.9}");
    let s = s.trim_end_matches('0');
    s.to_string()
}

pub fn parse(text: &str) -> Result<File> {
    if text.len() > MAX_FILE_BYTES {
        return Err("IFC: файл больше 512 МиБ".into());
    }
    let mut r = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    if r.keyword()? != "ISO-10303-21" {
        return r.error("нет заголовка ISO-10303-21");
    }
    r.expect(b';')?;
    let mut file = File::default();
    let mut in_data = false;
    loop {
        if r.peek()? == b'#' {
            if !in_data {
                return r.error("экземпляр вне секции DATA");
            }
            r.at += 1;
            let start = r.at;
            while r.at < r.bytes.len() && r.bytes[r.at].is_ascii_digit() {
                r.at += 1;
            }
            let id: u64 = std::str::from_utf8(&r.bytes[start..r.at])
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or("STEP: неверный номер экземпляра")?;
            r.expect(b'=')?;
            let kind = r.keyword()?;
            let args = match r.value(0)? {
                Value::List(items) => items,
                _ => return r.error("ожидались аргументы"),
            };
            r.expect(b';')?;
            if file.entities.insert(id, Entity { kind, args }).is_some() {
                return Err(format!("STEP: повторный экземпляр #{id}"));
            }
            if file.entities.len() > MAX_ENTITIES {
                return Err("IFC: слишком много экземпляров".into());
            }
            continue;
        }
        let word = r.keyword()?;
        match word.as_str() {
            "HEADER" | "DATA" => {
                r.expect(b';')?;
                in_data = word == "DATA";
            }
            "ENDSEC" => {
                r.expect(b';')?;
                in_data = false;
            }
            "END-ISO-10303-21" => {
                r.expect(b';')?;
                break;
            }
            _ if !in_data => {
                let args = r.value(0)?;
                r.expect(b';')?;
                if word == "FILE_SCHEMA" {
                    file.schema = args
                        .as_list()
                        .and_then(|a| a.first())
                        .and_then(Value::as_list)
                        .map(|l| {
                            l.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                }
            }
            _ => return r.error("ожидался экземпляр #N"),
        }
    }
    Ok(file)
}

const GUID_CHARS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// IFC compressed GUID (22 characters) of 128 bits.
pub fn guid(bits: u128) -> String {
    let mut out = String::with_capacity(22);
    out.push(GUID_CHARS[(bits >> 126) as usize & 3] as char);
    for i in (0..21).rev() {
        out.push(GUID_CHARS[((bits >> (i * 6)) & 63) as usize] as char);
    }
    out
}

pub fn valid_guid(text: &str) -> bool {
    text.len() == 22
        && text.bytes().all(|b| GUID_CHARS.contains(&b))
        && GUID_CHARS
            .iter()
            .position(|c| *c == text.as_bytes()[0])
            .is_some_and(|i| i < 4)
}
