//! 极简 JSON 解析：递归下降、零依赖，覆盖完整 JSON 语法
//! （对象/数组/字符串/数字/true/false/null）。
//! Minimal JSON parsing: recursive descent, zero dependencies, covering the
//! full grammar (objects/arrays/strings/numbers/true/false/null).
//! 用于 GitHub/GitLab CI API 的响应——响应体通常不大，性能不是目标。
//! Used for the GitHub/GitLab CI API responses — bodies are small, speed is
//! not the goal.

/// 解析值 / Parsed value
#[derive(Debug, Clone)]
pub enum V {
    /// null
    Null,
    /// true / false
    Bool(bool),
    /// 数字（一律 f64——API 里只要够用）/ numbers, always f64
    Num(f64),
    /// "..."
    Str(String),
    /// [...]
    Arr(Vec<V>),
    /// {...}（键序保留）/ object (key order preserved)
    Obj(Vec<(String, V)>),
}

impl V {
    /// obj["key"]：不是对象或键不存在 → Null / index into an object
    pub fn get(&self, key: &str) -> &V {
        match self {
            V::Obj(kvs) => kvs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or(&V::Null),
            _ => &V::Null,
        }
    }
    /// 数组视图 / array view
    pub fn arr(&self) -> &[V] {
        match self {
            V::Arr(v) => v,
            _ => &[],
        }
    }
    /// 字符串视图（非字符串 → ""）/ string view
    pub fn s(&self) -> &str {
        match self {
            V::Str(s) => s,
            _ => "",
        }
    }
    /// 布尔视图（非布尔 → false）/ bool view
    pub fn b(&self) -> bool {
        matches!(self, V::Bool(true))
    }
    /// 是否 null / is null
    pub fn is_null(&self) -> bool {
        matches!(self, V::Null)
    }
}

/// 解析一个 JSON 文档 / Parse one JSON document
pub fn parse(src: &str) -> Result<V, String> {
    let b = src.as_bytes();
    let mut p = P { b, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i < b.len() {
        return Err(format!("json: trailing data at byte {}", p.i));
    }
    Ok(v)
}

/// 解析游标 / Parse cursor
struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl P<'_> {
    /// 当前字节（越界返回 0）/ current byte (0 past the end)
    fn peek(&self) -> u8 {
        *self.b.get(self.i).unwrap_or(&0)
    }

    /// 跳过空白 / skip whitespace
    fn ws(&mut self) {
        while self.peek().is_ascii_whitespace() {
            self.i += 1;
        }
    }

    /// 消费一个字面字节 / consume one literal byte
    fn eat(&mut self, c: u8) -> Result<(), String> {
        if self.peek() != c {
            return Err(format!("json: expected '{}' at byte {}", c as char, self.i));
        }
        self.i += 1;
        Ok(())
    }

    /// 字面量（true/false/null）/ literal
    fn lit(&mut self, word: &str, v: V) -> Result<V, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("json: expected '{word}' at byte {}", self.i))
        }
    }

    /// 任意值 / any value
    fn value(&mut self) -> Result<V, String> {
        self.ws();
        match self.peek() {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Ok(V::Str(self.string()?)),
            b't' => self.lit("true", V::Bool(true)),
            b'f' => self.lit("false", V::Bool(false)),
            b'n' => self.lit("null", V::Null),
            b'-' | b'0'..=b'9' => self.number(),
            c => Err(format!(
                "json: unexpected '{}' at byte {}",
                c as char, self.i
            )),
        }
    }

    /// 数字 / number
    fn number(&mut self) -> Result<V, String> {
        let start = self.i;
        if self.peek() == b'-' {
            self.i += 1;
        }
        while matches!(self.peek(), b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| "bad utf8")?;
        s.parse::<f64>()
            .map(V::Num)
            .map_err(|_| format!("json: bad number '{s}'"))
    }

    /// 解析字符串字面量（支持 \" \\ \/ \b \f \n \r \t \uXXXX + 代理对）
    /// Parse a string literal (" \\ \/ \b \f \n \r \t \uXXXX + surrogate pairs)
    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut s = String::new();
        loop {
            let c = self.peek();
            match c {
                0 => return Err("json: unterminated string".into()),
                b'"' => {
                    self.i += 1;
                    return Ok(s);
                }
                b'\\' => {
                    self.i += 1;
                    match self.peek() {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{8}'),
                        b'f' => s.push('\u{c}'),
                        b'n' => s.push('\n'),
                        b't' => s.push('\t'),
                        b'r' => s.push('\r'),
                        b'u' => {
                            if self.i + 5 > self.b.len() {
                                return Err("json: bad \\u escape".into());
                            }
                            let h = std::str::from_utf8(&self.b[self.i + 1..self.i + 5])
                                .map_err(|_| "json: bad \\u escape".to_string())?;
                            let mut cp = u32::from_str_radix(h, 16)
                                .map_err(|_| "json: bad \\u escape".to_string())?;
                            self.i += 5;
                            // UTF-16 代理对 / UTF-16 surrogate pair
                            if (0xd800..0xdc00).contains(&cp)
                                && self.b[self.i..].starts_with(b"\\u")
                                && self.i + 6 <= self.b.len()
                            {
                                let h2 = std::str::from_utf8(&self.b[self.i + 2..self.i + 6])
                                    .map_err(|_| "json: bad \\u escape".to_string())?;
                                let lo = u32::from_str_radix(h2, 16)
                                    .map_err(|_| "json: bad \\u escape".to_string())?;
                                if (0xdc00..0xe000).contains(&lo) {
                                    cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                                    self.i += 6;
                                }
                            }
                            s.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                            self.i -= 1; // 下方统一 +1 / unified +1 below
                        }
                        _ => return Err(format!("json: bad escape at byte {}", self.i)),
                    }
                    self.i += 1;
                }
                _ => {
                    // UTF-8 字节原样拷贝 / copy UTF-8 bytes verbatim
                    let start = self.i;
                    self.i += 1;
                    while self.i < self.b.len() && self.b[self.i] != b'"' && self.b[self.i] != b'\\'
                    {
                        self.i += 1;
                    }
                    s.push_str(
                        std::str::from_utf8(&self.b[start..self.i])
                            .map_err(|_| "json: invalid utf-8".to_string())?,
                    );
                }
            }
        }
    }

    /// 解析对象 / Parse an object
    fn object(&mut self) -> Result<V, String> {
        self.eat(b'{')?;
        let mut out = Vec::new();
        loop {
            self.ws();
            if self.peek() == b'}' {
                self.i += 1;
                return Ok(V::Obj(out));
            }
            let k = self.string()?;
            self.ws();
            self.eat(b':')?;
            let v = self.value()?;
            out.push((k, v));
            self.ws();
            if self.peek() == b',' {
                self.i += 1;
            } else {
                self.eat(b'}')?;
                return Ok(V::Obj(out));
            }
        }
    }

    /// 解析数组 / Parse an array
    fn array(&mut self) -> Result<V, String> {
        self.eat(b'[')?;
        let mut out = Vec::new();
        loop {
            self.ws();
            if self.peek() == b']' {
                self.i += 1;
                return Ok(V::Arr(out));
            }
            out.push(self.value()?);
            self.ws();
            if self.peek() == b',' {
                self.i += 1;
            } else {
                self.eat(b']')?;
                return Ok(V::Arr(out));
            }
        }
    }
}
