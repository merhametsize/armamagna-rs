use std::fmt;
use std::hash::{BuildHasherDefault, Hash, Hasher};

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Custom FNV Hasher that implements the FNV-1a bulk-mix logic. Provides a HUUUUUGE speedup.
pub struct FnvHasher {
    hash: u64,
}

impl Default for FnvHasher {
    fn default() -> Self {
        FnvHasher {
            hash: FNV_OFFSET_BASIS,
        }
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    /// Replicates the C++ FNV logic: h = (h ^ v) * FNV_PRIME;
    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        // Use wrapping_mul for the same behavior as C++ arithmetic overflow
        self.hash = (self.hash ^ i).wrapping_mul(FNV_PRIME);
    }

    // We only need to implement the methods called by Signature::hash.
    #[inline(always)]
    fn write_u16(&mut self, i: u16) {
        self.write_u64(i as u64);
    }

    // Since Signature::hash only calls write_u64/write_u16, the generic write() is not strictly needed,
    // but the FNV implementation for bytes is also simple:
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash = (self.hash ^ (*byte as u64)).wrapping_mul(FNV_PRIME);
        }
    }
}

// Type alias for the BuildHasher needed by the HashMap
pub type FnvBuildHasher = BuildHasherDefault<FnvHasher>;

/// Represents the character signature of a word (a-z only, normalized).
/// Implemented as an array mapping letter index to letter count.
#[repr(C)]
#[repr(align(8))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Signature {
    table: [u8; 26],
}

impl Signature {
    /// Create a new Signature. The input word MUST be lowercase and normalized.
    pub fn new(word: &str) -> Self {
        let mut table = [0; 26];
        for c in word.bytes() {
            debug_assert!((b'a'..=b'z').contains(&(c as u8)), "Input must be a-z only");
            table[(c as u8 - b'a') as usize] += 1;
        }
        Self { table }
    }

    /// Creates an empty signature.
    pub fn new_empty() -> Self {
        let table = [0; 26];
        Self { table }
    }

    /// Add another Signature to this one.
    #[inline(always)]
    pub fn add(&mut self, other: &Signature) {
        let t = &mut self.table;
        for (i, &count) in other.table.iter().enumerate() {
            t[i] += count;
        }
    }

    /// Subtract another Signature from this one.
    #[inline(always)]
    pub fn sub(&mut self, other: &Signature) {
        let t = &mut self.table;
        for (i, &count) in other.table.iter().enumerate() {
            debug_assert!(t[i] >= count, "Subtraction would go negative");
            t[i] -= count;
        }
    }

    /// Returns true if self is a subset of other.
    #[inline(always)]
    pub fn is_subset_of(&self, other: &Signature) -> bool {
        for (a, b) in self.table.iter().zip(other.table.iter()) {
            if a > b {
                return false;
            }
        }
        true
    }

    /// Counts the characters in the signature.
    #[inline(always)]
    pub fn get_char_number(&self) -> usize {
        //self.table.iter().map(|&c| c as usize).sum()
        let mut s = 0usize;
        for i in 0..26 {
            s += self.table[i] as usize;
        }
        s
    }

    /// Returns a string representation.
    pub fn to_string(&self) -> String {
        let mut s = String::with_capacity(self.get_char_number());
        for (i, &count) in self.table.iter().enumerate() {
            if count > 0 {
                let c = (b'a' + i as u8) as char;
                s.extend(std::iter::repeat(c).take(count as usize));
            }
        }
        s
    }
}

impl Hash for Signature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let data = self.table.as_ptr();

        unsafe {
            // Write 3 chunks of 8 bytes (24 bytes total)
            state.write_u64(*(data as *const u64));
            state.write_u64(*(data.add(8) as *const u64));
            state.write_u64(*(data.add(16) as *const u64));

            // Write the remaining 2 bytes
            state.write_u16(*(data.add(24) as *const u16));
        }
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_signature() {
        let sig = Signature::new("cba");
        assert_eq!(sig.get_char_number(), 3);
        assert_eq!(sig.to_string(), "abc");
    }

    #[test]
    fn test_add_signature() {
        let mut sig1 = Signature::new("aab");
        let sig2 = Signature::new("bc");
        sig1.add(&sig2);
        // counts: a=2, b=2, c=1
        assert_eq!(sig1.to_string(), "aabbc");
    }

    #[test]
    fn test_sub_signature() {
        let mut sig1 = Signature::new("aabbc");
        let sig2 = Signature::new("abc");
        sig1.sub(&sig2);
        // counts: a=1, b=1, c=0
        assert_eq!(sig1.to_string(), "ab");
    }

    #[test]
    fn test_is_subset_of() {
        let sig1 = Signature::new("abc");
        let sig2 = Signature::new("aabbcc");
        assert!(sig1.is_subset_of(&sig2));
        assert!(!sig2.is_subset_of(&sig1));
    }

    #[test]
    #[should_panic(expected = "Subtraction would go negative")]
    fn test_sub_panics_on_negative() {
        let mut sig1 = Signature::new("a");
        let sig2 = Signature::new("aa");
        sig1.sub(&sig2); // should panic
    }
}
