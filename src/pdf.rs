//! Minimal text PDF. Courier is a standard font, so this stays offline
//! and adds no crate. Court summaries are short; this is not a typesetter.

const PAGE_W: f32 = 612.0;
const PAGE_H: f32 = 792.0;
const MARGIN: f32 = 54.0;
const FONT_SIZE: f32 = 10.0;
const LEADING: f32 = 13.0;
const WRAP_AT: usize = 84;

pub fn render_pdf(text: &str) -> Vec<u8> {
    let lines = wrap_lines(text);
    let capacity = ((PAGE_H - MARGIN * 2.0) / LEADING) as usize;
    let capacity = capacity.max(1);
    let mut pages: Vec<Vec<String>> = Vec::new();
    for chunk in lines.chunks(capacity) {
        pages.push(chunk.to_vec());
    }
    if pages.is_empty() {
        pages.push(Vec::new());
    }

    let mut body = Vec::new();
    body.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = vec![0];

    let page_count = pages.len();
    let font_id = 3;
    let first_page_id = 4;

    write_obj(
        &mut body,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    );

    let mut kids = String::from("<< /Type /Pages /Kids [");
    for i in 0..page_count {
        let id = first_page_id + i * 2;
        kids.push_str(&format!(" {id} 0 R"));
    }
    kids.push_str(&format!(" ] /Count {page_count} >>"));
    write_obj(&mut body, &mut offsets, 2, kids.as_bytes());
    write_obj(
        &mut body,
        &mut offsets,
        font_id,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
    );

    for (i, page_lines) in pages.iter().enumerate() {
        let page_id = first_page_id + i * 2;
        let content_id = page_id + 1;
        let page = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Contents {content_id} 0 R /Resources << /Font << /F1 {font_id} 0 R >> >> >>"
        );
        write_obj(&mut body, &mut offsets, page_id, page.as_bytes());

        let stream = page_stream(page_lines);
        let content = format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        );
        write_obj(&mut body, &mut offsets, content_id, content.as_bytes());
    }

    let xref_at = body.len();
    let size = offsets.len();
    body.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    body.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        body.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    body.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n")
            .as_bytes(),
    );
    body
}

fn write_obj(body: &mut Vec<u8>, offsets: &mut Vec<usize>, id: usize, inner: &[u8]) {
    offsets.push(body.len());
    body.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    body.extend_from_slice(inner);
    body.extend_from_slice(b"\nendobj\n");
}

fn page_stream(lines: &[String]) -> String {
    let mut out = format!(
        "BT\n/F1 {FONT_SIZE} Tf\n{MARGIN} {} Td\n{LEADING} TL\n",
        PAGE_H - MARGIN
    );
    for line in lines {
        out.push('(');
        out.push_str(&pdf_escape(line));
        out.push_str(") Tj T*\n");
    }
    out.push_str("ET");
    out
}

fn wrap_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        let ascii = to_pdf_text(raw);
        if ascii.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut rest = ascii.as_str();
        while rest.len() > WRAP_AT {
            let mut cut = WRAP_AT;
            if let Some(space) = rest[..WRAP_AT].rfind(' ') {
                if space > 20 {
                    cut = space;
                }
            }
            lines.push(rest[..cut].trim_end().to_string());
            rest = rest[cut..].trim_start();
        }
        lines.push(rest.to_string());
    }
    lines
}

fn to_pdf_text(raw: &str) -> String {
    raw.chars()
        .map(|ch| match ch {
            '\u{2014}' | '\u{2013}' => '-',
            c if c.is_ascii() && !c.is_control() => c,
            _ => '?',
        })
        .collect()
}

fn pdf_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\\' | '(' | ')' => {
                out.push('\\');
                out.push(ch);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_contains_case_text_and_a_real_xref() {
        let text = "DOCKET — CR-2026-0142\nDefendant: Client J.D.\nCharges: (possession)\n";
        let pdf = render_pdf(text);
        let raw = String::from_utf8_lossy(&pdf);
        assert!(raw.starts_with("%PDF-1.4\n"));
        assert!(raw.contains("CR-2026-0142"));
        assert!(raw.contains("Client J.D."));
        assert!(raw.contains("\\(possession\\)"));
        assert!(raw.contains("%%EOF"));
        let start = raw.rfind("startxref\n").unwrap();
        let xref_at: usize = raw[start + "startxref\n".len()..]
            .lines()
            .next()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(&pdf[xref_at..xref_at + 4], b"xref");
    }
}
