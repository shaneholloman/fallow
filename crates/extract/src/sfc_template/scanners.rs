pub(super) fn scan_curly_section(
    source: &str,
    start: usize,
    opening_len: usize,
    closing_len: usize,
) -> Option<(&str, usize)> {
    debug_assert_eq!(source.as_bytes().get(start), Some(&b'{'));
    debug_assert!(opening_len == 1 || opening_len == 2);
    debug_assert!(closing_len == 1 || closing_len == 2);

    let bytes = source.as_bytes();
    let mut index = start + opening_len;
    let mut nested_braces = 0_u32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while index < bytes.len() {
        let byte = bytes[index];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if escape {
            escape = false;
            index += 1;
            continue;
        }

        if in_single {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'\'' {
                in_single = false;
            }
            index += 1;
            continue;
        }

        if in_double {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_double = false;
            }
            index += 1;
            continue;
        }

        if in_backtick {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'`' {
                in_backtick = false;
            }
            index += 1;
            continue;
        }

        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            line_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            block_comment = true;
            index += 2;
            continue;
        }

        match byte {
            b'\'' => {
                in_single = true;
                index += 1;
            }
            b'"' => {
                in_double = true;
                index += 1;
            }
            b'`' => {
                in_backtick = true;
                index += 1;
            }
            b'{' => nested_braces += 1,
            b'}' => {
                if nested_braces == 0 {
                    if closing_len == 1 {
                        return Some((&source[start + opening_len..index], index + 1));
                    }
                    if bytes.get(index + 1) == Some(&b'}') {
                        return Some((&source[start + opening_len..index], index + 2));
                    }
                } else {
                    nested_braces -= 1;
                }
            }
            _ => {}
        }

        index += 1;
    }

    None
}

pub(super) fn scan_html_tag(source: &str, start: usize) -> Option<(&str, usize)> {
    debug_assert_eq!(source.as_bytes().get(start), Some(&b'<'));

    let bytes = source.as_bytes();
    let mut index = start + 1;
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if escape {
            escape = false;
            index += 1;
            continue;
        }

        if in_single {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'\'' {
                in_single = false;
            }
            index += 1;
            continue;
        }

        if in_double {
            if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_double = false;
            }
            index += 1;
            continue;
        }

        match byte {
            b'\'' => {
                in_single = true;
                index += 1;
            }
            b'"' => {
                in_double = true;
                index += 1;
            }
            b'>' => return Some((&source[start..=index], index + 1)),
            _ => index += 1,
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{scan_curly_section, scan_html_tag};

    #[test]
    fn scans_svelte_brace_section_with_nested_literals() {
        let source = "{handler({ key: `}` })}";
        let (inner, next_index) = scan_curly_section(source, 0, 1, 1).expect("brace section");
        assert_eq!(inner, "handler({ key: `}` })");
        assert_eq!(next_index, source.len());
    }

    #[test]
    fn scans_vue_interpolation_with_nested_comments() {
        let source = "{{ format(/* } */ value) }}";
        let (inner, next_index) = scan_curly_section(source, 0, 2, 2).expect("interpolation");
        assert_eq!(inner, " format(/* } */ value) ");
        assert_eq!(next_index, source.len());
    }

    #[test]
    fn scans_curly_sections_with_quoted_braces() {
        let source = r#"{format("}")}"#;
        let (inner, next_index) = scan_curly_section(source, 0, 1, 1).expect("expression");
        assert_eq!(inner, r#"format("}")"#);
        assert_eq!(next_index, source.len());
    }

    #[test]
    fn scans_html_tags_with_quoted_angle_brackets() {
        let source = r#"<Comp title="a > b" data-id='x>y'>"#;
        let (tag, next_index) = scan_html_tag(source, 0).expect("tag");
        assert_eq!(tag, source);
        assert_eq!(next_index, source.len());
    }
}
