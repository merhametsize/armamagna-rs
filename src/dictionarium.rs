use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::signature::{FnvBuildHasher, Signature};

use unicode_normalization::UnicodeNormalization;

pub const MAX_WORD_LENGTH: usize = 45;
pub type Section = HashMap<Signature, Vec<String>, FnvBuildHasher>;

/// Normalizes a string to ASCII non-accented  lower-case characters.
pub fn normalize_string(s: &str) -> String {
    let sn = s
        .nfd()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();

    return sn;
}

/// The dictionary object mapping signatures to their corresponding words. Divided in sections, one per word length,
/// for ease of access. Words that are not supersets of the target text are filtered out.
#[derive(Debug)]
pub struct Dictionarium {
    words_number: u64,
    reduced_words_number: u64,
    longest_word_length: usize,
    sections: Vec<Section>, // index = word length
}

/// Returns an empty dictionary.
impl Default for Dictionarium {
    fn default() -> Self {
        Self {
            words_number: 0,
            reduced_words_number: 0,
            longest_word_length: 0,
            sections: vec![HashMap::default(); MAX_WORD_LENGTH + 1],
        }
    }
}

impl Dictionarium {
    /// Constructor
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads a word list from a file and builds the sections.
    pub fn read_word_list(
        &mut self,
        wordlist_name: &str,
        target_text: &str,
    ) -> Result<u64, String> {
        //Opens the file
        let file =
            File::open(wordlist_name).map_err(|_| format!("Cannot open file {}", wordlist_name))?;
        let reader = BufReader::new(file);

        //Computes the target text signature
        let normalized_target_text = normalize_string(&target_text);
        let target_signature = Signature::new(&normalized_target_text);

        //Reads the wordlist line by line
        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            let normalized_word = normalize_string(&line);
            if normalized_word.is_empty() {
                continue; //Skip empty normalized words
            }

            //If it's longer than maxWordLength, error
            let word_length = normalized_word.len();
            if word_length > MAX_WORD_LENGTH {
                return Err(format!(
                    "A word in the dictionary is too long, maximum length: {}",
                    MAX_WORD_LENGTH
                ));
            }

            //Computes the word's signature
            let ws = Signature::new(&normalized_word);
            self.words_number += 1;

            //If the word is not a subset of the target, skips it
            if !ws.is_subset_of(&target_signature) {
                continue;
            }

            //Refreshes the length of the longest word
            self.reduced_words_number += 1;
            if word_length > self.longest_word_length {
                self.longest_word_length = word_length;
            }

            //Pushes the word in the right section, with the corresponding signature-key
            self.sections[word_length]
                .entry(ws)
                .or_insert_with(Vec::new)
                .push(line);
        }

        Ok(self.words_number)
    }

    /// Returns the number of words in the dictionary after filtering.
    pub fn get_reduced_words_number(&self) -> u64 {
        self.reduced_words_number
    }

    /// Returns a section of the dictionary (a hashmap mapping 1 signature --> multiple words)
    pub fn get_section(&self, section_number: usize) -> &Section {
        &self.sections[section_number]
    }

    /// Returns the words corresponding to a certain signature.
    pub fn get_words(&self, ws: &Signature) -> &Vec<String> {
        let characters_number = ws.get_char_number();
        self.sections[characters_number as usize].get(ws).unwrap() //Returns the set of words associated to ws
    }

    /// Returns the dictionary sections that still contain words after filtering.
    pub fn get_available_lengths(&self) -> Vec<usize> {
        self.sections
            .iter()
            .enumerate()
            .filter_map(|(i, s)| if !s.is_empty() { Some(i) } else { None })
            .collect()
    }
}

/// Implement Display for printing
impl fmt::Display for Dictionarium {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for section in &self.sections {
            for (ws, words) in section {
                write!(f, "{}", ws)?;
                for word in words {
                    write!(f, " {}", word)?;
                }
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_word_list_basic() {
        // Create a temporary file with mock dictionary
        let mut tmp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(tmp_file, "gabri").unwrap();
        writeln!(tmp_file, "glorietta").unwrap();
        writeln!(tmp_file, "qwertyuiop").unwrap();

        let target_text = "gabrielinoglorietta";

        let mut dict = Dictionarium::new();
        let result = dict
            .read_word_list(tmp_file.path().to_str().unwrap(), &target_text)
            .unwrap();

        // Check the number of words read
        assert_eq!(result, 3);
        assert!(dict.get_reduced_words_number() > 0);

        // Check that all words in sections are subset of the source
        for len in dict.get_available_lengths() {
            for (_sig, words) in dict.get_section(len) {
                for word in words {
                    let normalized_word = normalize_string(word);
                    let sig = Signature::new(&normalized_word);
                    let source_sig = Signature::new(&normalize_string(target_text));
                    assert!(sig.is_subset_of(&source_sig));
                }
            }
        }
    }

    #[test]
    fn test_get_words_and_sections() {
        let mut tmp_file = NamedTempFile::new().unwrap();
        writeln!(tmp_file, "gabri").unwrap();
        writeln!(tmp_file, "gabriele").unwrap();

        let mut dict = Dictionarium::new();
        let source_text = "gabrieleito";

        dict.read_word_list(tmp_file.path().to_str().unwrap(), source_text)
            .unwrap();

        let lengths = dict.get_available_lengths();
        assert!(lengths.contains(&5)); // "gabri"
        assert!(lengths.contains(&8)); // "gabriele"

        for len in lengths {
            let section = dict.get_section(len);
            for (_sig, words) in section {
                for word in words {
                    assert!(word.len() == len);
                }
            }
        }
    }
}
