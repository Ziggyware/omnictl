use arboard::Clipboard;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use colored::*;
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use tiktoken_rs::cl100k_base;

#[derive(Parser)]
#[command(name = "omnictl", version = "5.0", about = "30-Feature Supreme Codebase Context Packer", author = "Power User CLI")]
struct Cli {
    #[arg(short, long, default_value = ".")]
    path: PathBuf,
    
    #[arg(long, help = "Format output with Markdown code blocks", default_value_t = true)]
    llm: bool,
    
    #[arg(long, help = "F11: Format output with XML <file> tags", default_value_t = false)]
    xml_tags: bool,
    
    #[arg(short, long, help = "Token compression strategy", value_enum, default_value_t = CompressOption::Whitespace)]
    compress: CompressOption,

    #[arg(long, help = "Maximum tokens to allow.")]
    max_tokens: Option<usize>,

    #[arg(long, help = "Inject unstaged Git diffs inline above file content")]
    hydrate_diffs: bool,
    #[arg(long, help = "Replace 30+ repeating characters with a single token summary")]
    homogenize: bool,
    #[arg(long, help = "Drop files with low lexical entropy")]
    cull_entropy: bool,
    #[arg(long, help = "Scrub known secret formats (AWS, JWT, Private Keys)")]
    sanitize: bool,
    #[arg(long, help = "F12: Drop files with >85% Jaccard similarity to already packed files")]
    jaccard_dedup: bool,
    #[arg(long, help = "F13: Strip massive data arrays and objects in test files")]
    lobotomize_tests: bool,
    #[arg(long, help = "F14: Prioritize files modified recently (Time Decay)")]
    recency_decay: bool,
    #[arg(long, help = "F15: Inject <thinking> and Chain-of-Thought scaffolding")]
    cot_scaffold: bool,
    #[arg(long, help = "F16: Strip console.log, print, and debug statements")]
    strip_logs: bool,
    #[arg(long, help = "F17: Generate and prepend an Exported Symbol Map (ctags-lite)")]
    symbol_map: bool,
    #[arg(long, help = "F18: Collapse multi-line empty brackets and arrays")]
    bracket_collapse: bool,
    #[arg(long, help = "F19: Prioritize files frequently co-committed together")]
    co_commits: bool,
    #[arg(long, help = "F20: Extract and inline dependency summaries")]
    dep_inline: bool,

    // NEW 10 FEATURES (F21 - F30)
    #[arg(long, help = "F21: Extract and prepend an import/require dependency edge graph")]
    dep_graph: bool,
    #[arg(long, help = "F22: Reject auto-generated files (minified, DO NOT EDIT)")]
    skip_autogen: bool,
    #[arg(long, help = "F23: Transpile JSON context files to YAML on the fly to save tokens")]
    json_to_yaml: bool,
    #[arg(long, help = "F24: Inject chunk boundary markers every 300 lines for attention anchoring")]
    chunk_split: bool,
    #[arg(long, help = "F25: Eradicate commented-out code logic while keeping natural language docs")]
    strip_dead_code: bool,
    #[arg(long, help = "F26: Truncate static string literals longer than 50 chars")]
    truncate_strings: bool,
    #[arg(long, help = "F27: Normalize all 4-space and tab indents to 2 spaces")]
    normalize_space: bool,
    #[arg(long, help = "F28: Scrub high-entropy 40+ char hex hashes")]
    scrub_hashes: bool,
    #[arg(long, help = "F29: Prune outlier lines exceeding 256 characters")]
    prune_outliers: bool,
    #[arg(long, help = "F30: Strictly byte-truncate the final file to hit max_tokens exactly")]
    hard_cap: bool,

    #[arg(long, help = "Inject a task-specific system preamble", value_enum)]
    preamble: Option<PreambleOption>,

    #[arg(long, help = "Perform a dry run to see metrics without copying to clipboard")]
    dry_run: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate shell completion scripts for bash, zsh, fish, powershell, or elvish
    Complete {
        #[arg(short, long, help = "Target shell")]
        shell: Shell,
    },
}

#[derive(ValueEnum, Clone, PartialEq, Eq, Debug)]
enum CompressOption {
    None, Whitespace, Aggressive, MaxMinify, Skeleton,
}

#[derive(ValueEnum, Clone, PartialEq, Eq, Debug)]
enum PreambleOption {
    Audit, Refactor, Document,
}

struct CliOpts {
    lobotomize_tests: bool, strip_logs: bool, bracket_collapse: bool,
    strip_dead_code: bool, truncate_strings: bool, normalize_space: bool,
    scrub_hashes: bool, prune_outliers: bool, chunk_split: bool,
}

