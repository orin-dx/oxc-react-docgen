//! JSDoc extraction: find_jsdoc, extract_jsdoc_tags, parse_jsdoc_text.

use std::collections::BTreeMap;

use super::SourceDataCollector;

impl<'src> SourceDataCollector<'src> {
    // ─── JSDoc extraction ─────────────────────────────────────────────────────

    /// Find JSDoc comment immediately preceding the given byte offset.
    /// Returns empty string if none found.
    pub(super) fn find_jsdoc(&self, span_start: u32) -> String {
        const PROXIMITY_THRESHOLD: u32 = 120; // bytes — enough for blank lines + decorator

        let comment = self
            .comments
            .iter()
            .rev()
            .find(|c| c.is_block && c.span_end <= span_start && span_start - c.span_end <= PROXIMITY_THRESHOLD);

        match comment {
            Some(c) => {
                let raw = &self.source[c.span_start as usize..c.span_end as usize];
                parse_jsdoc_text(raw)
            }
            None => String::new(),
        }
    }

    /// Extract JSDoc @tags from the comment preceding the given byte offset.
    pub(super) fn extract_jsdoc_tags(&self, span_start: u32) -> BTreeMap<String, String> {
        const PROXIMITY_THRESHOLD: u32 = 120;

        let comment = self
            .comments
            .iter()
            .rev()
            .find(|c| c.is_block && c.span_end <= span_start && span_start - c.span_end <= PROXIMITY_THRESHOLD);

        match comment {
            Some(c) => {
                let raw = &self.source[c.span_start as usize..c.span_end as usize];
                extract_jsdoc_tags(raw)
            }
            None => BTreeMap::new(),
        }
    }
}

// ─── JSDoc parsing ────────────────────────────────────────────────────────────

pub(super) fn parse_jsdoc_text(raw: &str) -> String {
    // Strip `/**` prefix and `*/` suffix
    let inner = raw.trim_start_matches("/**").trim_end_matches("*/");

    let desc_lines: Vec<&str> = inner
        .lines()
        .map(|l| {
            let l = l.trim();
            // Strip leading `* ` or `*`
            let l = l.strip_prefix("* ").or_else(|| l.strip_prefix('*')).unwrap_or(l);
            l
        })
        .take_while(|l| !l.starts_with('@'))
        .collect();

    desc_lines.join("\n").trim().to_owned()
}

pub(super) fn extract_jsdoc_tags(raw: &str) -> BTreeMap<String, String> {
    let inner = raw.trim_start_matches("/**").trim_end_matches("*/");
    let mut tags: BTreeMap<String, String> = BTreeMap::new();
    let mut in_tags = false;

    for line in inner.lines() {
        let line = line.trim();
        let line = line.strip_prefix("* ").or_else(|| line.strip_prefix('*')).unwrap_or(line);
        let line = line.trim();

        if let Some(rest) = line.strip_prefix('@') {
            in_tags = true;
            // Parse tag: `@tagname rest`
            let (tag, value) = if let Some(sp) = rest.find(char::is_whitespace) {
                let tag = &rest[..sp];
                let value = rest[sp..].trim();
                (tag, value)
            } else {
                (rest, "")
            };

            // Special handling for @param — store as `param:propName`
            if tag == "param" {
                // `@param propName description` or `@param {type} propName description`
                let value = value.trim_start_matches('{');
                // Skip {type} if present
                let value =
                    if value.contains('}') { value.split_once('}').map(|x| x.1).unwrap_or("").trim() } else { value };
                // First word is the prop name
                if let Some(space) = value.find(char::is_whitespace) {
                    let prop_name = &value[..space];
                    let desc = value[space..].trim();
                    tags.insert(format!("param:{}", prop_name), desc.to_owned());
                } else if !value.is_empty() {
                    tags.insert(format!("param:{}", value), String::new());
                }
            } else {
                tags.insert(tag.to_owned(), value.to_owned());
            }
        } else if in_tags && !line.is_empty() {
            // Continuation of a tag — ignore for now
        }
    }

    tags
}
