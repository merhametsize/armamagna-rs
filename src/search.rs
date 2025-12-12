use std::sync::Arc;

use ahash::HashSetExt;
use crossbeam_channel::Sender;
use fxhash::FxHashSet;

use crate::dictionarium::Dictionarium;
use crate::signature::Signature;

/// Temporary mutable state passed during the recursive search.
/// This needs to be decoupled from SearchThread, otherwise recursion is not possible because the borrow checker
/// prevents &self and &mut self from existing at the same time.
struct SearchState {
    ws: Signature,
    solution: Vec<Signature>,
    anagram_set: FxHashSet<String>,
}

/// Not a real thread, more like a searcher object with a state and options.
pub struct SearchThread {
    // Immutable Context / Shared Resources (All Arcs and final config)
    dictionarium: Arc<Dictionarium>,
    target_signature: Arc<Signature>,
    included_text: Arc<String>,
    word_lengths: Vec<usize>,
    words_number: usize,
    sender: Sender<String>,
}

impl SearchThread {
    pub fn new(
        dictionarium: Arc<Dictionarium>,
        target_signature: Arc<Signature>,
        included_text: Arc<String>,
        word_lengths: Vec<usize>,
        sender: Sender<String>,
    ) -> Self {
        let words_number = word_lengths.len();

        Self {
            dictionarium,
            target_signature,
            included_text,
            word_lengths,
            words_number,
            sender,
        }
    }

    /// Launches the search.
    pub fn run(&mut self) {
        let mut state = SearchState {
            ws: Signature::new_empty(),
            solution: vec![Signature::new_empty(); self.words_number],
            anagram_set: FxHashSet::new(),
        };

        self.search(0, &mut state);
    }

    /// Recursive search function.
    fn search(&self, word_index: usize, state: &mut SearchState) {
        // Base case
        debug_assert!(word_index <= self.words_number);
        if word_index == self.words_number {
            if state.ws == *self.target_signature {
                //If an anagram is found
                self.compute_solution(state);
            }
            return;
        }

        let len = self.word_lengths[word_index];
        let section = self.dictionarium.get_section(len);

        for (current_signature, _) in section {
            state.ws.add(current_signature);

            // Pruning block
            if word_index >= 1 && !state.ws.is_subset_of(&self.target_signature) {
                state.ws.sub(current_signature);
                continue;
            }

            state.solution[word_index] = *current_signature;

            // Recursive call is safe: &self (immutable) and &mut state (mutable, external)
            self.search(word_index + 1, state);

            // Backtracking
            state.ws.sub(current_signature);
        }
    }

    /// Root for the recursive composition function that builds text anagrams from series of signatures.
    /// As every signature in the solution may correspond to multiple words, every solution may generate several anagrams.
    fn compute_solution(&self, state: &mut SearchState) {
        let mut anagram: Vec<String> = Vec::new();

        if !self.included_text.is_empty() {
            anagram.push(self.included_text.as_str().to_string());
        }

        self.output_solution(&mut anagram, 0, state);
    }

    /// Recursive function that generates text anagrams from a collection of signatures.
    fn output_solution(&self, anagram: &mut Vec<String>, index: usize, state: &mut SearchState) {
        debug_assert!(index <= self.words_number);

        // Base case
        if index == self.words_number {
            let mut ordered = anagram.clone();
            ordered.sort_unstable();

            let canonical = ordered.join(" ");
            debug_assert!(!canonical.is_empty());

            if state.anagram_set.insert(canonical.clone()) {
                let _ = self.sender.send(canonical);
            }

            return;
        }

        let sig = &state.solution[index];
        let words = self.dictionarium.get_words(sig);

        for w in words {
            anagram.push(w.clone());

            self.output_solution(anagram, index + 1, state);

            anagram.pop(); // Backtracking
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionarium::Dictionarium;
    use crate::signature::Signature;
    use crossbeam_channel::unbounded;
    use std::collections::HashSet;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper to create a Dictionarium instance from a list of words via a temporary file.
    fn create_mock_dictionarium(words: Vec<&'static str>, target_text: &str) -> Arc<Dictionarium> {
        let mut tmp_file = NamedTempFile::new().expect("Failed to create temp file");
        for word in words {
            writeln!(tmp_file, "{}", word).unwrap();
        }

        let mut dict = Dictionarium::new();
        dict.read_word_list(tmp_file.path().to_str().unwrap(), target_text)
            .unwrap();

        Arc::new(dict)
    }

    #[test]
    fn test_search_thread_basic_anagram() {
        let target_sig = Signature::new("act");
        let dict_words = vec!["cat", "act", "tac", "dog"];
        let dict_arc = create_mock_dictionarium(dict_words, "act");

        let word_lengths = vec![3];
        let (sender, receiver) = unbounded();

        let mut search_thread = SearchThread::new(
            dict_arc,
            Arc::new(target_sig),
            Arc::new("".to_string()),
            word_lengths,
            sender,
        );

        search_thread.run();

        let anagrams_found: HashSet<String> = receiver.try_iter().collect();
        let expected_anagrams: HashSet<String> = vec!["act", "cat", "tac"]
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        assert_eq!(
            anagrams_found, expected_anagrams,
            "Should find all single-word anagrams"
        );
    }

    #[test]
    fn test_search_thread_multi_word_anagram() {
        // Target: "barman" (a=2, b=1, m=1, n=1, r=1)
        let target_sig = Signature::new("barman");
        let dict_words = vec!["bar", "bra", "man", "nam", "ran"];
        let dict_arc = create_mock_dictionarium(dict_words, "barman");

        let word_lengths = vec![3, 3];
        let (sender, receiver) = unbounded();

        let mut search_thread = SearchThread::new(
            dict_arc,
            Arc::new(target_sig),
            Arc::new("".to_string()),
            word_lengths,
            sender,
        );

        search_thread.run();

        let anagrams_found: HashSet<String> = receiver.try_iter().collect();

        // Valid combinations that form "barman" are (bar/bra) + (man/nam).
        // The output is sorted alphabetically, joined by a space.
        let expected_anagrams: HashSet<String> = vec![
            "bar man".to_string(), // bar + man
            "bar nam".to_string(), // bar + nam
            "bra man".to_string(), // bra + man
            "bra nam".to_string(), // bra + nam
        ]
        .into_iter()
        .collect();

        assert_eq!(
            anagrams_found, expected_anagrams,
            "Should find all multi-word anagrams"
        );
    }
}