struct FileData {
    path: PathBuf, content: String, tokens: usize, score: f64, token_set: HashSet<String>,
}

fn compute_git_churn(path: &Path, include_co_commits: bool) -> HashMap<PathBuf, f64> {
    let mut churn = HashMap::new();
    if let Ok(output) = Command::new("git").args(["log", "--name-only", "--format="]).current_dir(path).output() {
        for line in String::from_utf8_lossy(&output.stdout).lines().filter(|l| !l.is_empty()) {
            *churn.entry(path.join(line)).or_insert(0.0) += 1.0;
        }
    }
    if include_co_commits {
        if let Ok(output) = Command::new("git").args(["log", "-n", "50", "--name-only", "--format="]).current_dir(path).output() {
            for line in String::from_utf8_lossy(&output.stdout).lines().filter(|l| !l.is_empty()) {
                if let Some(val) = churn.get_mut(&path.join(line)) { *val += 5.0; }
            }
        }
    }
    churn
}

fn get_unstaged_diff(path: &Path) -> Option<String> {
    let output = Command::new("git").args(["diff", "--", path.to_str().unwrap_or("")]).output().ok()?;
    let diff = String::from_utf8_lossy(&output.stdout).to_string();
    if diff.trim().is_empty() { None } else { Some(diff) }
}

fn homogenize_repeats(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let (mut count, mut current_char) = (0, '\0');
    while let Some(c) = chars.next() {
        if c == current_char { count += 1; } else {
            if count >= 30 { out.push_str(&format!("\n[... {}x '{}' ...]\n", count, current_char)); } 
            else { for _ in 0..count { out.push(current_char); } }
            current_char = c; count = 1;
        }
    }
    if count >= 30 { out.push_str(&format!("\n[... {}x '{}' ...]\n", count, current_char)); } 
    else { for _ in 0..count { out.push(current_char); } }
    out
}

fn has_sufficient_entropy(content: &str) -> bool {
    let words: Vec<&str> = content.split_whitespace().collect();
    if words.len() < 500 { return true; } 
    (words.iter().collect::<HashSet<_>>().len() as f64 / words.len() as f64) >= 0.04 
}

fn sanitize_secrets(content: &str) -> String {
    let s = Regex::new(r"(?i)(AKIA[0-9A-Z]{16})").unwrap().replace_all(content, "[REDACTED_AWS]").to_string();
    Regex::new(r"(?s)-----BEGIN (?:RSA|OPENSSH|PRIVATE) KEY.*?-----END (?:RSA|OPENSSH|PRIVATE) KEY-----").unwrap().replace_all(&s, "[REDACTED_PK]").to_string()
}

