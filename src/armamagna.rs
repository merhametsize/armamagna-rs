use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use rayon::ThreadPoolBuilder;

use crate::combinations::RepeatedCombinationsWithSum;
use crate::dictionarium::{Dictionarium, normalize_string};
use crate::search;
use crate::signature::Signature;

/// The Rust version of ArmaMagna, quite faithful to the original C++ version
pub struct ArmaMagna {
    // Constructor arguments
    target_text: String,
    included_text: String,
    dictionary_name: String,
    output_file_name: String,
    min_cardinality: u64,
    max_cardinality: u64,
    min_wordlength: u64,
    max_wordlength: u64,

    // Processed variables
    dictionary: Dictionarium, // Shared but read-only for threads (will be Arc-wrapped when needed)
    target_signature: Signature,
    included_text_signature: Signature,
    actual_target_signature: Signature, // actual = target - included
    included_words_number: u64,
    actual_min_cardinality: u64,
    actual_max_cardinality: u64,
    num_threads: u64,

    explored_sets: Arc<AtomicU64>, //⚛️Progress index, keeps track of how many search threads finished
}

impl ArmaMagna {
    // Constructor
    pub fn new() -> Self {
        Self {
            target_text: String::new(),
            included_text: String::new(),
            dictionary_name: String::new(),
            output_file_name: String::new(),
            min_cardinality: 1,
            max_cardinality: 3,
            min_wordlength: 1,
            max_wordlength: 30,

            dictionary: Dictionarium::new(),
            target_signature: Signature::new_empty(),
            included_text_signature: Signature::new_empty(),
            actual_target_signature: Signature::new_empty(),
            included_words_number: 0,
            actual_min_cardinality: 0,
            actual_max_cardinality: 0,
            num_threads: num_cpus::get() as u64,

            explored_sets: Arc::new(AtomicU64::new(0)), //⚛️
        }
    }

    /// Sets the search options.
    pub fn set_options(
        &mut self,
        text: &str,
        dictionary: &str,
        output_file_name: &str,
        included: &str,
        mincard: u64,
        maxcard: u64,
        minwlen: u64,
        maxwlen: u64,
        num_threads: u64,
    ) -> Result<(), String> {
        self.set_target_text(text)?;
        self.set_included_text(included)?;
        self.set_restrictions(mincard, maxcard, minwlen, maxwlen)?;
        self.set_dictionary_name(dictionary);
        self.set_threads_number(num_threads);
        self.output_file_name = output_file_name.to_string();
        Ok(())
    }

    /// Sets the text to anagram.
    pub fn set_target_text(&mut self, text: &str) -> Result<(), String> {
        self.target_text = text.to_string();
        let processed_source_text = normalize_string(&self.target_text); // Processes the target text and computes its signature
        self.target_signature = Signature::new(&processed_source_text);
        Ok(())
    }

    /// Sets the file to read words from.
    pub fn set_dictionary_name(&mut self, dictionary: &str) {
        self.dictionary_name = dictionary.to_string();
    }

    /// Sets the text to be included in the anagrams to search.
    /// The search space is drastically reduced this way.
    pub fn set_included_text(&mut self, included: &str) -> Result<(), String> {
        self.included_text = included.to_string();

        // Processes the included text
        let processed_included_text = normalize_string(&self.included_text);
        self.included_text_signature = Signature::new(&processed_included_text);

        // Computes the number of included words
        if self.included_text.is_empty() {
            self.included_words_number = 0;
        } else {
            self.included_words_number = count_words(&self.included_text) as u64;
        }

        // Invalid argument checking
        if !self
            .included_text_signature
            .is_subset_of(&self.target_signature)
        {
            return Err("The included text must be a subset of the target text".to_string());
        }
        if self.target_signature == self.included_text_signature {
            return Err("The included is an anagram of the target text".to_string());
        }

        // actual = target - included
        self.actual_target_signature = self.target_signature.clone();
        self.actual_target_signature
            .sub(&self.included_text_signature);

        // Computes the actual cardinalities
        self.actual_min_cardinality = self.min_cardinality - self.included_words_number;
        self.actual_max_cardinality = self.max_cardinality - self.included_words_number;

        Ok(())
    }

