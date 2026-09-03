//! Offset conversion for front ends that address strings in UTF-16 (AppKit,
//! Windows). Engines match on UTF-8 byte offsets; this maps them.

/// Converts UTF-8 byte ranges within `s` to UTF-16 code-unit ranges. ASCII
/// lines (the common case) map 1:1 without any work.
pub fn to_utf16_ranges(
    s: &str,
    byte_ranges: impl Iterator<Item = (usize, usize)>,
) -> Vec<(u32, u32)> {
    if s.is_ascii() {
        return byte_ranges.map(|(a, b)| (a as u32, b as u32)).collect();
    }
    // Prefix table: UTF-16 offset for every byte offset (char boundaries only
    // are ever queried; interior bytes copy the preceding boundary's value).
    let mut table = Vec::with_capacity(s.len() + 1);
    let mut u16 = 0u32;
    for ch in s.chars() {
        for _ in 0..ch.len_utf8() {
            table.push(u16);
        }
        u16 += ch.len_utf16() as u32;
    }
    table.push(u16);
    byte_ranges.map(|(a, b)| (table[a], table[b])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_identity() {
        assert_eq!(to_utf16_ranges("hello", [(1, 3)].into_iter()), [(1, 3)]);
    }

    #[test]
    fn multibyte_shifts_offsets() {
        let s = "aé😀b"; // a=1 byte, é=2 bytes/1 unit, 😀=4 bytes/2 units, b
        assert_eq!(
            to_utf16_ranges(s, [(0, 1), (1, 3), (3, 7), (7, 8)].into_iter()),
            [(0, 1), (1, 2), (2, 4), (4, 5)]
        );
    }
}
