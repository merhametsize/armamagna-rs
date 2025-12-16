mod armamagna;
mod combinations;
mod dictionarium;
mod search;
mod signature;

use std::error::Error;
use std::thread;

use armamagna::ArmaMagna;

use clap::Parser;

#[derive(clap::Parser, Debug)]
#[command(author = "Gabriele Cassetta, @merhametsize", version, about = "ArmaMagna", long_about = None)]
#[command(
    after_help = "Example:\n  ./armamagna \"bazzecole andanti\" -d ../../data/it.txt --mincard 1 --maxcard 3"
)]
struct Args {
    /// Text to anagram
    text: String,

    /// Wordlist file path
    #[arg(short = 'd', long = "dict")]
    dictionary: String,

    /// Included text
    #[arg(short = 'i', long = "incl", default_value = "")]
    included_text: String,

    /// Minimum cardinality (number of words in the anagram)
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..), default_value="1")]
    mincard: u64,

    /// Maximum cardinality (number of words in the anagram)
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..), default_value="3")]
    maxcard: u64,

    /// Minimum word length
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..), default_value="1")]
    minwlen: u64,

    /// Maximum word length
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..), default_value="30")]
    maxwlen: u64,

    /// Output file
    #[arg(short = 'o', long = "out", default_value = "anagrams.txt")]
    output_file: String,

    /// Number of threads
    #[arg(short = 't', long = "thr", default_value_t = thread::available_parallelism().map(|n| n.get()).unwrap_or(1))]
    num_threads: usize,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Command line parsing
    let args = Args::parse();

    // Initialize ArmaMagna
    let mut am = ArmaMagna::new();
    am.set_options(
        &args.text,
        &args.dictionary,
        &args.output_file,
        &args.included_text,
        args.mincard,
        args.maxcard,
        args.minwlen,
        args.maxwlen,
        args.num_threads as u64,
    )?;

    // Run the search
    let anagrams_found = am.anagram()?;
    println!(
        "\nFound {} anagrams. Output written to {}.",
        anagrams_found, args.output_file
    );

    // Success return
    Ok(())
}
