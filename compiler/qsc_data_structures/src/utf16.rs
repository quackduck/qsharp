// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Position {
    pub encoding: PositionEncoding,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PositionEncoding {
    Utf8,
    Utf16,
}

pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Position {
    #[must_use]
    pub fn utf16(contents: &str, utf8_byte_offset: u32) -> Self {
        let target = utf8_byte_offset as usize;
        let mut utf16_column: u32 = 0;
        let mut line: u32 = 0;

        // chars        | 𝑓                 (        𝑥                 ⃗        )
        // char indices | 0                 1        2                 3        4    5
        // code points  | 1d453             28       1d465             20d7     29
        // utf-8 units  | f09d9193          28       f09d91a5          e28397   29
        // utf-16 units | d835     dc53     28       d835     dc65     20d7     29

        let mut chars = contents.char_indices().peekable();
        while let Some((char_index, c)) = chars.next() {
            let next = match chars.peek() {
                Some((next_offset, _)) => *next_offset,
                None => char_index + c.len_utf8(), // eof is a valid offset
            };
            // We're about to move past the requested offset
            if next > target {
                break;
            }

            // Windows (\r\n) line endings will be handled
            // fine here, with the \r at the end counting as
            // an extra char.
            if c == '\n' {
                line += 1;
                utf16_column = 0;
            } else {
                utf16_column +=
                    u32::try_from(c.len_utf16()).expect("character length should fit in u32");
            }
        }

        Position {
            encoding: PositionEncoding::Utf16,
            line,
            column: utf16_column,
        }
    }

    #[must_use]
    pub fn utf8(contents: &str, utf8_byte_offset: u32) -> Self {
        let target = utf8_byte_offset as usize;
        let mut column: u32 = 0;
        let mut line: u32 = 0;

        let mut chars = contents.char_indices().peekable();
        while let Some((char_index, c)) = chars.next() {
            let next = match chars.peek() {
                Some((next_offset, _)) => *next_offset,
                None => char_index + c.len_utf8(), // eof is a valid offset
            };

            // We're about to move past the requested offset
            if next > target {
                break;
            }

            // Windows (\r\n) line endings will be handled
            // fine here, with the \r at the end counting as
            // an extra char.
            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += u32::try_from(c.len_utf8()).expect("character length should fit in u32");
            }
        }

        Position {
            encoding: PositionEncoding::Utf8,
            line,
            column,
        }
    }

    /// # Panics
    /// Panics if position falls out of valid range for the given contents
    #[must_use]
    pub fn to_offset(&self, contents: &str) -> u32 {
        let mut column: u32 = 0;
        let mut line: u32 = 0;

        //  l    i    n    e    1    \n   l    i    n    e    2    <eof>
        //  0    1    2    3    4    5    6    7    8    9    10   11
        //  0,0  0,1  0,2  0,3  0,4  0,5  1,0  1,1  1,2  1,3  1,4  1,5

        for (char_index, c) in contents.char_indices() {
            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += u32::try_from(c.len_utf16()).expect("character length should fit in u32");
            }

            if line > self.line || (line == self.line && column > self.column) {
                // We moved past the requested line+column
                return u32::try_from(char_index).expect("offset should fit in u32");
            }
        }

        // return eof if we move past the end of the string
        u32::try_from(contents.len()).expect("expected length to fit in u32")
    }
}

#[cfg(test)]
mod test {
    use expect_test::expect;

    use super::{Position, PositionEncoding};

    #[test]
    fn one_line() {
        let contents = "Hello, world!";
        let offsets = [0, 1, 2, 13];
        let positions = offsets
            .iter()
            .map(|offset| Position::utf16(contents, *offset))
            .collect::<Vec<_>>();
        assert_eq!(positions, from(&[(0, 0), (0, 1), (0, 2), (0, 13)]));
    }

    #[test]
    fn lines() {
        let contents = "Hello, world!\nHello, world!\nHello, world!";
        let offsets = [13, 14, 29];
        let positions = offsets
            .iter()
            .map(|offset| Position::utf16(contents, *offset))
            .collect::<Vec<_>>();
        assert_eq!(positions, from(&[(0, 13), (1, 0), (2, 1)]));
    }

    #[test]
    fn newline_at_end() {
        let contents = "Hello, world!\n";
        let offsets = [12, 13, 14];
        let positions = offsets
            .iter()
            .map(|offset| Position::utf16(contents, *offset))
            .collect::<Vec<_>>();
        assert_eq!(positions, from(&[(0, 12), (0, 13), (1, 0)]));
    }

