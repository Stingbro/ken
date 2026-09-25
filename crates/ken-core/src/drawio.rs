//! `.drawio` (diagrams.net) label extraction.
//!
//! An mxfile wraps one `<diagram>` per page. A page's content is either
//! inline `<mxGraphModel>` XML or a compressed payload:
//! base64( raw-deflate( percent-encode( xml ) ) ). The searchable text of a
//! diagram is its page names and the `value` attributes of its cells (which
//! may themselves contain HTML markup to strip). Malformed input yields an
//! empty string — a diagram we can't decode is metadata-only, never an error
//! the scanner would retry.

use quick_xml::events::Event;
use quick_xml::Reader;

/// A real page's payload decodes to an mxGraphModel — one level of nesting.
/// A crafted file could nest payload-inside-payload without bound, so the
/// recursion below is capped instead of left to blow the stack.
const MAX_PAYLOAD_DEPTH: u8 = 4;

/// Ceiling on the label text one file may yield in total. The per-payload cap
/// below bounds each page, but a file can hold hundreds of pages — a crafted
/// one whose payloads are almost entirely `value="…"` attributes would multiply
/// that cap by its page count and exhaust memory. Whatever fits under the
/// budget is kept; the rest is dropped, like every other degradation here.
const MAX_TOTAL_LABEL_BYTES: usize = 32 * 1024 * 1024;

pub fn extract_labels(raw: &str) -> String {
    let mut budget = MAX_TOTAL_LABEL_BYTES;
    labels_at(raw, 0, &mut budget)
}

/// `budget` is the label text still allowed across the whole file, shared by
/// every page and every nested payload.
fn labels_at(raw: &str, depth: u8, budget: &mut usize) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    // With quick-xml's `encoding` feature enabled (another crate in the
    // workspace turns it on, and features unify), the convenience `unescape*`
    // methods are compiled out — the decoder has to be threaded explicitly.
    let decoder = reader.decoder();
    let mut in_diagram = false;
    loop {
        if *budget == 0 {
            break; // budget spent: keep what we have, stop reading
        }
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if e.name().local_name().as_ref() == b"diagram" {
                    // Only an open <diagram> can carry a compressed text
                    // payload as its child text node.
                    in_diagram = true;
                }
                collect_attr_labels(&e, decoder, &mut out, budget);
            }
            Ok(Event::Empty(e)) => collect_attr_labels(&e, decoder, &mut out, budget),
            Ok(Event::End(e)) if e.name().local_name().as_ref() == b"diagram" => {
                in_diagram = false;
            }
            // A compressed page is the diagram element's text payload.
            Ok(Event::Text(t)) if in_diagram && depth < MAX_PAYLOAD_DEPTH => {
                if let Ok(txt) = t.decode() {
                    if let Some(inner) = decode_payload(txt.trim()) {
                        // Recurse over the decoded mxGraphModel XML.
                        // The nested text is already charged to the shared
                        // budget as it is gathered.
                        let nested = labels_at(&inner, depth + 1, budget);
                        if !nested.is_empty() {
                            out.push(nested);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break, // malformed: keep whatever was gathered
            _ => {}
        }
    }
    out.join("\n")
}

/// Page names (`name` on `<diagram>`) and cell labels (`value`/`label`
/// attributes) — the only attributes that carry human text.
fn collect_attr_labels(
    e: &quick_xml::events::BytesStart,
    decoder: quick_xml::encoding::Decoder,
    out: &mut Vec<String>,
    budget: &mut usize,
) {
    let name = e.name();
    let is_diagram = name.local_name().as_ref() == b"diagram";
    for attr in e.attributes().flatten() {
        let key = attr.key.local_name();
        if (is_diagram && key.as_ref() == b"name")
            || key.as_ref() == b"value"
            || key.as_ref() == b"label"
        {
            if let Ok(v) = attr.decode_and_unescape_value(decoder) {
                push_clean(out, &v, budget);
            }
        }
    }
}

/// Ceiling on a single page's *decompressed* XML. `MAX_EXTRACT_BYTES` gates the
/// file on disk, which deflate's ~1000:1 ratio makes meaningless here: a few
/// hundred kilobytes of `.drawio` can expand to gigabytes and take the scanner
/// down with it. Real pages are kilobytes; 32 MiB is orders of magnitude more
/// XML than any hand-drawn diagram carries, and a payload that exceeds it is
/// treated like any other undecodable one.
const MAX_PAYLOAD_BYTES: u64 = 32 * 1024 * 1024;

/// base64 → raw deflate → percent-decode. None when any stage fails, including
/// a payload that inflates past `MAX_PAYLOAD_BYTES`.
fn decode_payload(payload: &str) -> Option<String> {
    use base64::Engine;
    use std::io::Read;
    if payload.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload).ok()?;
    // Read one byte past the cap so hitting it is distinguishable from a
    // payload that merely ends there.
    let mut inflated = Vec::new();
    flate2::read::DeflateDecoder::new(bytes.as_slice())
        .take(MAX_PAYLOAD_BYTES + 1)
        .read_to_end(&mut inflated)
        .ok()?;
    if inflated.len() as u64 > MAX_PAYLOAD_BYTES {
        return None;
    }
    let xml = String::from_utf8_lossy(&inflated);
    Some(percent_decode(&xml))
}

