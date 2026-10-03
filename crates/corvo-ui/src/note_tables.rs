#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TableAlign {
    Left,
    Center,
    Right,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TableCell {
    pub start: usize,
    pub text: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TableRowKind {
    Header,
    Delimiter,
    Body,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TableRow {
    pub start: usize,
    pub end: usize,
    pub cells: Vec<TableCell>,
    pub align: Vec<TableAlign>,
    pub kind: TableRowKind,
}

fn cells(line: &str) -> Option<Vec<TableCell>> {
    let chars: Vec<char> = line.chars().collect();
    let mut pipes = Vec::new();
    let mut code = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            i = (i + 2).min(chars.len());
            continue;
        }
        if chars[i] == '`' {
            let start = i;
            while i < chars.len() && chars[i] == '`' {
                i += 1;
            }
            let count = i - start;
            if code == count {
                code = 0;
            } else if code == 0 {
                let mut next = i;
                let mut matched = false;
                while next < chars.len() {
                    if chars[next] != '`' {
                        next += 1;
                        continue;
                    }
                    let begin = next;
                    while next < chars.len() && chars[next] == '`' {
                        next += 1;
                    }
                    if next - begin == count {
                        matched = true;
                        break;
                    }
                }
                if matched {
                    code = count;
                }
            }
            continue;
        }
        if chars[i] == '|' && code == 0 {
            pipes.push(i);
        }
        i += 1;
    }
    if pipes.is_empty() {
        return None;
    }
    let first = pipes[0];
    let last = *pipes.last().unwrap();
    let leading = chars[..first].iter().all(|c| c.is_whitespace());
    let trailing = chars[last + 1..].iter().all(|c| c.is_whitespace());
    let mut boundaries = vec![0];
    boundaries.extend(pipes.iter().map(|index| index + 1));
    let mut ends = pipes;
    ends.push(chars.len());
    let mut result = Vec::new();
    for (index, (start, end)) in boundaries.into_iter().zip(ends).enumerate() {
        if (index == 0 && leading) || (end == chars.len() && trailing) {
            continue;
        }
        let trim_start = start
            + chars[start..end]
                .iter()
                .take_while(|c| c.is_whitespace())
                .count();
        let trim_end = end
            - chars[trim_start..end]
                .iter()
                .rev()
                .take_while(|c| c.is_whitespace())
                .count();
        result.push(TableCell {
            start: trim_start,
            text: chars[trim_start..trim_end].iter().collect(),
        });
    }
    Some(result)
}
fn alignment(cell: &TableCell) -> Option<TableAlign> {
    let text = cell.text.as_str();
    let left = text.starts_with(':');
    let right = text.ends_with(':');
    let dashes = text.strip_prefix(':').unwrap_or(text);
    let dashes = dashes.strip_suffix(':').unwrap_or(dashes);
    if dashes.len() < 3 || !dashes.chars().all(|c| c == '-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => TableAlign::Center,
        (_, true) => TableAlign::Right,
        _ => TableAlign::Left,
    })
}
pub(super) fn table_rows(lines: &[String]) -> Vec<Option<TableRow>> {
    let code = super::note_syntax::code_rows(lines);
    let mut result = vec![None; lines.len()];
    let mut index = 0;
    while index + 1 < lines.len() {
        if code[index].is_some() || code[index + 1].is_some() {
            index += 1;
            continue;
        }
        let Some(header) = cells(&lines[index]).filter(|cells| !cells.is_empty()) else {
            index += 1;
            continue;
        };
        let Some(delimiter) = cells(&lines[index + 1]).filter(|cells| cells.len() == header.len())
        else {
            index += 1;
            continue;
        };
        let Some(align) = delimiter.iter().map(alignment).collect::<Option<Vec<_>>>() else {
            index += 1;
            continue;
        };
        let start = index;
        let mut rows = vec![
            (TableRowKind::Header, header),
            (TableRowKind::Delimiter, delimiter),
        ];
        index += 2;
        while index < lines.len() && code[index].is_none() {
            let Some(mut body) = cells(&lines[index]) else {
                break;
            };
            body.truncate(align.len());
            body.resize_with(align.len(), || TableCell {
                start: lines[index].chars().count(),
                text: String::new(),
            });
            rows.push((TableRowKind::Body, body));
            index += 1;
        }
        let end = index - 1;
        for (offset, (kind, cells)) in rows.into_iter().enumerate() {
            result[start + offset] = Some(TableRow {
                start,
                end,
                cells,
                align: align.clone(),
                kind,
            });
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rows(lines: &[&str]) -> Vec<Option<TableRow>> {
        table_rows(&lines.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    #[test]
    fn aligns_and_tracks_ranges() {
        let r = rows(&[
            "before",
            "| a | b | c |",
            "| :--- | :---: | ---: |",
            "| 1 | 2 | 3 |",
            "after",
        ]);
        for row in r[1..=3].iter().flatten() {
            assert_eq!((row.start, row.end), (1, 3));
            assert_eq!(
                row.align,
                vec![TableAlign::Left, TableAlign::Center, TableAlign::Right]
            );
        }
        assert_eq!(r[1].as_ref().unwrap().kind, TableRowKind::Header);
        assert_eq!(r[2].as_ref().unwrap().kind, TableRowKind::Delimiter);
        assert_eq!(r[3].as_ref().unwrap().kind, TableRowKind::Body);
        assert!(r[4].is_none());
    }
    #[test]
    fn preserves_unicode_columns_and_escaped_code_pipes() {
        let r = rows(&["| café | a\\|b | `x|y` |", "| --- | --- | --- |"]);
        let c = &r[0].as_ref().unwrap().cells;
        assert_eq!(
            c[0],
            TableCell {
                start: 2,
                text: "café".into()
            }
        );
        assert_eq!(
            c[1],
            TableCell {
                start: 9,
                text: "a\\|b".into()
            }
        );
        assert_eq!(c[2].text, "`x|y`");
    }
    #[test]
    fn normalizes_body_columns() {
        let r = rows(&["a | b | c", "--- | --- | ---", "x | y", "1 | 2 | 3 | 4"]);
        let c = &r[2].as_ref().unwrap().cells;
        assert_eq!(c.len(), 3);
        assert_eq!(
            c[2],
            TableCell {
                start: 5,
                text: String::new()
            }
        );
        assert_eq!(r[3].as_ref().unwrap().cells.len(), 3);
    }
    #[test]
    fn rejects_bad_delimiters_and_fenced_tables() {
        for separator in ["-- | ---", "--- | ::::---", "--- | --- | ---", "--- | x"] {
            assert!(rows(&["a | b", separator]).iter().all(Option::is_none));
        }
        assert!(rows(&["```md", "a | b", "--- | ---", "```"])
            .iter()
            .all(Option::is_none));
        assert!(rows(&["~~~", "a | b", "--- | ---"])
            .iter()
            .all(Option::is_none));
        assert!(table_rows(&[]).is_empty());
    }
}
