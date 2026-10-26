use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::{Document, JsonError, Kind, NONE, Node, NodeId};

/// These bounds keep byte offsets compact and avoid unbounded recursion.
pub const MAX_SOURCE_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_DEPTH: usize = 256;
pub const MAX_NODES: usize = 5_000_000;

pub fn parse(source: String) -> Result<Document, JsonError> {
    parse_with_cancel(source, &AtomicBool::new(false))
}

pub fn parse_with_cancel(source: String, cancelled: &AtomicBool) -> Result<Document, JsonError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(source_size_error());
    }
    parse_arc_with_cancel(Arc::from(source), cancelled)
}

pub fn parse_arc_with_cancel(
    source: Arc<str>,
    cancelled: &AtomicBool,
) -> Result<Document, JsonError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(source_size_error());
    }
    let nodes = {
        let mut parser = Parser {
            source: &source,
            pos: 0,
            nodes: Vec::new(),
            cancelled,
            next_check: 65536,
        };
        parser.whitespace()?;
        parser.value(NONE, None, NONE, 0)?;
        parser.whitespace()?;
        if parser.pos != source.len() {
            return Err(parser.error("根值后还有多余内容"));
        }
        parser.nodes
    };
    Ok(Document { source, nodes })
}

fn source_size_error() -> JsonError {
    JsonError {
        message: "JSON 超过 512 MiB，当前版本不载入此文件".into(),
        offset: 0,
        line: 1,
        column: 1,
    }
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    nodes: Vec<Node>,
    cancelled: &'a AtomicBool,
    next_check: usize,
}

