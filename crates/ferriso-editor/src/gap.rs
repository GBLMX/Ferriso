//! A small, safe gap buffer.
//!
//! The editor borrowed the gap-buffer mutation model from `microsoft/edit`:
//! the movable hole keeps typing at the cursor O(1) amortized, and all edits
//! are expressed as replacements in byte coordinates. The implementation here
//! uses a `Vec<u8>` rather than virtual memory so it remains `Send`, portable,
//! and free of unsafe code.

use std::ops::Range;

/// A byte buffer with a movable gap.
#[derive(Clone, Debug)]
pub struct GapBuffer {
    data: Vec<u8>,
    gap: usize,
    gap_len: usize,
}

impl GapBuffer {
    /// Create an empty buffer with a small typing gap.
    pub fn new() -> Self {
        Self {
            data: vec![0; 64],
            gap: 0,
            gap_len: 64,
        }
    }

    /// Create a buffer from UTF-8 bytes, leaving a gap at the end.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut data = Vec::with_capacity(bytes.len() + 64);
        data.extend_from_slice(bytes);
        data.resize(bytes.len() + 64, 0);
        Self {
            data,
            gap: bytes.len(),
            gap_len: 64,
        }
    }

    /// Number of text bytes, excluding the gap.
    pub fn len(&self) -> usize {
        self.data.len() - self.gap_len
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterate the two textual chunks in order. The gap is skipped.
    pub fn chunks(&self) -> [&[u8]; 2] {
        let right_start = self.gap + self.gap_len;
        [&self.data[..self.gap], &self.data[right_start..]]
    }

    /// Read an arbitrary byte range. The caller must pass a valid range.
    pub fn read(&self, range: Range<usize>) -> Vec<u8> {
        let start = range.start.min(self.len());
        let end = range.end.min(self.len()).max(start);
        if start == end {
            return Vec::new();
        }

        let right_start = self.gap + self.gap_len;
        if end <= self.gap {
            self.data[start..end].to_vec()
        } else if start >= self.gap {
            self.data[right_start + start - self.gap..right_start + end - self.gap].to_vec()
        } else {
            let mut out = Vec::with_capacity(end - start);
            out.extend_from_slice(&self.data[start..self.gap]);
            out.extend_from_slice(&self.data[right_start..right_start + end - self.gap]);
            out
        }
    }

    /// Visit every UTF-8 character with its byte range, without joining the gap.
    pub fn visit_chars(&self, mut visit: impl FnMut(usize, char, usize)) {
        let [left, right] = self.chunks();
        let mut right_start = 0;
        let offset;
        match std::str::from_utf8(left) {
            Ok(text) => {
                for (start, ch) in text.char_indices() {
                    visit(start, ch, start + ch.len_utf8());
                }
                offset = left.len();
            }
            Err(error) => {
                let valid = error.valid_up_to();
                let text = std::str::from_utf8(&left[..valid]).expect("editor buffer is UTF-8");
                for (start, ch) in text.char_indices() {
                    visit(start, ch, start + ch.len_utf8());
                }
                let carry = &left[valid..];
                let width = utf8_width(carry[0]);
                let mut bytes = [0; 4];
                bytes[..carry.len()].copy_from_slice(carry);
                let take = (width - carry.len()).min(right.len());
                bytes[carry.len()..carry.len() + take].copy_from_slice(&right[..take]);
                let ch = std::str::from_utf8(&bytes[..width])
                    .expect("editor buffer is UTF-8")
                    .chars()
                    .next()
                    .expect("a complete character");
                visit(valid, ch, valid + width);
                right_start = take;
                offset = valid + width;
            }
        }
        let text = std::str::from_utf8(&right[right_start..]).expect("editor buffer is UTF-8");
        for (start, ch) in text.char_indices() {
            visit(offset + start, ch, offset + start + ch.len_utf8());
        }
    }

    /// Copy the whole buffer into a byte vector.
    pub fn to_vec(&self) -> Vec<u8> {
        let [left, right] = self.chunks();
        let mut out = Vec::with_capacity(left.len() + right.len());
        out.extend_from_slice(left);
        out.extend_from_slice(right);
        out
    }

    /// Replace `range` with `insert`.
    ///
    /// The caller is responsible for keeping byte offsets on UTF-8 boundaries.
    pub fn replace(&mut self, range: Range<usize>, insert: &[u8]) {
        let text_len = self.len();
        assert!(range.start <= range.end, "reversed replacement range");
        assert!(range.end <= text_len, "replacement range is out of bounds");

        self.move_gap(range.start);
        let removed = range.end - range.start;
        self.gap_len += removed;
        self.ensure_gap(insert.len());

        let gap_end = self.gap + insert.len();
        self.data[self.gap..gap_end].copy_from_slice(insert);
        self.gap = gap_end;
        self.gap_len -= insert.len();

        debug_assert_eq!(self.len(), text_len + insert.len() - removed);
    }

    /// Insert bytes at `offset`.
    #[cfg(test)]
    pub fn insert(&mut self, offset: usize, bytes: &[u8]) {
        self.replace(offset..offset, bytes);
    }

    /// Delete a byte range.
    #[cfg(test)]
    pub fn delete(&mut self, range: Range<usize>) {
        self.replace(range, &[]);
    }

    fn move_gap(&mut self, position: usize) {
        if position == self.gap {
            return;
        }
        if position < self.gap {
            self.data
                .copy_within(position..self.gap, position + self.gap_len);
            self.gap = position;
        } else {
            let len = position - self.gap;
            let right_start = self.gap + self.gap_len;
            self.data
                .copy_within(right_start..right_start + len, self.gap);
            self.gap = position;
        }
    }

    fn ensure_gap(&mut self, needed: usize) {
        if self.gap_len >= needed {
            return;
        }
        let extra = needed - self.gap_len;
        let old_len = self.data.len();
        self.data.resize(old_len + extra, 0);
        let right_start = self.gap + self.gap_len;
        self.data
            .copy_within(right_start..old_len, right_start + extra);
        self.gap_len = needed;
    }
}

fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

impl Default for GapBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(buffer: &GapBuffer) -> String {
        String::from_utf8(buffer.to_vec()).expect("valid UTF-8")
    }

    #[test]
    fn insertion_and_deletion_cross_the_gap() {
        let mut buffer = GapBuffer::from_bytes(b"hello world");
        buffer.insert(5, b",");
        assert_eq!(text(&buffer), "hello, world");
        buffer.delete(0..5);
        assert_eq!(text(&buffer), ", world");
        buffer.insert(0, b">");
        assert_eq!(text(&buffer), ">, world");
        buffer.delete(0..2);
        assert_eq!(text(&buffer), " world");
    }

    #[test]
    fn deleting_across_the_gap_keeps_the_right_bytes() {
        let mut buffer = GapBuffer::from_bytes(b"abcdef");
        buffer.delete(2..5);
        assert_eq!(text(&buffer), "abf");
        buffer.insert(2, b"XYZ");
        assert_eq!(text(&buffer), "abXYZf");
        assert_eq!(buffer.read(0..7), b"abXYZf");
    }

    #[test]
    fn chunks_skip_the_hole() {
        let mut buffer = GapBuffer::from_bytes(b"123456");
        buffer.replace(2..4, b"xy");
        let [left, right] = buffer.chunks();
        assert_eq!([left, right].concat(), b"12xy56");
    }

    #[test]
    fn visiting_chars_crosses_a_gap_inside_a_character() {
        let mut buffer = GapBuffer::from_bytes("aé中😀z".as_bytes());
        buffer.replace(2..2, b"");
        let mut visited = Vec::new();
        buffer.visit_chars(|start, ch, end| visited.push((start, ch, end)));
        assert_eq!(
            visited,
            vec![
                (0, 'a', 1),
                (1, 'é', 3),
                (3, '中', 6),
                (6, '😀', 10),
                (10, 'z', 11),
            ]
        );
    }

    #[test]
    fn randomized_replacements_match_a_plain_vec() {
        let mut buffer = GapBuffer::from_bytes(b"seed");
        let mut model = b"seed".to_vec();
        let mut state = 0x1234_5678_u64;

        for _ in 0..2_000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start = (state as usize) % (model.len() + 1);
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let end = start + ((state as usize) % (model.len() + 1 - start));
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let insert_len = (state as usize) % 5;
            let insert: Vec<u8> = (0..insert_len)
                .map(|index| b'a' + (((state >> (index * 3)) & 0xf) as u8 % 26))
                .collect();

            buffer.replace(start..end, &insert);
            model.splice(start..end, insert);
            assert_eq!(buffer.to_vec(), model, "range {start}..{end}");
        }
    }
}
