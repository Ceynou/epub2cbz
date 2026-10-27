use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SimpleStylesheet {
    rules: HashMap<String, HashMap<String, String>>,
}

impl SimpleStylesheet {
    pub fn parse(css: &str) -> Self {
        let mut rules = HashMap::new();
        let mut chars = css.chars().peekable();
        let mut current_pos = 0;

        while current_pos < css.len() {
            // Skip whitespace and comments
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    chars.next();
                    current_pos += c.len_utf8();
                } else if c == '/' {
                    chars.next();
                    current_pos += 1;
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        current_pos += 1;
                        loop {
                            match chars.next() {
                                Some('*') if chars.peek() == Some(&'/') => {
                                    chars.next();
                                    current_pos += 2;
                                    break;
                                }
                                Some(ch) => current_pos += ch.len_utf8(),
                                None => break,
                            }
                        }
                    }
                } else {
                    break;
                }
            }

            // Read selector
            let mut selector = String::new();
            while let Some(&c) = chars.peek() {
                if c == '{' {
                    break;
                }
                selector.push(c);
                chars.next();
                current_pos += c.len_utf8();
            }
            let selector = selector.trim().to_string();

            if selector.is_empty() {
                break;
            }

            // Skip '{'
            if chars.peek() == Some(&'{') {
                chars.next();
                current_pos += 1;
            } else {
                break;
            }

            // Read properties
            let mut properties = HashMap::new();
            let mut prop_str = String::new();
            let mut brace_depth = 1;
            while let Some(&c) = chars.peek() {
                if c == '{' {
                    brace_depth += 1;
                    prop_str.push(c);
                } else if c == '}' {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        chars.next();
                        current_pos += 1;
                        break;
                    }
                    prop_str.push(c);
                } else {
                    prop_str.push(c);
                }
                chars.next();
                current_pos += c.len_utf8();
            }

            for declaration in prop_str.split(';') {
                let declaration = declaration.trim();
                if let Some(colon_pos) = declaration.find(':') {
                    let prop = declaration[..colon_pos].trim().to_lowercase();
                    let val = declaration[colon_pos + 1..].trim().to_string();
                    properties.insert(prop, val);
                }
            }

            rules.insert(selector, properties);
        }

        Self { rules }
    }

    pub fn get_background_image(&self, selector: &str) -> Option<String> {
        let properties = self.rules.get(selector)?;
        let bg = properties.get("background-image")?;
        extract_url(bg)
    }
}

fn extract_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.starts_with("url(") && trimmed.ends_with(')') {
        let inner = &trimmed[4..trimmed.len() - 1];
        let inner = inner.trim().trim_matches(|c| c == '\'' || c == '"');
        Some(inner.to_string())
    } else {
        None
    }
}
