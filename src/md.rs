use crate::highlight;
pub const PAGES: &[(&str, &str)] = &[
    ("index", include_str!("../help/index.md")),
    ("screen", include_str!("../help/screen.md")),
    ("flying", include_str!("../help/flying.md")),
    ("commands", include_str!("../help/commands.md")),
    ("building", include_str!("../help/building.md")),
    ("edit", include_str!("../help/edit.md")),
    ("saving", include_str!("../help/saving.md")),
    ("stories", include_str!("../help/stories.md")),
    ("space", include_str!("../help/space.md")),
    ("vr", include_str!("../help/vr.md")),
    ("more", include_str!("../help/more.md")),
    ("writing", include_str!("../help/writing.md")),
];
const RESET: &str = "\x1b[0m";
pub fn cells(c: char) -> usize {
    match c as u32 {
        0..=31 | 0x300..=0x36f | 0xfe00..=0xfe0f | 0x200d => 0,
        0x1100..=0x115f
        | 0x2e80..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xff01..=0xff60
        | 0x1f300..=0x1faff
        | 0x20000..=0x3ffff => 2,
        _ => 1,
    }
}
pub fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            for c in it.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c)
        }
    }
    out
}
pub fn wrap(s: &str, w: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    let mut col = 0;
    let mut it = s.chars().peekable();
    let mut styles = String::new();
    while let Some(c) = it.next() {
        if c == '\x1b' && it.peek() == Some(&'[') {
            let mut style = String::from(c);
            for c in it.by_ref() {
                style.push(c);
                if c == 'm' {
                    break;
                }
            }
            if style == RESET {
                styles.clear()
            } else {
                styles.push_str(&style)
            }
            line.push_str(&style);
            continue;
        }
        let n = cells(c);
        if col + n > w.max(1) && col > 0 {
            line.push_str(RESET);
            lines.push(line);
            line = styles.clone();
            col = 0
        }
        line.push(c);
        col += n;
    }
    line.push_str(RESET);
    lines.push(line);
    lines
}
fn inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap();
        if c == '[' {
            if let Some((label, tail)) = rest[1..].split_once("](") {
                if let Some((url, after)) = tail.split_once(')') {
                    out.push_str(&format!("\x1b[4;36m{label}{RESET} ({url})"));
                    rest = after;
                    continue;
                }
            }
        }
        let token = if rest.starts_with("**") {
            Some(("**", "\x1b[1m"))
        } else if c == '\u{60}' {
            Some(("\u{60}", "\x1b[38;2;230;196;120m"))
        } else if c == '*' {
            Some(("*", "\x1b[3m"))
        } else {
            None
        };
        if let Some((mark, style)) = token {
            if let Some((inside, after)) = rest[mark.len()..].split_once(mark) {
                out.push_str(style);
                out.push_str(inside);
                out.push_str(RESET);
                rest = after;
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}
fn table(rows: &[&str], width: usize) -> Vec<String> {
    let values: Vec<Vec<String>> = rows
        .iter()
        .map(|l| {
            l.trim()
                .trim_matches('|')
                .split('|')
                .map(|s| inline(s.trim()))
                .collect()
        })
        .filter(|r: &Vec<String>| {
            !r.iter()
                .all(|v| v.chars().all(|c| matches!(c, '-' | ':' | ' ')))
        })
        .collect();
    let count = values.iter().map(Vec::len).max().unwrap_or(1);
    let mut widths = vec![1; count];
    for row in &values {
        for (i, v) in row.iter().enumerate() {
            widths[i] = widths[i].max(plain(v).chars().map(cells).sum())
        }
    }
    let room = width.saturating_sub(3 * (count - 1)).max(count);
    while widths.iter().sum::<usize>() > room {
        let i = (0..count).max_by_key(|&i| widths[i]).unwrap();
        widths[i] -= 1;
    }
    let mut out = vec![];
    for (r, row) in values.iter().enumerate() {
        let chunks: Vec<_> = (0..count)
            .map(|i| wrap(row.get(i).map_or("", String::as_str), widths[i]))
            .collect();
        for j in 0..chunks.iter().map(Vec::len).max().unwrap_or(0) {
            let mut line = String::new();
            for (i, col) in chunks.iter().enumerate() {
                if i > 0 {
                    line.push_str(" │ ")
                }
                let text = col.get(j).map_or("", String::as_str);
                line.push_str(text);
                line.push_str(
                    &" ".repeat(widths[i].saturating_sub(plain(text).chars().map(cells).sum())),
                )
            }
            out.extend(wrap(&line, width));
        }
        if r == 0 {
            out.extend(wrap(
                &widths
                    .iter()
                    .map(|n| "─".repeat(*n))
                    .collect::<Vec<_>>()
                    .join("─┼─"),
                width,
            ))
        }
    }
    out
}
pub fn document(s: &str, file: &str, width: usize) -> Vec<String> {
    let clean: String = s
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    let clean = clean.replace('\t', "    ");
    let ext = file.rsplit('.').next().unwrap_or("");
    let codefile = matches!(
        ext,
        "json"
            | "3draft"
            | "rs"
            | "js"
            | "mjs"
            | "ts"
            | "py"
            | "sh"
            | "css"
            | "go"
            | "c"
            | "cpp"
            | "html"
            | "xml"
            | "wat"
    );
    let raw: Vec<_> = clean.lines().collect();
    let mut out = vec![];
    let mut fence = None;
    let mut i = 0;
    while i < raw.len() {
        let l = raw[i];
        i += 1;
        if !codefile
            && (l.trim_start().starts_with("\u{60}\u{60}\u{60}")
                || l.trim_start().starts_with("~~~"))
        {
            fence = if fence.is_some() {
                None
            } else {
                Some(l.trim()[3..].trim())
            };
            continue;
        }
        let line = if let Some(lang) = fence.or(codefile.then_some(ext)) {
            highlight::code(l, lang)
        } else if l.trim().starts_with('|') {
            let start = i - 1;
            while i < raw.len() && raw[i].trim().starts_with('|') {
                i += 1
            }
            out.extend(table(&raw[start..i], width));
            continue;
        } else if l.starts_with('#') {
            format!(
                "\x1b[1;38;2;100;210;225m{}{RESET}",
                l.trim_start_matches('#').trim()
            )
        } else {
            inline(l)
        };
        out.extend(wrap(&line, width))
    }
    out
}
pub fn ansi(s: &str) -> String {
    document(s, "md", crate::terminal::size().0).join("\n") + "\n"
}
