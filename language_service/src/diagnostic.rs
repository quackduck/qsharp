// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    protocol::{ls_Diagnostic, DiagnosticSeverity, Range},
    qsc_utils::position,
    PositionEncodingKind,
};
use miette::{Diagnostic, Severity};
use qsc::SourceMap;
use std::{error::Error, fmt::Write, iter};

impl ls_Diagnostic {
    fn from(
        position_encoding_kind: PositionEncodingKind,
        source_map: &SourceMap,
        err: qsc::compile::Error,
    ) -> Self {
        let mut message = err.to_string();
        for source in iter::successors(err.source(), |e| e.source()) {
            write!(message, ": {source}").expect("message should be writable");
        }
        if let Some(help) = err.help() {
            write!(message, "\n\nhelp: {help}").expect("message should be writable");
        }

        let mut ranges = err.labels().into_iter().flatten().map(|l| {
            let (lo, hi) = (l.offset(), l.offset() + l.len());
            (
                l.label(),
                Range {
                    start: position(position_encoding_kind, source_map, lo as u32),
                    end: position(position_encoding_kind, source_map, hi as u32),
                },
            )
        });

        let range = ranges.next().map_or(
            Range {
                start: position(position_encoding_kind, source_map, 0),
                end: position(position_encoding_kind, source_map, 1),
            },
            |(l, r)| r,
        );

        let severity = match err.severity().unwrap_or(Severity::Error) {
            Severity::Error => DiagnosticSeverity::Error,
            Severity::Warning => DiagnosticSeverity::Warning,
            Severity::Advice => DiagnosticSeverity::Information,
        };

        let code = err.code().map(|code| code.to_string());

        ls_Diagnostic {
            range,
            severity,
            code,
            message,
            related_info: ranges
                .map(|(l, r)| (l.unwrap_or("").to_string(), r.start))
                .collect(),
        }
    }
}
