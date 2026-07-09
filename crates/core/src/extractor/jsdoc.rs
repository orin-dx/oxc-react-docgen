//! JSDoc extraction: find_jsdoc, extract_jsdoc_tags, parse_jsdoc_text.

use std::collections::BTreeMap;

use super::SourceDataCollector;

impl<'src> SourceDataCollector<'src> {
    // ─── JSDoc extraction ─────────────────────────────────────────────────────

    /// Find JSDoc comment immediately preceding the given byte offset.
    /// Returns empty string if none found.
    ///
    /// Marks the comment as consumed so subsequent calls for a different span cannot
    /// return the same comment (prevents prop JSDoc leaking into component descriptions).
    pub(super) fn find_jsdoc(&mut self, span_start: u32) -> String {
        self.find_jsdoc_with_tags(span_start).0
    }

    /// Find the JSDoc comment immediately preceding the given byte offset and return
    /// both its description and its `@tag` map from a single lookup.
    ///
    /// Description and tags must come from the same consumed-tracking pass: an earlier
    /// version called a separate (non-consuming) tag scan after `find_jsdoc`, so a
    /// comment already claimed for one element's description was still visible to a
    /// later, unrelated element's tag scan — e.g. a `@deprecated` tag on one prop would
    /// bleed onto sibling props with no JSDoc of their own. Sharing the same consumed-set
    /// lookup for both makes that impossible.
    ///
    /// `self.comments` is sorted by span (source order), so the candidate window is
    /// located via binary search rather than a linear scan — callers run one of these
    /// per prop/interface/component, and a full rescan-per-call is quadratic in file size.
    pub(super) fn find_jsdoc_with_tags(&mut self, span_start: u32) -> (String, BTreeMap<String, String>) {
        const PROXIMITY_THRESHOLD: u32 = 120; // bytes — enough for blank lines + decorator

        let upper = self.comments.partition_point(|c| c.span_end <= span_start);
        let mut found: Option<usize> = None;
        for i in (0..upper).rev() {
            let c = &self.comments[i];
            if span_start - c.span_end > PROXIMITY_THRESHOLD {
                // Comments are sorted ascending by span, so every earlier comment
                // is farther still — nothing left in range.
                break;
            }
            if c.is_block && !self.consumed_jsdoc.contains(&c.span_end) {
                found = Some(i);
                break;
            }
        }

        let Some(i) = found else { return (String::new(), BTreeMap::new()) };
        let c = &self.comments[i];
        let span_end = c.span_end;
        let raw = &self.source[c.span_start as usize..c.span_end as usize];
        let text = parse_jsdoc_text(raw);
        let tags = extract_jsdoc_tags(raw);
        if !text.is_empty() || !tags.is_empty() {
            self.consumed_jsdoc.insert(span_end);
        }
        (text, tags)
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