/// Minimal %XX decoder (draw.io percent-encodes the XML before deflating).
/// Invalid escapes pass through untouched; '+' is NOT a space in this scheme.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Strip HTML tags and decode the handful of entities draw.io emits in rich
/// labels, then push the cleaned text if anything is left and the shared
/// `budget` still covers it. A label that would overrun the budget is dropped
/// and the budget zeroed, which stops the walk.
fn push_clean(out: &mut Vec<String>, value: &str, budget: &mut usize) {
    let mut text = String::with_capacity(value.len());
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return;
    }
    if text.len() > *budget {
        *budget = 0;
        return;
    }
    *budget -= text.len();
    out.push(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNCOMPRESSED: &str = r#"<mxfile><diagram name="Page-1">
        <mxGraphModel><root>
          <mxCell id="0"/><mxCell id="1" parent="0"/>
          <mxCell id="2" value="API Gateway" vertex="1"/>
          <mxCell id="3" value="&lt;b&gt;Auth&amp;nbsp;Service&lt;/b&gt;" vertex="1"/>
          <mxCell id="4" value="" edge="1"/>
        </root></mxGraphModel></diagram></mxfile>"#;

    #[test]
    fn extracts_page_names_and_cell_labels() {
        let text = extract_labels(UNCOMPRESSED);
        assert!(text.contains("Page-1"));
        assert!(text.contains("API Gateway"));
        // HTML in labels is stripped to its words, entities decoded.
        assert!(text.contains("Auth Service"), "got: {text:?}");
        assert!(!text.contains("<b>"));
    }

    #[test]
    fn extracts_compressed_diagram_payloads() {
        // Build a compressed page the way draw.io does:
        // percent-encode → raw deflate → base64.
        use base64::Engine;
        use flate2::write::DeflateEncoder;
        use flate2::Compression;
        use std::io::Write;
        let inner = r#"<mxGraphModel><root>
            <mxCell id="2" value="Billing%20Cutover" vertex="1"/>
        </root></mxGraphModel>"#;
        // draw.io percent-encodes BEFORE deflating; our fixture's inner text
        // already carries the %20 that decoding must turn into a space.
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(inner.as_bytes()).unwrap();
        let payload = base64::engine::general_purpose::STANDARD.encode(enc.finish().unwrap());
        let raw = format!(r#"<mxfile><diagram name="Flow">{payload}</diagram></mxfile>"#);
        let text = extract_labels(&raw);
        assert!(text.contains("Flow"));
        assert!(text.contains("Billing Cutover"), "got: {text:?}");
    }

    /// base64( deflate( body ) ), the way draw.io stores a page.
    fn pack(body: &str) -> String {
        use base64::Engine;
        use flate2::write::DeflateEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(body.as_bytes()).unwrap();
        base64::engine::general_purpose::STANDARD.encode(enc.finish().unwrap())
    }

    #[test]
    fn payload_nesting_is_capped() {
        // A hostile file can nest payload-in-payload; extraction must stay
        // shallow rather than recurse until the stack gives out.
        let mut xml = r#"<mxfile><diagram name="Deepest"/></mxfile>"#.to_string();
        for level in 0..8 {
            xml = format!(
                r#"<mxfile><diagram name="L{level}">{}</diagram></mxfile>"#,
                pack(&xml)
            );
        }
        let text = extract_labels(&xml);
        assert!(text.contains("L7"), "got: {text:?}");
        assert!(!text.contains("Deepest"), "got: {text:?}");
    }

    #[test]
    fn oversized_payload_is_refused_not_inflated() {
        // A deflate bomb: highly redundant XML that compresses to a handful of
        // kilobytes but inflates past the cap. It must degrade like any other
        // undecodable payload — the page name survives, its content doesn't.
        let mut body = String::from(r#"<mxGraphModel><root>"#);
        let cell = r#"<mxCell id="2" value="Bomb" vertex="1"/>"#;
        while body.len() as u64 <= MAX_PAYLOAD_BYTES {
            body.push_str(cell);
        }
        body.push_str("</root></mxGraphModel>");
        let packed = pack(&body);
        assert!(packed.len() < 1024 * 1024, "fixture should be small: {}", packed.len());
        let raw = format!(r#"<mxfile><diagram name="Boom">{packed}</diagram></mxfile>"#);
        let text = extract_labels(&raw);
        assert_eq!(text, "Boom", "got: {text:?}");
    }

    #[test]
    fn total_label_output_is_budgeted() {
        // The per-payload cap bounds one page; a file with many bomb pages
        // would still multiply it. Ten 4 MiB pages exceed the total budget:
        // extraction keeps roughly a budget's worth and stops.
        const PAGE_LABEL_BYTES: usize = 4 * 1024 * 1024;
        let value = "a".repeat(PAGE_LABEL_BYTES / 4);
        let cells: String = std::iter::repeat_n(&value, 4)
            .map(|v| format!(r#"<mxCell id="2" value="{v}" vertex="1"/>"#))
            .collect();
        let packed = pack(&format!("<mxGraphModel><root>{cells}</root></mxGraphModel>"));
        let pages: String = (0..MAX_TOTAL_LABEL_BYTES / PAGE_LABEL_BYTES + 2)
            .map(|i| format!(r#"<diagram name="P{i}">{packed}</diagram>"#))
            .collect();
        let text = extract_labels(&format!("<mxfile>{pages}</mxfile>"));
        // The join's newlines are the only slack over the budget.
        assert!(
            text.len() <= MAX_TOTAL_LABEL_BYTES + 1024,
            "unbounded: {} bytes",
            text.len()
        );
        // What fit under the budget is still indexed.
        assert!(text.len() > MAX_TOTAL_LABEL_BYTES / 2, "too little: {} bytes", text.len());
    }

    #[test]
    fn malformed_input_degrades_to_empty() {
        assert_eq!(extract_labels("not xml at all"), "");
        assert_eq!(extract_labels("<mxfile><diagram>!!!not-base64!!!</diagram></mxfile>"), "");
        assert_eq!(extract_labels(""), "");
    }
}
