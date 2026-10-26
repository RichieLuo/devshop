use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};

use super::{Document, MAX_SOURCE_BYTES, NodeId};

impl Document {
    pub fn pretty(&self) -> String {
        self.format_node(self.root(), true)
    }

    pub fn minify(&self) -> String {
        self.format_node(self.root(), false)
    }

    pub fn format_node(&self, node: NodeId, pretty: bool) -> String {
        let mut output = Vec::with_capacity(self.raw(node).len());
        self.write_node(node, pretty, &mut output)
            .expect("writing to a Vec cannot fail");
        String::from_utf8(output).expect("formatting retains the original UTF-8")
    }

    /// Bounded, cancellable formatting for interactive use. Streaming saves
    /// can call `write_node` directly instead of retaining the output in RAM.
    pub fn try_format_node(&self, node: NodeId, pretty: bool) -> io::Result<String> {
        self.try_format_node_with_cancel(node, pretty, &AtomicBool::new(false))
    }

    pub fn try_format_node_with_cancel(
        &self,
        node: NodeId,
        pretty: bool,
        cancelled: &AtomicBool,
    ) -> io::Result<String> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::other("格式化已取消"));
        }
        struct Bounded<'a> {
            buffer: Vec<u8>,
            cancelled: &'a AtomicBool,
        }
        impl Write for Bounded<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.cancelled.load(Ordering::Relaxed) {
                    return Err(io::Error::other("格式化已取消"));
                }
                if self.buffer.len().saturating_add(bytes.len()) > MAX_SOURCE_BYTES {
                    return Err(io::Error::other("格式化结果超过 512 MiB，请缩小数据范围"));
                }
                self.buffer.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut output = Bounded {
            buffer: Vec::with_capacity(self.raw(node).len()),
            cancelled,
        };
        self.write_node(node, pretty, &mut output)?;
        Ok(String::from_utf8(output.buffer).expect("formatting retains the original UTF-8"))
    }

    /// Stream large formatted documents into a file without building a second
    /// complete String. Output preserves member order, duplicate members,
    /// number spelling, and string escapes exactly.
    pub fn write_node(
        &self,
        node: NodeId,
        pretty: bool,
        writer: &mut impl Write,
    ) -> io::Result<()> {
        let bytes = self.raw(node).as_bytes();
        let mut output = Output {
            writer,
            buffer: Vec::with_capacity(65536),
        };
        let mut position = 0;
        let mut depth: usize = 0;
        let mut previous = 0;
        while position < bytes.len() {
            let byte = bytes[position];
            if byte.is_ascii_whitespace() {
                position += 1;
                continue;
            }
            if byte == b'"' {
                let start = position;
                position += 1;
                while position < bytes.len() {
                    match bytes[position] {
                        b'\\' => position += 2,
                        b'"' => {
                            position += 1;
                            break;
                        }
                        _ => position += 1,
                    }
                }
                output.extend(&bytes[start..position])?;
                previous = b'"';
                continue;
            }
            match byte {
                b'{' | b'[' => {
                    output.byte(byte)?;
                    depth += 1;
                    if pretty {
                        let next = bytes[position + 1..]
                            .iter()
                            .copied()
                            .find(|b| !b.is_ascii_whitespace());
                        if next != Some(if byte == b'{' { b'}' } else { b']' }) {
                            output.indent(depth)?;
                        }
                    }
                }
                b'}' | b']' => {
                    depth = depth.saturating_sub(1);
                    if pretty && !matches!(previous, b'{' | b'[') {
                        output.indent(depth)?;
                    }
                    output.byte(byte)?;
                }
                b',' => {
                    output.byte(byte)?;
                    if pretty {
                        output.indent(depth)?;
                    }
                }
                b':' => {
                    output.byte(byte)?;
                    if pretty {
                        output.byte(b' ')?;
                    }
                }
                _ => output.byte(byte)?,
            }
            previous = byte;
            position += 1;
        }
        output.flush()
    }
}

struct Output<'a, W: Write> {
    writer: &'a mut W,
    buffer: Vec<u8>,
}

impl<W: Write> Output<'_, W> {
    fn byte(&mut self, byte: u8) -> io::Result<()> {
        self.buffer.push(byte);
        if self.buffer.len() >= 65536 {
            self.flush()?;
        }
        Ok(())
    }

    fn extend(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() >= 65536 {
            self.flush()?;
            self.writer.write_all(bytes)
        } else {
            if self.buffer.len() + bytes.len() > 65536 {
                self.flush()?;
            }
            self.buffer.extend_from_slice(bytes);
            Ok(())
        }
    }

    fn indent(&mut self, depth: usize) -> io::Result<()> {
        self.byte(b'\n')?;
        for _ in 0..depth * 2 {
            self.byte(b' ')?;
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.write_all(&self.buffer)?;
        self.buffer.clear();
        Ok(())
    }
}
