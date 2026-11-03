use crate::{model::Removal, transform::Candidate};
use oxc_span::Span;
use sourcemap::SourceMapBuilder;

/// Retain source text and translate UTF-8 AST offsets to UTF-16 map columns.
pub struct Output<'a> {
    source: &'a str,
    filename: &'a str,
    lines: Vec<usize>,
    pub code: String,
    builder: Option<SourceMapBuilder>,
    line: u32,
    column: u32,
    pub removals: Vec<Removal>,
}

impl<'a> Output<'a> {
    pub fn new(source: &'a str, filename: &'a str, map: bool) -> Self {
        let mut lines = vec![0];
        for (offset, ch) in source.char_indices() {
            if ch == '\n'
                || ch == '\u{2028}'
                || ch == '\u{2029}'
                || (ch == '\r' && source.as_bytes().get(offset + 1) != Some(&b'\n'))
            {
                lines.push(offset + ch.len_utf8());
            }
        }
        let builder = map.then(|| {
            let mut builder = SourceMapBuilder::new(Some(filename));
            let id = builder.add_source(filename);
            builder.set_source_contents(id, Some(source));
            builder
        });
        Self {
            source,
            filename,
            lines,
            code: String::new(),
            builder,
            line: 0,
            column: 0,
            removals: vec![],
        }
    }

    fn append(&mut self, text: &str, original: Option<usize>) {
        let mut src_line = original
            .map(|offset| self.lines.partition_point(|start| *start <= offset) - 1)
            .unwrap_or(0);
        let mut src_column = original
            .map(|offset| {
                self.source[self.lines[src_line]..offset]
                    .encode_utf16()
                    .count() as u32
            })
            .unwrap_or(0);
        let mut characters = text.chars().peekable();
        while let Some(ch) = characters.next() {
            if let Some(builder) = &mut self.builder {
                builder.add(
                    self.line,
                    self.column,
                    src_line as u32,
                    src_column,
                    original.map(|_| self.filename),
                    None,
                    false,
                );
            }
            self.code.push(ch);
            if ch == '\n'
                || ch == '\u{2028}'
                || ch == '\u{2029}'
                || (ch == '\r' && characters.peek() != Some(&'\n'))
            {
                self.line += 1;
                self.column = 0;
                src_line += 1;
                src_column = 0;
            } else {
                self.column += ch.len_utf16() as u32;
                src_column += ch.len_utf16() as u32;
            }
        }
    }

    pub fn render(&mut self, range: Span, candidates: &[Candidate]) {
        let mut cursor = range.start;
        for candidate in candidates {
            if candidate.span.start < cursor || candidate.span.end > range.end {
                continue;
            }
            self.append(
                &self.source[cursor as usize..candidate.span.start as usize],
                Some(cursor as usize),
            );
            self.append(&candidate.prefix, None);
            if let Some(kept) = candidate.kept {
                self.render(kept, candidates);
            }
            self.append(&candidate.suffix, None);
            self.removals.push(Removal {
                start: candidate.span.start,
                end: candidate.span.end,
                kind: candidate.kind,
                features: candidate.features.clone(),
            });
            cursor = candidate.span.end;
        }
        self.append(
            &self.source[cursor as usize..range.end as usize],
            Some(cursor as usize),
        );
    }

    pub fn map(&mut self) -> Result<Option<serde_json::Value>, String> {
        self.builder
            .take()
            .map(|builder| {
                let mut buffer = Vec::new();
                builder
                    .into_sourcemap()
                    .to_writer(&mut buffer)
                    .map_err(|e| e.to_string())?;
                serde_json::from_slice(&buffer).map_err(|e| e.to_string())
            })
            .transpose()
    }
}