    /// Sets the cardinality restrictions (number of words in the anagrams).
    pub fn set_restrictions(
        &mut self,
        mincard: u64,
        maxcard: u64,
        minwlen: u64,
        maxwlen: u64,
    ) -> Result<(), String> {
        // Arguments validity checking
        if mincard > maxcard || minwlen > maxwlen {
            return Err(
                "Maximum cardinality/word length must be greater or equal than minimum cardinality/word length".to_string(),
            );
        }
        if mincard <= self.included_words_number {
            return Err(
                "Minimum cardinality must be >= than the number of included words".to_string(),
            );
        }
        if maxcard <= self.included_words_number {
            return Err(
                "Maximum cardinality must be >= than the number of included words".to_string(),
            );
        }

        self.min_cardinality = mincard;
        self.max_cardinality = maxcard;
        self.min_wordlength = minwlen;
        self.max_wordlength = maxwlen;

        // Computes the actual cardinalities
        self.actual_min_cardinality = mincard - self.included_words_number;
        self.actual_max_cardinality = maxcard - self.included_words_number;

        Ok(())
    }

    /// Sets the desired number of search threads.
    pub fn set_threads_number(&mut self, n: u64) {
        self.num_threads = n;
    }

    /// Main function equivalent to C++ `anagram()`.
    /// Returns the number of anagrams found on success.
    pub fn anagram(&mut self) -> Result<u64, String> {
        // Output settings
        self.print();

        // Reads the dictionary
        let words_read = self
            .dictionary
            .read_word_list(&self.dictionary_name, &self.target_text)?;
        println!(
            "[*] Read {} words, after filter {}\n",
            words_read,
            self.dictionary.get_reduced_words_number()
        );

        // Computes the power set from the word lengths that are available in the dictionary after filtering
        let available_lengths = self
            .dictionary
            .get_available_lengths(self.min_wordlength as usize, self.max_wordlength as usize);

        let rcs = RepeatedCombinationsWithSum::new(
            self.actual_target_signature.get_char_number(),
            self.actual_min_cardinality as usize,
            self.actual_max_cardinality as usize,
            available_lengths,
        );
        let combinations_number = rcs.get_sets_number();

        // Reserve two threads: main + IO
        let workers_number = (self.num_threads - 2).max(1);
        println!("[*] Starting {} search threads", workers_number);
        println!("[*] Covering {} length combinations\n", combinations_number);

        // Prepare the Arcs to share with workers
        let dict_arc = Arc::new(std::mem::take(&mut self.dictionary)); //Moved
        let actual_target_signature_arc = Arc::new(self.actual_target_signature.clone());
        let included_text_arc = Arc::new(self.included_text.clone());

        // Build a rayon thread pool with the desired number of worker threads
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers_number as usize)
            .build()
            .map_err(|e| format!("Failed to build thread pool: {}", e))?;

        // Create the crossbeam channel (unbounded). Producers will be clones of sender
        let (sender, receiver): (Sender<String>, Receiver<String>) = unbounded();

        // Spawn the IO thread which consumes from the receiver and writes to the output file
        let of = self.output_file_name.clone();
        let progress_clone = self.explored_sets.clone();
        let io_handle =
            thread::spawn(move || Self::io_loop(receiver, of, progress_clone, combinations_number));

        let timer_start = Instant::now();

        // Scope the work so we block until all tasks are done.
        pool.scope(|s| {
            for i in 0..combinations_number {
                let set = rcs.get_set(i).clone();

                // Clone arcs & sender for move into task
                let dict = Arc::clone(&dict_arc);
                let actual_sig = Arc::clone(&actual_target_signature_arc);
                let included_txt = Arc::clone(&included_text_arc);
                let task_sender = sender.clone();
                let explored_sets_clone = self.explored_sets.clone();

                s.spawn(move |_| {
                    let mut search_thread =
                        search::SearchThread::new(dict, actual_sig, included_txt, set, task_sender);
                    search_thread.run();
                    explored_sets_clone.fetch_add(1, Ordering::Relaxed);
                });
            }
            // When the scope ends, all spawned tasks are guaranteed to have completed,
            // and their clones of `sender` will be dropped.
        });