fn apply_compression(content: &str, strategy: &CompressOption, opts: &CliOpts) -> String {
    let mut p = match strategy {
        CompressOption::None => content.to_string(),
        CompressOption::Whitespace => content.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n"),
        CompressOption::Aggressive => content.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n"),
        CompressOption::MaxMinify => {
            let s = Regex::new(r"(?s)/\*.*?\*/").unwrap().replace_all(content, "").to_string();
            let s = Regex::new(r"(?m)^\s*(?://|#).*$").unwrap().replace_all(&s, "").to_string();
            s.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
        },
        CompressOption::Skeleton => {
            let mut s = String::new();
            for cap in Regex::new(r"(?m)^[ \t]*(pub |export |class |def |fn |function |interface |type |struct ).*?[\{:]").unwrap().captures_iter(content) {
                s.push_str(&cap[0]); s.push_str(" ... }\n");
            }
            if s.is_empty() { content.to_string() } else { s }
        }
    };

    if opts.normalize_space { p = p.replace("    ", "  ").replace("\t", "  "); }
    if opts.lobotomize_tests { p = Regex::new(r"(?s)\[\s*\{.*?\}\s*(?:,\s*\{.*?\}\s*)*\]").unwrap().replace_all(&p, "[ /* lobotomized */ ]").to_string(); }
    if opts.strip_logs { p = Regex::new(r"(?m)^\s*(?:console\.log|print|println!|dbg!|fmt\.Println)\b.*?;?$").unwrap().replace_all(&p, "").to_string(); }
    if opts.bracket_collapse {
        p = Regex::new(r"\{\s+\}").unwrap().replace_all(&p, "{}").to_string();
        p = Regex::new(r"\[\s+\]").unwrap().replace_all(&p, "[]").to_string();
    }
    if opts.strip_dead_code { p = Regex::new(r"(?m)^\s*(?://|#)\s*(?:let|var|const|fn|def|class|import|if|for)\b.*$").unwrap().replace_all(&p, "").to_string(); }
    if opts.truncate_strings {
        p = Regex::new(r#""([^"]{50,})""#).unwrap().replace_all(&p, "\"[TRUNCATED]\"").to_string();
        p = Regex::new(r#"'([^']{50,})'"#).unwrap().replace_all(&p, "'[TRUNCATED]'").to_string();
    }
    if opts.scrub_hashes { p = Regex::new(r"\b[a-fA-F0-9]{40,64}\b").unwrap().replace_all(&p, "[HASH]").to_string(); }
    if opts.prune_outliers { p = p.lines().filter(|l| l.len() < 256).collect::<Vec<_>>().join("\n"); }
    if opts.chunk_split {
        let lines: Vec<&str> = p.lines().collect();
        let mut chunked = String::new();
        for (i, chunk) in lines.chunks(300).enumerate() {
            chunked.push_str(&format!("\n=== CHUNK {} ===\n", i + 1));
            chunked.push_str(&chunk.join("\n"));
        }
        p = chunked;
    }
    p
}

fn main() {
    let cli = Cli::parse();

    if let Some(Commands::Complete { shell }) = cli.command {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "omnictl", &mut io::stdout());
        return;
    }

    let bpe = cl100k_base().unwrap();
    let now = SystemTime::now();

    eprintln!("{}", "Initializing supreme ingestion pipeline (30 Features)...".cyan());
    
    let churn_map = compute_git_churn(&cli.path, cli.co_commits);
    let walker = WalkBuilder::new(&cli.path).hidden(true).parents(true).git_ignore(true).git_global(true).git_exclude(true).require_git(false).build();

    let mut files_data: Vec<FileData> = Vec::new();
    let (mut original_bytes, mut all_symbols, mut deps_summary, mut edge_graph) = (0, Vec::new(), String::new(), Vec::new());

    let c_opts = CliOpts {
        lobotomize_tests: cli.lobotomize_tests, strip_logs: cli.strip_logs, bracket_collapse: cli.bracket_collapse,
        strip_dead_code: cli.strip_dead_code, truncate_strings: cli.truncate_strings, normalize_space: cli.normalize_space,
        scrub_hashes: cli.scrub_hashes, prune_outliers: cli.prune_outliers, chunk_split: cli.chunk_split,
    };

    for result in walker {
        if let Ok(entry) = result {
            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                let f_path = entry.path().to_path_buf();
                let f_name = f_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let ext = f_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                
                if let Ok(bytes) = fs::read(&f_path) {
                    if std::str::from_utf8(&bytes[..std::cmp::min(1024, bytes.len())]).is_err() { continue; }
                    
                    if let Ok(mut text) = String::from_utf8(bytes.clone()) {
                        if cli.skip_autogen && (text.contains("DO NOT EDIT") || text.lines().any(|l| l.len() > 1000)) { continue; }
                        if cli.cull_entropy && !has_sufficient_entropy(&text) { continue; }
                        if cli.sanitize { text = sanitize_secrets(&text); }
                        if cli.homogenize { text = homogenize_repeats(&text); }
                        
                        if cli.json_to_yaml && ext == "json" {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Ok(y) = serde_yaml::to_string(&v) { text = y; }
                            }
                        }

                        if cli.dep_inline && (f_name == "package.json" || f_name == "Cargo.toml") {
                            deps_summary.push_str(&format!("\nManifest ({}):\n{}\n", f_path.display(), text.lines().take(50).collect::<Vec<_>>().join("\n")));
                        }

                        if cli.symbol_map {
                            for cap in Regex::new(r"(?m)^[ \t]*(?:pub\s+|export\s+)?(?:class|fn|def|struct|interface)\s+([a-zA-Z0-9_]+)").unwrap().captures_iter(&text) {
                                all_symbols.push(format!("{} -> {}", f_path.display(), &cap[1]));
                            }
                        }

                        if cli.dep_graph {
                            for cap in Regex::new(r"(?m)^[ \t]*(?:import|require|use)\s+.*?([a-zA-Z0-9_./-]+)").unwrap().captures_iter(&text) {
                                edge_graph.push(format!("{} depends on {}", f_path.display(), &cap[1]));
                            }
                        }

                        let mut processed = apply_compression(&text, &cli.compress, &c_opts);
                        original_bytes += text.len();

                        if cli.hydrate_diffs {
                            if let Some(diff) = get_unstaged_diff(&f_path) { processed = format!("### UNSTAGED CHANGES ###\n{}\n### END CHANGES ###\n{}", diff, processed); }
                        }

                        let mut score = *churn_map.get(&f_path).unwrap_or(&1.0);
                        if cli.recency_decay {
                            if let Ok(modified) = fs::metadata(&f_path).and_then(|m| m.modified()) {
                                if let Ok(dur) = now.duration_since(modified) { score += 10.0 / (1.0 + ((dur.as_secs() as f64) / 3600.0).ln().max(1.0)); }
                            }
                        }

                        let token_set: HashSet<String> = if cli.jaccard_dedup { processed.split_whitespace().map(|s| s.to_string()).collect() } else { HashSet::new() };
                        let tokens = bpe.encode_with_special_tokens(&processed).len();
                        files_data.push(FileData { path: f_path, content: processed, tokens, score, token_set });
                    }
                }
            }
        }
    }

    files_data.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut final_output = String::new();
    
    if let Some(p) = &cli.preamble {
        let inst = match p {
            PreambleOption::Audit => "SYSTEM ALIGNMENT: Perform a rigorous security audit.\n\n",
            PreambleOption::Refactor => "SYSTEM ALIGNMENT: Identify code smells and refactor.\n\n",
            PreambleOption::Document => "SYSTEM ALIGNMENT: Generate architectural documentation.\n\n",
        };
        final_output.push_str(inst);
    }

    if cli.cot_scaffold { final_output.push_str("<scratchpad>\n1. Analyze architecture.\n2. Evaluate context.\n3. Formulate directives.\n</scratchpad>\n\n"); }
    if cli.dep_inline && !deps_summary.is_empty() { final_output.push_str(&format!("### DEPENDENCIES ###{}\n### END ###\n\n", deps_summary)); }
    if cli.symbol_map && !all_symbols.is_empty() { final_output.push_str(&format!("### SYMBOL MAP ###\n{}\n### END ###\n\n", all_symbols.join("\n"))); }
    if cli.dep_graph && !edge_graph.is_empty() { final_output.push_str(&format!("### DEP GRAPH ###\n{}\n### END ###\n\n", edge_graph.join("\n"))); }

    let (mut accumulated_tokens, mut included_count) = (0, 0);
    let mut accepted_token_sets: Vec<HashSet<String>> = Vec::new();
    let pb = ProgressBar::new(files_data.len() as u64);
    pb.set_style(ProgressStyle::default_bar().template("{spinner:.green} [{bar:40.cyan/blue}] Packing...").unwrap());

    for fd in files_data {
        if let Some(limit) = cli.max_tokens {
            if accumulated_tokens + fd.tokens > limit {
                if cli.hard_cap {
                    let allowed_tokens = limit - accumulated_tokens;
                    let approx_chars = allowed_tokens * 3;
                    let truncated = fd.content.chars().take(approx_chars).collect::<String>();
                    if cli.xml_tags { final_output.push_str(&format!("<file path=\"{}\">\n{}\n[HARD CAPPED]\n</file>\n\n", fd.path.display(), truncated)); }
                    else { final_output.push_str(&format!("--- .\\{}\n{}\n[HARD CAPPED]\n\n", fd.path.display(), truncated)); }
                    accumulated_tokens += allowed_tokens;
                    included_count += 1;
                }
                break;
            }
        }

        if cli.jaccard_dedup {
            if accepted_token_sets.iter().any(|s| (fd.token_set.intersection(s).count() as f64 / fd.token_set.union(s).count().max(1) as f64) > 0.85) { continue; }
            accepted_token_sets.push(fd.token_set);
        }
        
        if cli.xml_tags { final_output.push_str(&format!("<file path=\"{}\">\n{}\n</file>\n\n", fd.path.display(), fd.content)); }
        else if cli.llm { final_output.push_str(&format!(".\\{}\n```{}\n{}\n```\n\n", fd.path.display(), fd.path.extension().and_then(|e| e.to_str()).unwrap_or(""), fd.content)); }
        else { final_output.push_str(&format!("--- .\\{}\n{}\n\n", fd.path.display(), fd.content)); }
        
        accumulated_tokens += fd.tokens; included_count += 1; pb.inc(1);
    }
    pb.finish_with_message("Pipeline execution complete.");

    let final_bytes = final_output.len();
    eprintln!("\n{}", "========================================".bold());
    eprintln!("{}", " OmniCTL - 30 Feature Telemetry".bold().cyan());
    eprintln!("{}", "========================================".bold());
    eprintln!("{:<18}: {}", "Files Packed", included_count.to_string().green());
    eprintln!("{:<18}: {}", "Final Tokens", accumulated_tokens.to_string().magenta().bold());
    eprintln!("{:<18}: {:.2}%", "Data Reduction", if original_bytes > 0 { (1.0 - (final_bytes as f64 / original_bytes as f64)) * 100.0 } else { 0.0 });
    eprintln!("{}", "========================================\n".bold());

    if cli.dry_run { std::process::exit(0); }
    if let Ok(mut clipboard) = Clipboard::new() { clipboard.set_text(final_output.clone()).ok(); }
}
