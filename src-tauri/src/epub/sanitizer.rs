use ammonia::Builder;
use std::collections::HashSet;

pub fn sanitize_html(html: &str) -> String {
    let mut schemes = HashSet::new();
    schemes.insert("http");
    schemes.insert("https");
    schemes.insert("data");

    let cleaned = Builder::default()
        .add_tags(&[
            "div", "span", "p", "br", "hr", "h1", "h2", "h3", "h4", "h5", "h6",
            "ul", "ol", "li", "dl", "dt", "dd", "table", "thead", "tbody", "tfoot",
            "tr", "th", "td", "caption", "colgroup", "col",
            "a", "img", "em", "strong", "b", "i", "u", "s", "sub", "sup",
            "blockquote", "pre", "code", "figure", "figcaption",
            "section", "article", "nav", "aside", "header", "footer",
            "ruby", "rt", "rp",
        ])
        .add_tag_attributes("img", &["src", "alt", "width", "height", "style", "class"])
        .add_tag_attributes("a", &["href", "title", "class"])
        .add_tag_attributes("div", &["class", "id", "style"])
        .add_tag_attributes("span", &["class", "id", "style"])
        .add_tag_attributes("p", &["class", "id", "style"])
        .add_tag_attributes("section", &["class", "id"])
        .add_tag_attributes("table", &["class", "border", "cellspacing", "cellpadding"])
        .add_tag_attributes("td", &["colspan", "rowspan", "class"])
        .add_tag_attributes("th", &["colspan", "rowspan", "class"])
        .add_tag_attributes("blockquote", &["class"])
        .add_tag_attributes("pre", &["class"])
        .add_tag_attributes("code", &["class"])
        .add_tag_attributes("h1", &["class", "id"])
        .add_tag_attributes("h2", &["class", "id"])
        .add_tag_attributes("h3", &["class", "id"])
        .add_tag_attributes("h4", &["class", "id"])
        .add_tag_attributes("h5", &["class", "id"])
        .add_tag_attributes("h6", &["class", "id"])
        .add_tag_attributes("ul", &["class"])
        .add_tag_attributes("ol", &["class"])
        .add_tag_attributes("li", &["class"])
        .add_tag_attributes("figure", &["class"])
        .add_tag_attributes("figcaption", &["class"])
        .add_tag_attributes("ruby", &["class"])
        .add_tag_attributes("rt", &["class"])
        .add_tag_attributes("rp", &["class"])
        .add_generic_attributes(&["class", "id", "style"])
        .url_schemes(schemes)
        .attribute_filter(|element, attribute, value| match (element, attribute) {
            ("img", "src") if !is_inline_image_src(value) => None,
            ("a", "href") if !is_safe_link_href(value) => None,
            _ => Some(value.into()),
        })
        .link_rel(None)
        .clean(html)
        .to_string();

    // ammonia 把 <head>/<meta charset> 全删了，浏览器不知道编码会默认 Latin-1 → 中文乱码。
    // 强制加 UTF-8 charset 声明。
    if !cleaned.contains("charset") {
        format!("<meta charset=\"UTF-8\">{}", cleaned)
    } else {
        cleaned
    }
}

fn is_inline_image_src(value: &str) -> bool {
    let trimmed = value.trim_start();
    if trimmed.is_empty() || trimmed.starts_with("//") {
        return false;
    }

    if let Some(colon) = trimmed.find(':') {
        let scheme = &trimmed[..colon];
        if has_url_scheme(scheme) {
            return scheme.eq_ignore_ascii_case("data")
                && trimmed[colon + 1..].starts_with("image/");
        }
    }
    true
}

fn is_safe_link_href(value: &str) -> bool {
    let trimmed = value.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    if let Some(colon) = trimmed.find(':') {
        let scheme = &trimmed[..colon];
        if has_url_scheme(scheme) {
            return scheme.eq_ignore_ascii_case("http")
                || scheme.eq_ignore_ascii_case("https");
        }
    }
    true
}

fn has_url_scheme(prefix: &str) -> bool {
    let mut chars = prefix.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_script_and_event_handlers() {
        let html = r#"<p onclick="alert(1)">hi<script>alert(2)</script></p>"#;
        let cleaned = sanitize_html(html);
        assert!(!cleaned.contains("<script"));
        assert!(!cleaned.contains("onclick"));
        assert!(cleaned.contains("hi"));
    }

    #[test]
    fn strips_file_urls() {
        let html = r#"<a href="file:///C:/Windows/win.ini">win</a>"#;
        let cleaned = sanitize_html(html);
        assert!(!cleaned.contains("file:"));
        assert!(cleaned.contains("win"));
    }

    #[test]
    fn keeps_inline_images_but_blocks_external_images() {
        let html = r#"<img src="data:image/png;base64,abc"><img src="https://tracker.example/1.png"><img src="images/cover.png">"#;
        let cleaned = sanitize_html(html);

        assert!(cleaned.contains("data:image/png;base64,abc"));
        assert!(cleaned.contains("images/cover.png"));
        assert!(!cleaned.contains("tracker.example"));
    }

    #[test]
    fn keeps_safe_links_but_blocks_data_hrefs() {
        let html = r#"<a href="data:text/html,hello">bad</a><a href="https://example.com">web</a><a href="chapter.xhtml">relative</a>"#;
        let cleaned = sanitize_html(html);

        assert!(cleaned.contains("https://example.com"));
        assert!(cleaned.contains("chapter.xhtml"));
        assert!(!cleaned.contains("data:text/html"));
    }
}
