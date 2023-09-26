// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::{Diagnostic, MietteError, MietteSpanContents, SourceCode, SourceSpan, SpanContents};
use qsc_fs_util::{Source, SourceMap};
use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
};

#[derive(Clone, Debug)]
pub struct WithSource<E>
where
    E: Diagnostic,
{
    sources: Vec<Source>,
    error: E,
}

impl<E: Diagnostic> WithSource<E> {
    pub fn error(&self) -> &E {
        &self.error
    }
}

impl<E: Diagnostic> WithSource<E> {
    /// Construct a diagnostic with source information from a source map.
    /// Since errors may contain labeled spans from any source file in the
    /// compilation, the entire source map is needed to resolve offsets.
    /// # Panics
    ///
    /// This function will panic if compiler state is invalid or in out-of-memory conditions.
    pub fn from_map(sources: &SourceMap, error: E) -> Self {
        // Filter the source map to the relevant sources
        // to avoid cloning all of them.
        let mut filtered = Vec::<Source>::new();

        for offset in error
            .labels()
            .into_iter()
            .flatten()
            .map(|label| u32::try_from(label.offset()).expect("offset should fit into u32"))
        {
            let source = sources
                .find_by_offset(offset)
                .expect("expected to find source at offset");

            // Keep the vector sorted by source offsets
            match filtered.binary_search_by_key(&source.offset, |s| s.offset) {
                Ok(_) => {} // source already in vector
                Err(pos) => filtered.insert(pos, source.clone()),
            }
        }

        Self {
            sources: filtered,
            error,
        }
    }
}

impl<E: Diagnostic> Error for WithSource<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.error.source()
    }
}

impl<E: Diagnostic + Send + Sync> Diagnostic for WithSource<E> {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.error.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        self.error.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(self)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.error.labels()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        self.error.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.error.diagnostic_source()
    }
}

impl<E: Diagnostic + Display> Display for WithSource<E> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}

impl<E: Diagnostic + Sync + Send> SourceCode for WithSource<E> {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        let offset = u32::try_from(span.offset()).expect("expected the offset to fit into u32");
        let source = self
            .sources
            .iter()
            .rev()
            .find(|source| offset >= source.offset)
            .expect("expected to find source at span");

        let contents = source.contents.read_span(
            &with_offset(span, |o| o - (source.offset as usize)),
            context_lines_before,
            context_lines_after,
        )?;

        Ok(Box::new(MietteSpanContents::new_named(
            source.name.to_string(),
            contents.data(),
            with_offset(contents.span(), |o| o + (source.offset as usize)),
            contents.line(),
            contents.column(),
            contents.line_count(),
        )))
    }
}

fn with_offset(span: &SourceSpan, f: impl FnOnce(usize) -> usize) -> SourceSpan {
    SourceSpan::new(f(span.offset()).into(), span.len().into())
}