impl Parser<'_> {
    fn error(&self, message: impl Into<String>) -> JsonError {
        let mut end = self.pos.min(self.source.len());
        while !self.source.is_char_boundary(end) {
            end -= 1;
        }
        let prefix = &self.source[..end];
        let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        JsonError {
            message: message.into(),
            offset: self.pos,
            line,
            column,
        }
    }

    fn check_cancel(&self) -> Result<(), JsonError> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(self.error("解析已取消"))
        } else {
            Ok(())
        }
    }

    fn whitespace(&mut self) -> Result<(), JsonError> {
        while let Some(byte) = self.byte() {
            if !matches!(byte, b' ' | b'\n' | b'\r' | b'\t') {
                break;
            }
            self.pos += 1;
            self.periodic_cancel()?;
        }
        Ok(())
    }

    fn byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }

    fn periodic_cancel(&mut self) -> Result<(), JsonError> {
        if self.pos >= self.next_check {
            self.check_cancel()?;
            self.next_check = self.pos.saturating_add(65536);
        }
        Ok(())
    }

    fn value(
        &mut self,
        parent: u32,
        key: Option<(u32, u32)>,
        index: u32,
        depth: usize,
    ) -> Result<NodeId, JsonError> {
        self.check_cancel()?;
        if depth > MAX_DEPTH {
            return Err(self.error("JSON 嵌套超过 256 层"));
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(self.error("JSON 超过 500 万个节点，当前版本停止构建索引"));
        }
        let kind = match self.byte() {
            Some(b'{') => Kind::Object,
            Some(b'[') => Kind::Array,
            Some(b'"') => Kind::String,
            Some(b'-' | b'0'..=b'9') => Kind::Number,
            Some(b't' | b'f') => Kind::Bool,
            Some(b'n') => Kind::Null,
            Some(_) => return Err(self.error("此处需要 JSON 值")),
            None => return Err(self.error("JSON 不完整，此处需要值")),
        };
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            kind,
            start: self.pos as u32,
            end: 0,
            key_start: key.map_or(NONE, |p| p.0),
            key_end: key.map_or(NONE, |p| p.1),
            parent,
            first_child: NONE,
            next_sibling: NONE,
            child_count: 0,
            array_index: index,
        });
        match kind {
            Kind::Object | Kind::Array => self.container(id, depth)?,
            Kind::String => {
                self.string()?;
            }
            Kind::Number => self.number()?,
            Kind::Bool => {
                if self.byte() == Some(b't') {
                    self.literal(b"true")?;
                } else {
                    self.literal(b"false")?;
                }
            }
            Kind::Null => self.literal(b"null")?,
        }
        self.nodes[id as usize].end = self.pos as u32;
        Ok(id)
    }

    fn container(&mut self, id: NodeId, depth: usize) -> Result<(), JsonError> {
        let object = self.nodes[id as usize].kind == Kind::Object;
        let close = if object { b'}' } else { b']' };
        self.pos += 1;
        self.whitespace()?;
        if self.byte() == Some(close) {
            self.pos += 1;
            return Ok(());
        }
        let mut previous = NONE;
        let mut count = 0;
        loop {
            let key = if object {
                if self.byte() != Some(b'"') {
                    return Err(self.error("对象字段名需要双引号"));
                }
                let start = self.pos as u32;
                self.string()?;
                let end = self.pos as u32;
                self.whitespace()?;
                if self.byte() != Some(b':') {
                    return Err(self.error("字段名后缺少冒号"));
                }
                self.pos += 1;
                self.whitespace()?;
                Some((start, end))
            } else {
                None
            };
            let child = self.value(id, key, if object { NONE } else { count }, depth + 1)?;
            if previous == NONE {
                self.nodes[id as usize].first_child = child;
            } else {
                self.nodes[previous as usize].next_sibling = child;
            }
            previous = child;
            count += 1;
            self.whitespace()?;
            match self.byte() {
                Some(byte) if byte == close => {
                    self.pos += 1;
                    break;
                }
                Some(b',') => {
                    self.pos += 1;
                    self.whitespace()?;
                }
                _ => {
                    return Err(self.error(if object {
                        "字段后需要逗号或右花括号"
                    } else {
                        "数组项后需要逗号或右方括号"
                    }));
                }
            }
        }
        self.nodes[id as usize].child_count = count;
        Ok(())
    }

    fn string(&mut self) -> Result<(), JsonError> {
        self.pos += 1;
        while let Some(byte) = self.byte() {
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(());
                }
                b'\\' => {
                    self.pos += 1;
                    match self.byte() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.pos += 1
                        }
                        Some(b'u') => {
                            self.pos += 1;
                            let code = self.hex4()?;
                            if (0xd800..=0xdbff).contains(&code) {
                                if self.source.as_bytes().get(self.pos..self.pos + 2)
                                    != Some(b"\\u")
                                {
                                    return Err(self.error("Unicode 高代理项后缺少低代理项"));
                                }
                                self.pos += 2;
                                if !(0xdc00..=0xdfff).contains(&self.hex4()?) {
                                    return Err(self.error("Unicode 低代理项无效"));
                                }
                            } else if (0xdc00..=0xdfff).contains(&code) {
                                return Err(self.error("Unicode 低代理项缺少高代理项"));
                            }
                        }
                        _ => return Err(self.error("字符串转义无效")),
                    }
                }
                0..=31 => return Err(self.error("字符串内含未转义的控制字符")),
                _ => self.pos += 1,
            }
            self.periodic_cancel()?;
        }
        Err(self.error("字符串缺少结束双引号"))
    }

    fn hex4(&mut self) -> Result<u16, JsonError> {
        let mut code = 0_u16;
        for _ in 0..4 {
            let digit = match self.byte() {
                Some(b'0'..=b'9') => self.byte().unwrap() - b'0',
                Some(b'a'..=b'f') => self.byte().unwrap() - b'a' + 10,
                Some(b'A'..=b'F') => self.byte().unwrap() - b'A' + 10,
                _ => return Err(self.error("Unicode 转义需要四位十六进制数字")),
            };
            code = code * 16 + u16::from(digit);
            self.pos += 1;
        }
        Ok(code)
    }

    fn literal(&mut self, expected: &[u8]) -> Result<(), JsonError> {
        if self
            .source
            .as_bytes()
            .get(self.pos..self.pos + expected.len())
            != Some(expected)
        {
            return Err(self.error("JSON 字面量无效"));
        }
        self.pos += expected.len();
        Ok(())
    }

    fn number(&mut self) -> Result<(), JsonError> {
        if self.byte() == Some(b'-') {
            self.pos += 1;
        }
        match self.byte() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.pos += 1;
                self.digits()?;
            }
            _ => return Err(self.error("数字缺少整数部分")),
        }
        if self.byte() == Some(b'.') {
            self.pos += 1;
            let start = self.pos;
            self.digits()?;
            if self.pos == start {
                return Err(self.error("小数点后缺少数字"));
            }
        }
        if matches!(self.byte(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.byte(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let start = self.pos;
            self.digits()?;
            if self.pos == start {
                return Err(self.error("指数部分缺少数字"));
            }
        }
        Ok(())
    }

    fn digits(&mut self) -> Result<(), JsonError> {
        while matches!(self.byte(), Some(b'0'..=b'9')) {
            self.pos += 1;
            self.periodic_cancel()?;
        }
        Ok(())
    }
}
