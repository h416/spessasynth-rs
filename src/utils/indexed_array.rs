/// IndexedByteArray
/// purpose: Uint8Array with a current_index cursor.
/// Ported from: src/utils/indexed_array.ts
use std::ops::{Deref, DerefMut, Index, IndexMut};

#[derive(Debug)]
pub struct IndexedByteArray {
    data: Vec<u8>,
    pub current_index: usize,
}

impl IndexedByteArray {
    /// Creates a zeroed array of the given length.
    /// Equivalent to: new IndexedByteArray(len)
    pub fn new(len: usize) -> Self {
        Self {
            data: vec![0u8; len],
            current_index: 0,
        }
    }

    /// Creates an IndexedByteArray from an existing Vec<u8>.
    pub fn from_vec(data: Vec<u8>) -> Self {
        Self {
            data,
            current_index: 0,
        }
    }

    /// Creates an IndexedByteArray by copying a byte slice.
    pub fn from_slice(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            current_index: 0,
        }
    }

    /// Returns a copy of the portion [start, end) with current_index reset to 0.
    /// Equivalent to: slice(start?, end?)
    pub fn slice(&self, start: usize, end: usize) -> Self {
        Self {
            data: self.data[start..end].to_vec(),
            current_index: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Index<usize> for IndexedByteArray {
    type Output = u8;
    fn index(&self, i: usize) -> &u8 {
        &self.data[i]
    }
}

impl IndexMut<usize> for IndexedByteArray {
    fn index_mut(&mut self, i: usize) -> &mut u8 {
        &mut self.data[i]
    }
}

/// Allows passing IndexedByteArray as &[u8].
/// Equivalent to: ArrayLike<number> usage in TypeScript.
impl Deref for IndexedByteArray {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.data
    }
}

impl DerefMut for IndexedByteArray {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_zeroed() {
        let arr = IndexedByteArray::new(4);
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0], 0);
        assert_eq!(arr.current_index, 0);
    }

    #[test]
    fn test_from_vec() {
        let arr = IndexedByteArray::from_vec(vec![1, 2, 3]);
        assert_eq!(arr[0], 1);
        assert_eq!(arr[2], 3);
        assert_eq!(arr.current_index, 0);
    }

    #[test]
    fn test_index_read_write() {
        let mut arr = IndexedByteArray::new(3);
        arr[0] = 0xAB;
        arr[1] = 0xCD;
        assert_eq!(arr[0], 0xAB);
        assert_eq!(arr[1], 0xCD);
    }

    #[test]
    fn test_current_index_advance() {
        let mut arr = IndexedByteArray::from_vec(vec![10, 20, 30]);
        let b = arr[arr.current_index];
        arr.current_index += 1;
        assert_eq!(b, 10);
        let b = arr[arr.current_index];
        arr.current_index += 1;
        assert_eq!(b, 20);
    }

    #[test]
    fn test_slice() {
        let arr = IndexedByteArray::from_vec(vec![1, 2, 3, 4, 5]);
        let sliced = arr.slice(1, 4);
        assert_eq!(sliced.len(), 3);
        assert_eq!(sliced[0], 2);
        assert_eq!(sliced[1], 3);
        assert_eq!(sliced[2], 4);
        assert_eq!(sliced.current_index, 0);
    }

    #[test]
    fn test_deref_as_slice() {
        let arr = IndexedByteArray::from_vec(vec![1, 2, 3]);
        let s: &[u8] = &arr;
        assert_eq!(s, &[1u8, 2, 3]);
    }
}
