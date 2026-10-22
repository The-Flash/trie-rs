use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::time::Instant;

use trie_rs::{DiskTrie, TrieBuilder};

const USAGE: &str = "\
usage:
  trie-rs build <words.txt> <out.trie>
  trie-rs query <file.trie> <prefix> [--limit N]
  trie-rs repl  <file.trie> [--limit N]";

const DEFAULT_LIMIT: usize = 10;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("build") if args.len() == 3 => build(&args[1], &args[2]),
        Some("query") if args.len() >= 3 => {
            parse_limit(&args[3..]).and_then(|limit| query(&args[1], &args[2], limit))
        }
        Some("repl") if args.len() >= 2 => {
            parse_limit(&args[2..]).and_then(|limit| repl(&args[1], limit))
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_limit(rest: &[String]) -> io::Result<usize> {
    match rest {
        [] => Ok(DEFAULT_LIMIT),
        [flag, n] if flag == "--limit" => n
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("bad --limit: {n}"))),
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE)),
    }
}

fn build(words: &str, out: &str) -> io::Result<()> {
    let start = Instant::now();
    let builder = TrieBuilder::from_word_file(words)?;
    builder.write_to(out)?;
    let size = std::fs::metadata(out)?.len();
    println!(
        "wrote {out}: {} nodes, {size} bytes in {:.2?}",
        builder.node_count(),
        start.elapsed()
    );
    Ok(())
}

fn query(path: &str, prefix: &str, limit: usize) -> io::Result<()> {
    let trie = DiskTrie::open(path)?;
    print_result(&trie, prefix, limit)
}

fn repl(path: &str, limit: usize) -> io::Result<()> {
    let trie = DiskTrie::open(path)?;
    println!("{} nodes loaded. Type a prefix (Ctrl-D to quit).", trie.node_count()?);
    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            println!();
            return Ok(());
        }
        print_result(&trie, &line.trim().to_lowercase(), limit)?;
    }
}

fn print_result(trie: &DiskTrie, prefix: &str, limit: usize) -> io::Result<()> {
    let start = Instant::now();
    let next = trie.next_chars(prefix)?;
    let words = trie.complete(prefix, limit)?;
    let elapsed = start.elapsed();

    let Some(next) = next else {
        println!("no words start with {prefix:?}  ({elapsed:.2?})");
        return Ok(());
    };
    let letters: String = next.chars.iter().collect();
    println!("next letters: {letters}");
    if next.is_word {
        println!("{prefix:?} is itself a word");
    }
    println!("completions:  {}", words.join(", "));
    println!("({elapsed:.2?})");
    Ok(())
}
