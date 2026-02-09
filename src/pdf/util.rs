//! Shared helpers for PDF serialization.

pub(super) fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded == 0.0 {
        return "0".to_string();
    }
    let mut s = format!("{:.6}", rounded);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

pub(super) fn write_number(buffer: &mut Vec<u8>, value: f32) {
    let s = format_number(value as f64);
    buffer.extend_from_slice(s.as_bytes());
}

pub(super) fn write_numbers(buffer: &mut Vec<u8>, values: &[f32]) {
    for (idx, value) in values.iter().enumerate() {
        if idx > 0 {
            buffer.push(b' ');
        }
        write_number(buffer, *value);
    }
}
