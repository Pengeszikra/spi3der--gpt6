pub fn json(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                let mut t = String::from("\"");
                while let Some(x) = it.next() {
                    t.push(x);
                    if x == '\\' {
                        if let Some(y) = it.next() {
                            t.push(y)
                        }
                    } else if x == '"' {
                        break;
                    }
                }
                let key = it.clone().find(|x| !x.is_whitespace()) == Some(':');
                out.push_str(if key {
                    "\x1b[38;2;225;164;204m"
                } else {
                    "\x1b[38;2;55;210;140m"
                });
                out.push_str(&t);
                out.push_str("\x1b[0m");
            }
            '0'..='9' | '-' => {
                out.push_str("\x1b[38;2;255;176;95m");
                out.push(c);
                while it
                    .peek()
                    .is_some_and(|x| matches!(x, '0'..='9' | '.' | 'e' | 'E' | '+' | '-'))
                {
                    out.push(it.next().unwrap())
                }
                out.push_str("\x1b[0m");
            }
            '{' | '}' | '[' | ']' => {
                out.push_str("\x1b[38;2;255;145;45m");
                out.push(c);
                out.push_str("\x1b[0m");
            }
            c if c.is_control() && c != '\n' && c != '\t' => {}
            _ => out.push(c),
        }
    }
    out
}
pub fn code(s: &str, lang: &str) -> String {
    if matches!(lang, "json" | "3draft") {
        return json(s);
    }
    let words=match lang{
 "rust"|"rs"=>"as async await break const continue crate else enum extern fn for if impl in let loop match mod move mut pub ref return self Self static struct trait type unsafe use where while",
 "ts"|"tsx"|"js"|"mjs"|"javascript"|"typescript"=>"async await break case catch class const continue default else export extends for from function if import in interface let new of return switch throw try type var while yield",
 "sh"|"bash"|"zsh"=>"case do done echo elif else esac export fi for function git if in local npm cargo then while",
 "py"|"python"=>"and as async await class def elif else except for from if import in is lambda not or pass raise return try while with yield",
 "css"=>"color display width height margin padding background position",
 "c"|"cpp"|"go"=>"break case char const continue default else float for func if import int package return struct switch typedef void while",
 "html"|"xml"|"wat"=>"div span head body script style module func param result local export",_=>return s.into()};
    let mut out = String::new();
    let mut it = s.char_indices().peekable();
    while let Some((start, c)) = it.next() {
        let style = if (c == '/' && it.peek().is_some_and(|(_, c)| *c == '/'))
            || (c == '#' && matches!(lang, "python" | "py" | "sh" | "bash" | "zsh"))
        {
            out.push_str("\x1b[38;2;123;136;143m");
            out.push_str(&s[start..]);
            out.push_str("\x1b[0m");
            break;
        } else if matches!(c, '\'' | '"' | '\u{60}') {
            while let Some((_, v)) = it.next() {
                if v == '\\' {
                    it.next();
                } else if v == c {
                    break;
                }
            }
            Some("55;210;140")
        } else if c.is_ascii_digit() {
            while it
                .peek()
                .is_some_and(|(_, v)| v.is_ascii_hexdigit() || *v == '.')
            {
                it.next();
            }
            Some("255;176;95")
        } else if c.is_alphabetic() || c == '_' {
            while it
                .peek()
                .is_some_and(|(_, v)| v.is_alphanumeric() || *v == '_')
            {
                it.next();
            }
            let tok = &s[start..it.peek().map_or(s.len(), |x| x.0)];
            if words.split_whitespace().any(|w| w == tok) {
                Some("172;214;65")
            } else if matches!(tok, "true" | "false" | "null" | "None") {
                Some("225;164;204")
            } else {
                None
            }
        } else {
            None
        };
        let token = &s[start..it.peek().map_or(s.len(), |x| x.0)];
        if let Some(rgb) = style {
            out.push_str(&format!("\x1b[38;2;{rgb}m{token}\x1b[0m"))
        } else {
            out.push_str(token)
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors() {
        let s = json(r#"{"x":12,"y":"a"}"#);
        assert!(s.contains("225;164;204"));
        assert!(s.contains("55;210;140"));
        assert!(s.contains("255;176;95"));
    }
}