    #[test]
    fn utf_8_multibyte() {
        // utf-8 encoding has multi-unit characters, utf-16 doesn't
        // string       | ççç
        // chars        | ç        ç        ç
        // code points  | e7       e7       e7
        // utf-8 units  | c3a7     c3a7     c3a7
        // utf-16 units | e7       e7       e7
        let contents = "ççç\nççç";
        check_all_offsets(
            contents,
            &expect![[r#"
                [
                    "0:0",
                    "0:0",
                    "0:1",
                    "0:1",
                    "0:2",
                    "0:2",
                    "0:3",
                    "1:0",
                    "1:0",
                    "1:1",
                    "1:1",
                    "1:2",
                    "1:2",
                    "1:3",
                ]
            "#]],
        );
    }

    #[test]
    fn utf_8_multibyte_utf_16_surrogate() {
        // both encodings have multi-unit characters
        // string       | 𝑓𝑓
        // chars        | 𝑓                 𝑓
        // code points  | 1d453             1d453
        // utf-8 units  | f09d9193          f09d9193
        // utf-16 units | d835     dc53     d835     dc53

        let contents = "𝑓𝑓\n𝑓𝑓";
        check_all_offsets(
            contents,
            &expect![[r#"
            [
                "0:0",
                "0:0",
                "0:0",
                "0:0",
                "0:2",
                "0:2",
                "0:2",
                "0:2",
                "0:4",
                "1:0",
                "1:0",
                "1:0",
                "1:0",
                "1:2",
                "1:2",
                "1:2",
                "1:2",
                "1:4",
            ]
        "#]],
        );
    }

    #[test]
    fn grapheme_clusters() {
        // grapheme clusters, both encodings have multi-unit characters
        // string       | 𝑓(𝑥⃗) ≔ Σᵢ 𝑥ᵢ 𝑟ᵢ
        // chars        | 𝑓                 (        𝑥                 ⃗        )                 ≔                 Σ        ᵢ                 𝑥                 ᵢ                 𝑟                 ᵢ
        // code points  | 1d453             28       1d465             20d7     29       20       2254     20       3a3      1d62     20       1d465             1d62     20       1d45f             1d62
        // utf-8 units  | f09d9193          28       f09d91a5          e28397   29       20       e28994   20       cea3     e1b5a2   20       f09d91a5          e1b5a2   20       f09d919f          e1b5a2
        // utf-16 units | d835     dc53     28       d835     dc65     20d7     29       20       2254     20       3a3      1d62     20       d835     dc65     1d62     20       d835     dc5f     1d62

        let contents = "𝑓(𝑥⃗) ≔ Σᵢ 𝑥ᵢ 𝑟ᵢ";
        check_all_offsets(
            contents,
            &expect![[r#"
            [
                "0:0",
                "0:0",
                "0:0",
                "0:0",
                "0:2",
                "0:3",
                "0:3",
                "0:3",
                "0:3",
                "0:5",
                "0:5",
                "0:5",
                "0:6",
                "0:7",
                "0:8",
                "0:8",
                "0:8",
                "0:9",
                "0:10",
                "0:10",
                "0:11",
                "0:11",
                "0:11",
                "0:12",
                "0:13",
                "0:13",
                "0:13",
                "0:13",
                "0:15",
                "0:15",
                "0:15",
                "0:16",
                "0:17",
                "0:17",
                "0:17",
                "0:17",
                "0:19",
                "0:19",
                "0:19",
                "0:20",
            ]
        "#]],
        );
    }

    #[test]
    fn to_offset() {
        let contents = "line1\nline2";
        let positions = [
            (0, 0),
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
            (0, 5),
            (1, 0),
            (1, 1),
            (1, 2),
            (1, 3),
            (1, 4),
            (1, 5),
        ]
        .iter()
        .map(|p| Position {
            encoding: PositionEncoding::Utf16,
            line: p.0,
            column: p.1,
        });
        expect![[r#"
            [
                0,
                1,
                2,
                3,
                4,
                5,
                6,
                7,
                8,
                9,
                10,
                11,
            ]
        "#]]
        .assert_debug_eq(&positions.map(|p| p.to_offset(contents)).collect::<Vec<_>>());
    }

    fn check_all_offsets(contents: &str, expected: &expect_test::Expect) {
        let byte_offsets = 0..=contents.len();
        let positions = byte_offsets
            .map(|offset| {
                Position::utf16(
                    contents,
                    u32::try_from(offset).expect("offset should fit in u32"),
                )
            })
            .map(|pos| format!("{}:{}", pos.line, pos.column))
            .collect::<Vec<_>>();
        expected.assert_debug_eq(&positions);
    }

    fn from(pos: &[(u32, u32)]) -> Vec<Position> {
        pos.iter()
            .map(|(line, column)| Position {
                encoding: PositionEncoding::Utf16,
                line: *line,
                column: *column,
            })
            .collect()
    }
}
