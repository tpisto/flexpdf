pub(super) fn split_lines_from_text(
    full_text: &str,
    line_texts: &[&str],
) -> Vec<Option<(usize, usize)>> {
    let mut ranges = Vec::new();
    let mut cursor = 0usize;

    for &line_text in line_texts {
        if line_text.is_empty() {
            ranges.push(None);
            continue;
        }
        if cursor >= full_text.len() {
            ranges.push(None);
            continue;
        }

        let line_start = cursor;
        let mut line_end = if full_text[cursor..].starts_with(line_text) {
            cursor + line_text.len()
        } else {
            advance_by_chars(full_text, cursor, line_text.chars().count())
        };

        if line_end > full_text.len() {
            line_end = full_text.len();
        }

        ranges.push(Some((line_start, line_end)));
        cursor = skip_whitespace(full_text, line_end);
    }

    ranges
}

fn advance_by_chars(text: &str, start: usize, count: usize) -> usize {
    if count == 0 || start >= text.len() {
        return start;
    }

    let mut idx = start;
    let mut remaining = count;
    while remaining > 0 && idx < text.len() {
        let ch = text[idx..].chars().next().unwrap();
        idx += ch.len_utf8();
        remaining -= 1;
    }
    idx
}

fn skip_whitespace(text: &str, start: usize) -> usize {
    let mut idx = start;
    while idx < text.len() {
        let ch = text[idx..].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        idx += ch.len_utf8();
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::split_lines_from_text;

    #[test]
    fn split_lines_from_text_handles_trailing_whitespace() {
        let full_text = "This is normal text with italic emphasis and bold importance mixed in the same paragraph.";
        let line_texts = vec![
            "This is normal text with italic emphasis and bold importance mixed in the",
            "same paragraph.",
        ];
        let ranges = split_lines_from_text(full_text, &line_texts);
        assert_eq!(ranges.len(), 2);
        let first = ranges[0].expect("first line range");
        let second = ranges[1].expect("second line range");
        assert_eq!(&full_text[first.0..first.1], line_texts[0]);
        assert_eq!(&full_text[second.0..second.1], line_texts[1]);
    }
}
