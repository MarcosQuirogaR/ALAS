// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn write_procurement_line(xml: &mut String, line: &ProcurementLine) {
    xml.push_str("          <item quantity=\"");
    xml.push_str(&line.quantity.to_string());
    xml.push_str("\">\n");
    write_text(xml, 12, "componentId", &line.component_id);
    if let Some(quote) = &line.quote {
        xml.push_str("            <quote currency=\"");
        xml.push_str(quote.currency.code());
        xml.push_str("\">\n");
        write_integer(xml, 14, "unitPriceMinor", quote.unit_price_minor);
        write_text(xml, 14, "sourceUrl", &quote.source_url);
        write_text(xml, 14, "observedOnUtc", &quote.observed_on_utc);
        xml.push_str("            </quote>\n");
    }
    xml.push_str("          </item>\n");
}

fn write_number(xml: &mut String, indentation: usize, name: &str, value: f64) {
    write_text(xml, indentation, name, &value.to_string());
}

fn write_integer(xml: &mut String, indentation: usize, name: &str, value: u64) {
    write_text(xml, indentation, name, &value.to_string());
}

fn write_text(xml: &mut String, indentation: usize, name: &str, value: &str) {
    for _ in 0..indentation {
        xml.push(' ');
    }
    xml.push('<');
    xml.push_str(name);
    xml.push('>');
    escape_xml_into(xml, value);
    xml.push_str("</");
    xml.push_str(name);
    xml.push_str(">\n");
}

fn escape_xml_into(xml: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            '\"' => xml.push_str("&quot;"),
            '\'' => xml.push_str("&apos;"),
            _ => xml.push(character),
        }
    }
}