        let now = Instant::now();
        let elapsed = now.duration_since(timer_start);
        println!("\n\n[*] Search time: {:.2?}", elapsed);

        drop(sender); // Drop the first sender to avoid deadlock

        // Join the I/O thread
        let thread_result = io_handle.join();

        let anagram_count = match thread_result {
            // IO thread completed without panic, but might have returned an Err<io::Error>
            Ok(io_res) => io_res.map_err(|e| format!("IO thread error: {}", e))?,

            // IO thread panicked (JoinHandle::join returns Err)
            Err(e) => {
                if let Some(panic_msg) = e.downcast_ref::<&str>() {
                    return Err(format!("IO thread panicked: {}", panic_msg));
                } else if let Some(panic_msg) = e.downcast_ref::<String>() {
                    return Err(format!("IO thread panicked: {}", panic_msg));
                } else {
                    return Err("IO thread panicked with unknown type.".to_string());
                }
            }
        };

        Ok(anagram_count)
    }

    /// Consumes anagrams from the receiver and writes them to file. Returns anagram count or IO error.
    fn io_loop(
        receiver: Receiver<String>,
        output_file_name: String,
        explored_sets: Arc<AtomicU64>,
        sets_number: usize,
    ) -> Result<u64, std::io::Error> {
        let mut last_display_time = Instant::now();

        // Open output file
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&output_file_name)?;

        let mut writer = BufWriter::new(file);
        let mut anagram_count: u64 = 0;

        for anagram in receiver.iter() {
            writeln!(writer, "{}", anagram)?;

            anagram_count += 1;

            // Update console every 1 second
            let now = Instant::now();
            if now.duration_since(last_display_time) >= Duration::from_millis(1000) {
                print!(
                    "\r[{}/{} sets] {}: {}{}",
                    explored_sets.load(Ordering::Relaxed),
                    sets_number,
                    anagram_count,
                    anagram,
                    " ".repeat(30)
                );
                std::io::stdout().flush()?;
                writer.flush()?; //Flush periodically on file
                last_display_time = now;
            }
        }

        // Final flush after the channel is exhausted
        writer.flush()?;

        Ok(anagram_count)
    }

    // Debug print function
    pub fn print(&self) {
        println!("\nArmaMagna multi-threaded anagrammer engine\n");

        println!("{:<40}{}", "[*] Source text:", self.target_text);
        println!("{:<40}{}", "[*] Dictionary:", self.dictionary_name);
        println!(
            "{:<40}{}",
            "[*] Included text:",
            if self.included_text.is_empty() {
                "<void>"
            } else {
                &self.included_text
            }
        );
        println!(
            "{:<40}({},{})",
            "[*] Cardinality:", self.min_cardinality, self.max_cardinality
        );
        println!(
            "{:<40}({},{})",
            "[*] Word lengths:", self.min_wordlength, self.max_wordlength
        );
        println!("{:<40}{}", "[*] Estimated concurrency:", num_cpus::get());
        println!("{:<40}{}", "[*] Threads to launch:", self.num_threads);
        println!();

        println!(
            "{:<40}{}",
            "[*] Target signature:",
            self.target_signature.to_string()
        );
        println!(
            "{:<40}{}",
            "[*] Included words number:", self.included_words_number
        );
        println!(
            "{:<40}{}",
            "[*] Included text signature:",
            if self.included_text.is_empty() {
                "<void>".to_string()
            } else {
                format!("{}", self.included_text_signature.to_string())
            }
        );
        println!(
            "{:<40}{}",
            "[*] Actual target signature:",
            self.actual_target_signature.to_string()
        );
        println!(
            "{:<40}({},{})",
            "[*] Actual cardinality:", self.actual_min_cardinality, self.actual_max_cardinality
        );
        println!();
    }
}

// Counts words in a string
fn count_words(s: &str) -> usize {
    s.split(' ').filter(|w| !w.is_empty()).count()
}
