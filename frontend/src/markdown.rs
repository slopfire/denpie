use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    static ref MARKDOWN_CACHE: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn safe_markdown_url(url: &str) -> &str {
    let url = url.trim();
    if url.starts_with("http:") || url.starts_with("https:") || url.starts_with("mailto:") || 
       url.starts_with('/') || url.starts_with('#') || url.starts_with('?') {
        url
    } else {
        "#"
    }
}

fn render_inline_markdown(value: &str) -> String {
    let mut text = value.to_string();
    let mut tokens = Vec::new();
    
    // Inline code
    let re_code = Regex::new(r"`([^`\n]+)`").unwrap();
    text = re_code.replace_all(&text, |caps: &regex::Captures| {
        let escaped = escape_html(&caps[1]);
        let token = format!("<code>{}</code>", escaped);
        let index = tokens.len();
        tokens.push(token);
        format!("\u{0000}{}\u{0000}", index)
    }).to_string();

    // Links
    let re_link = Regex::new(r"\[([^\]\n]+)\]\(([^)\s]+)\)").unwrap();
    text = re_link.replace_all(&text, |caps: &regex::Captures| {
        let label = &caps[1];
        let url = safe_markdown_url(&caps[2]);
        let rendered_label = render_basic_inline(label);
        let token = format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>", escape_html(url), rendered_label);
        let index = tokens.len();
        tokens.push(token);
        format!("\u{0000}{}\u{0000}", index)
    }).to_string();

    let mut result = render_basic_inline(&text);
    
    // Restore tokens
    for (i, token) in tokens.iter().enumerate() {
        let placeholder = format!("\u{0000}{}\u{0000}", i);
        result = result.replace(&placeholder, token);
    }
    
    result
}

fn render_basic_inline(raw: &str) -> String {
    let mut s = escape_html(raw);
    
    // Bold
    let re_bold1 = Regex::new(r"\*\*([\s\S]+?)\*\*").unwrap();
    s = re_bold1.replace_all(&s, "<strong>$1</strong>").to_string();
    let re_bold2 = Regex::new(r"__([\s\S]+?)__").unwrap();
    s = re_bold2.replace_all(&s, "<strong>$1</strong>").to_string();
    
    // Strikethrough
    let re_strike = Regex::new(r"~~([\s\S]+?)~~").unwrap();
    s = re_strike.replace_all(&s, "<del>$1</del>").to_string();
    
    // Italic
    let re_ital1 = Regex::new(r"(^|[^\*])\*([^*\n]+)\*").unwrap();
    s = re_ital1.replace_all(&s, "$1<em>$2</em>").to_string();
    let re_ital2 = Regex::new(r"(^|[^_])_([^_\n]+)_").unwrap();
    s = re_ital2.replace_all(&s, "$1<em>$2</em>").to_string();
    
    s
}

pub fn render_markdown(value: &str) -> String {
    if let Some(cached) = MARKDOWN_CACHE.lock().unwrap().get(value) {
        return cached.clone();
    }

    let lines: Vec<&str> = value.split('\n').collect();
    let mut html = Vec::new();
    let mut paragraph = Vec::new();
    let mut list_type = None;
    let mut list_items = Vec::new();
    let mut quote_lines = Vec::new();
    let mut in_code = false;
    let mut code_lines = Vec::new();

    let flush_paragraph = |html: &mut Vec<String>, paragraph: &mut Vec<&str>| {
        if paragraph.is_empty() { return; }
        let content = render_inline_markdown(&paragraph.join("\n")).replace('\n', "<br>");
        html.push(format!("<p>{}</p>", content));
        paragraph.clear();
    };

    let flush_list = |html: &mut Vec<String>, list_type: &mut Option<&str>, list_items: &mut Vec<&str>| {
        if let Some(t) = list_type {
            let items: String = list_items.iter().map(|item| format!("<li>{}</li>", render_inline_markdown(item))).collect();
            html.push(format!("<{}>{}</{}>", t, items, t));
            *list_type = None;
            list_items.clear();
        }
    };

    let flush_quote = |html: &mut Vec<String>, quote_lines: &mut Vec<&str>| {
        if quote_lines.is_empty() { return; }
        let content = render_inline_markdown(&quote_lines.join("\n")).replace('\n', "<br>");
        html.push(format!("<blockquote>{}</blockquote>", content));
        quote_lines.clear();
    };

    let flush_code = |html: &mut Vec<String>, code_lines: &mut Vec<&str>| {
        html.push(format!("<pre><code>{}</code></pre>", escape_html(&code_lines.join("\n"))));
        code_lines.clear();
    };

    let flush_blocks = |html: &mut Vec<String>, paragraph: &mut Vec<&str>, list_type: &mut Option<&str>, list_items: &mut Vec<&str>, quote_lines: &mut Vec<&str>| {
        flush_paragraph(html, paragraph);
        flush_list(html, list_type, list_items);
        flush_quote(html, quote_lines);
    };

    let re_heading = Regex::new(r"^(#{1,3})\s+(.+)$").unwrap();
    let re_unordered = Regex::new(r"^\s*[-*+]\s+(.+)$").unwrap();
    let re_ordered = Regex::new(r"^\s*\d+[.)]\s+(.+)$").unwrap();
    let re_quote = Regex::new(r"^\s*>\s?(.+)$").unwrap();

    for line in lines {
        if line.trim().starts_with("```") {
            if in_code {
                flush_code(&mut html, &mut code_lines);
                in_code = false;
            } else {
                flush_blocks(&mut html, &mut paragraph, &mut list_type, &mut list_items, &mut quote_lines);
                in_code = true;
            }
            continue;
        }
        
        if in_code {
            code_lines.push(line);
            continue;
        }
        
        if line.trim().is_empty() {
            flush_blocks(&mut html, &mut paragraph, &mut list_type, &mut list_items, &mut quote_lines);
            continue;
        }

        if let Some(caps) = re_heading.captures(line) {
            flush_blocks(&mut html, &mut paragraph, &mut list_type, &mut list_items, &mut quote_lines);
            let level = caps[1].len();
            html.push(format!("<h{}>{}</h{}>", level, render_inline_markdown(&caps[2]), level));
            continue;
        }

        let is_unordered = re_unordered.captures(line);
        let is_ordered = re_ordered.captures(line);
        if is_unordered.is_some() || is_ordered.is_some() {
            flush_paragraph(&mut html, &mut paragraph);
            flush_quote(&mut html, &mut quote_lines);
            let next_type = if is_unordered.is_some() { "ul" } else { "ol" };
            if list_type.is_some() && list_type != Some(next_type) {
                flush_list(&mut html, &mut list_type, &mut list_items);
            }
            list_type = Some(next_type);
            let content = if let Some(c) = is_unordered { c } else { is_ordered.unwrap() };
            list_items.push(content.get(1).unwrap().as_str());
            continue;
        }

        if let Some(caps) = re_quote.captures(line) {
            flush_paragraph(&mut html, &mut paragraph);
            flush_list(&mut html, &mut list_type, &mut list_items);
            quote_lines.push(caps.get(1).unwrap().as_str());
            continue;
        }

        flush_list(&mut html, &mut list_type, &mut list_items);
        flush_quote(&mut html, &mut quote_lines);
        paragraph.push(line);
    }

    if in_code { flush_code(&mut html, &mut code_lines); }
    flush_blocks(&mut html, &mut paragraph, &mut list_type, &mut list_items, &mut quote_lines);

    let result = html.join("");
    MARKDOWN_CACHE.lock().unwrap().insert(value.to_string(), result.clone());
    result
}
