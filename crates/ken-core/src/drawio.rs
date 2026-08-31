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

pub fn extract_labels(raw: &str) -> String {
    labels_at(raw, 0)
}

fn labels_at(raw: &str, depth: u8) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    // With quick-xml's `encoding` feature enabled (another crate in the
    // workspace turns it on, and features unify), the convenience `unescape*`
    // methods are compiled out — the decoder has to be threaded explicitly.
    let decoder = reader.decoder();
    let mut in_diagram = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if e.name().local_name().as_ref() == b"diagram" {
                    // Only an open <diagram> can carry a compressed text
                    // payload as its child text node.
                    in_diagram = true;
                }
                collect_attr_labels(&e, decoder, &mut out);
            }
            Ok(Event::Empty(e)) => collect_attr_labels(&e, decoder, &mut out),
            Ok(Event::End(e)) if e.name().local_name().as_ref() == b"diagram" => {
                in_diagram = false;
            }
            // A compressed page is the diagram element's text payload.
            Ok(Event::Text(t)) if in_diagram && depth < MAX_PAYLOAD_DEPTH => {
                if let Ok(txt) = t.decode() {
                    if let Some(inner) = decode_payload(txt.trim()) {
                        // Recurse over the decoded mxGraphModel XML.
                        let nested = labels_at(&inner, depth + 1);
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
                push_clean(out, &v);
            }
        }
    }
}

/// base64 → raw deflate → percent-decode. None when any stage fails.
fn decode_payload(payload: &str) -> Option<String> {
    use base64::Engine;
    use std::io::Read;
    if payload.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload).ok()?;
    let mut xml = String::new();
    flate2::read::DeflateDecoder::new(bytes.as_slice())
        .read_to_string(&mut xml)
        .ok()?;
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
/// labels, then push the cleaned text if anything is left.
fn push_clean(out: &mut Vec<String>, value: &str) {
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
    if !text.is_empty() {
        out.push(text);
    }
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
    fn malformed_input_degrades_to_empty() {
        assert_eq!(extract_labels("not xml at all"), "");
        assert_eq!(extract_labels("<mxfile><diagram>!!!not-base64!!!</diagram></mxfile>"), "");
        assert_eq!(extract_labels(""), "");
    }
}
