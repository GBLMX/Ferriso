//! Segmented, lazily shifted sequence used by line and wrap indexes.
//!
//! Values are stored in fixed-size chunks. An edit inserts one chunk and
//! changes the delta on the suffix chunks, so untouched values are never moved
//! or rewritten. Chunks are coalesced only when their count grows beyond the
//! natural chunk count.

use std::fmt;

const CHUNK: usize = 256;
const MIN_SEGMENTS: usize = 256;

/// A value that can be shifted when bytes or lines before it change.
pub(crate) trait Shift: Copy {
    fn shifted(self, line_delta: isize, byte_delta: isize) -> Self;
}

impl Shift for usize {
    fn shifted(self, _line_delta: isize, byte_delta: isize) -> Self {
        (self as isize + byte_delta) as usize
    }
}

struct Segment<T> {
    start: usize,
    line_delta: isize,
    byte_delta: isize,
    values: Vec<T>,
}

pub(crate) struct Segmented<T> {
    segments: Vec<Segment<T>>,
    len: usize,
}

impl<T> Clone for Segmented<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            segments: self
                .segments
                .iter()
                .map(|segment| Segment {
                    start: segment.start,
                    line_delta: segment.line_delta,
                    byte_delta: segment.byte_delta,
                    values: segment.values.clone(),
                })
                .collect(),
            len: self.len,
        }
    }
}

impl<T> fmt::Debug for Segmented<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Segmented")
            .field("len", &self.len)
            .field("segments", &self.segments.len())
            .finish()
    }
}

impl<T: Shift> Segmented<T> {
    pub(crate) fn from_vec(values: Vec<T>) -> Self {
        let len = values.len();
        let mut segments = Vec::with_capacity(len.div_ceil(CHUNK));
        let mut start = 0;
        let mut iter = values.into_iter();
        loop {
            let chunk: Vec<T> = iter.by_ref().take(CHUNK).collect();
            if chunk.is_empty() {
                break;
            }
            let chunk_len = chunk.len();
            segments.push(Segment {
                start,
                line_delta: 0,
                byte_delta: 0,
                values: chunk,
            });
            start += chunk_len;
        }
        Self { segments, len }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn get(&self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }
        let segment_index = self
            .segments
            .partition_point(|segment| segment.start <= index)
            .saturating_sub(1);
        let segment = self.segments.get(segment_index)?;
        segment
            .values
            .get(index - segment.start)
            .map(|value| value.shifted(segment.line_delta, segment.byte_delta))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = T> + '_ {
        (0..self.len).filter_map(|index| self.get(index))
    }

    /// First index whose value matches the monotonic predicate. Returns `len`
    /// when no value matches.
    pub(crate) fn partition_point<F>(&self, predicate: F) -> usize
    where
        F: Fn(T) -> bool,
    {
        let mut low = 0;
        let mut high = self.len;
        while low < high {
            let middle = (low + high) / 2;
            if self.get(middle).is_some_and(&predicate) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    pub(crate) fn replace(
        &mut self,
        prefix_end: usize,
        suffix_start: usize,
        inserted: Vec<T>,
        line_delta: isize,
        byte_delta: isize,
    ) {
        debug_assert!(prefix_end <= suffix_start);
        debug_assert!(suffix_start <= self.len);
        self.split_at(prefix_end);
        self.split_at(suffix_start);

        let start_index = self
            .segments
            .partition_point(|segment| segment.start < prefix_end);
        let end_index = self
            .segments
            .partition_point(|segment| segment.start < suffix_start);
        self.segments.drain(start_index..end_index);

        let inserted_len = inserted.len();
        if inserted_len > 0 {
            self.segments.insert(
                start_index,
                Segment {
                    start: prefix_end,
                    line_delta: 0,
                    byte_delta: 0,
                    values: inserted,
                },
            );
        }

        let suffix_index = start_index + usize::from(inserted_len > 0);
        let removed = suffix_start - prefix_end;
        for segment in &mut self.segments[suffix_index..] {
            segment.start =
                (segment.start as isize - removed as isize + inserted_len as isize) as usize;
            segment.line_delta += line_delta;
            segment.byte_delta += byte_delta;
        }

        self.len = self.len - removed + inserted_len;
        let natural_segments = self.len.div_ceil(CHUNK).max(1);
        if self.segments.len() > (natural_segments * 2).max(MIN_SEGMENTS) {
            let values = self.iter().collect();
            *self = Self::from_vec(values);
        }
    }

    fn split_at(&mut self, index: usize) {
        if index == 0 || index >= self.len {
            return;
        }
        let segment_index = self
            .segments
            .partition_point(|segment| segment.start <= index)
            .saturating_sub(1);
        let segment = &mut self.segments[segment_index];
        let local = index - segment.start;
        if local == 0 || local >= segment.values.len() {
            return;
        }
        let right_values = segment.values.split_off(local);
        let right = Segment {
            start: index,
            line_delta: segment.line_delta,
            byte_delta: segment.byte_delta,
            values: right_values,
        };
        self.segments.insert(segment_index + 1, right);
    }

    #[cfg(test)]
    pub(crate) fn to_vec(&self) -> Vec<T> {
        self.iter().collect()
    }

    #[cfg(test)]
    pub(crate) fn segment_count(&self) -> usize {
        self.segments.len()
    }
}
