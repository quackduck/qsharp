use std::sync::Arc;

#[derive(Debug, Default)]
pub struct SourceMap {
    sources: Vec<Source>,
    entry: Option<Source>,
}

impl SourceMap {
    pub fn new(
        sources: impl IntoIterator<Item = (SourceName, SourceContents)>,
        entry: Option<Arc<str>>,
    ) -> Self {
        let mut offset_sources = Vec::new();

        let entry_source = entry.map(|contents| Source {
            name: "<entry>".into(),
            contents,
            offset: 0,
        });

        let mut offset = next_offset(entry_source.as_ref());
        for (name, contents) in sources {
            let source = Source {
                name,
                contents,
                offset,
            };
            offset = next_offset(Some(&source));
            offset_sources.push(source);
        }

        Self {
            sources: offset_sources,
            entry: entry_source,
        }
    }

    pub fn push(&mut self, name: SourceName, contents: SourceContents) -> u32 {
        let offset = next_offset(self.sources.last());

        self.sources.push(Source {
            name,
            contents,
            offset,
        });

        offset
    }

    #[must_use]
    pub fn find_by_offset(&self, offset: u32) -> Option<&Source> {
        self.sources
            .iter()
            .rev()
            .chain(&self.entry)
            .find(|source| offset >= source.offset)
    }

    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.name.as_ref() == name)
    }
}
#[derive(Clone, Debug)]
pub struct Source {
    pub name: SourceName,
    pub contents: SourceContents,
    pub offset: u32,
}

pub type SourceName = Arc<str>;

pub type SourceContents = Arc<str>;
fn next_offset(last_source: Option<&Source>) -> u32 {
    // Leave a gap of 1 between each source so that offsets at EOF
    // get mapped to the correct source
    last_source.map_or(0, |s| {
        1 + s.offset + u32::try_from(s.contents.len()).expect("contents length should fit into u32")
    })
}
