# OmniCTL (v5.0.0)

OmniCTL is a native, high-performance CLI utility written in Rust, engineered specifically as an avant-garde context packer and token-optimization pipeline for power users of LLM platforms. It recursively traverses local source trees while strictly honoring hierarchical `.gitignore`, `.gitexclude`, and global ignore rules, processing files through a 30-feature heuristic curation engine to maximize signal-to-noise ratio and fit large repositories into tight token windows.

---

## Installation

### Prerequisites
* Rust toolchain (1.70+ recommended): [Install Rust](https://www.rust-lang.org/)
* System clipboard support (X11/Wayland dev libraries on Linux if clipboard copying is required).

### Building from Source

1. Clone the repository and navigate to the project directory:
   ```bash
   git clone <repository-url>
   omnictl
   ```
2. Compile and release the binary:
   ```bash
   cargo build --release
   ```
3. Move the binary to your system PATH (e.g., `/usr/local/bin` or `~/.cargo/bin`).

---

## Shell Completion Configuration

OmniCTL includes native shell completion generation powered by `clap_complete`. You can output completion scripts for your target shell directly to your configuration directories.

### Supported Shells
* `bash`
* `zsh`
* `fish`
* `powershell`
* `elvish`

### Installation Examples

**For Zsh:**
```bash
omnictl complete --shell zsh > ~/.zsh/_omnictl
autoload -U compinit && compinit
```

**For Bash:**
```bash
omnictl complete --shell bash > ~/.local/share/bash-completion/completions/omnictl
```

**For Fish:**
```bash
omnictl complete --shell fish > ~/.config/fish/completions/omnictl.fish
```

---

## Usage Overview

OmniCTL is designed with a flat CLI structure. Typing `omnictl` in any directory immediately packs the current path without requiring a nested subcommand.

```bash
# Basic usage (packs current directory with default whitespace compression)
omnictl

# Target a specific directory with XML tags and custom token limit
omnictl --path /path/to/project --xml-tags --max-tokens 32000
```

---

## Complete Command-Line Parameters Reference

| Option / Flag | Type | Default | Description |
|---|---|---|---|
| `-p, --path <PATH>` | PathBuf | `.` | Target directory to traverse and pack. |
| `--llm` | Bool | `true` | Format output blocks with standard Markdown code syntax. |
| `--xml-tags` | Bool | `false` | Format output blocks using Anthropic/Frontier-native XML `<file>` tags (overrides `--llm`). |
| `-c, --compress <STRATEGY>` | Enum | `whitespace` | Token compression strategy (`none`, `whitespace`, `aggressive`, `max-minify`, `skeleton`). |
| `--max-tokens <INT>` | usize | `None` | Strict token budget ceiling. Triggers fractional knapsack sorting by git churn. |
| `--hydrate-diffs` | Bool | `false` | Inlines unstaged git diffs directly above modified files. |
| `--homogenize` | Bool | `false` | Replaces 30+ repeating identical characters with a single summarizing token line. |
| `--cull-entropy` | Bool | `false` | Discards files failing Shannon lexical entropy thresholds (boilerplate/padding tests). |
| `--sanitize` | Bool | `false` | Scrubs known secret signatures (AWS keys, private keys, JWTs). |
| `--jaccard-dedup` | Bool | `false` | Rejects files sharing >85% Jaccard lexical similarity with already packed files. |
| `--lobotomize-tests` | Bool | `false` | Strips massive data test arrays and object fixtures from test files. |
| `--recency-decay` | Bool | `false` | Prioritizes files based on exponential OS modification time decay. |
| `--cot-scaffold` | Bool | `false` | Prepends Chain-of-Thought `<scratchpad>` reasoning blocks. |
| `--strip-logs` | Bool | `false` | Eradicates console logging, `print!`, `println!`, `dbg!`, and debug statements. |
| `--symbol-map` | Bool | `false` | Generates and prepends an exported symbol map (`ctags`-lite index). |
| `--bracket-collapse` | Bool | `false` | Collapses empty multi-line brackets and arrays (`{}` / `[]`). |
| `--co-commits` | Bool | `false` | Boosts file priority based on recent multi-file commit co-occurrence. |
| `--dep-inline` | Bool | `false` | Extracts and inlines dependency manifests (`package.json`, `Cargo.toml`). |
| `--dep-graph` | Bool | `false` | Generates and prepends an import/require dependency edge graph. |
| `--skip-autogen` | Bool | `false` | Drops auto-generated or minified files (`DO NOT EDIT` / >1000 char lines). |
| `--json-to-yaml` | Bool | `false` | Transpiles `.json` context files to YAML on-the-fly to reduce syntactic tokens. |
| `--chunk-split` | Bool | `false` | Injects attention-anchoring boundary markers every 300 lines (`=== CHUNK N ===`). |
| `--strip-dead-code` | Bool | `false` | Eradicates commented-out code logic while preserving natural language documentation. |
| `--truncate-strings` | Bool | `false` | Truncates static string literals exceeding 50 characters to `"[TRUNCATED]"`. |
| `--normalize-space` | Bool | `false` | Normalizes all 4-space and tab indentations down to uniform 2-space indents. |
| `--scrub-hashes` | Bool | `false` | Scrubs high-entropy 40+ character hex hashes and digests. |
| `--prune-outliers` | Bool | `false` | Prunes individual outlier lines exceeding 256 characters. |
| `--hard-cap` | Bool | `false` | Byte-truncates the final packed payload to hit `--max-tokens` precisely. |
| `--preamble <PREAMBLE>` | Enum | `None` | Injects a task-specific alignment preamble (`audit`, `refactor`, `document`). |
| `--dry-run` | Bool | `false` | Executes pipeline and displays metrics without modifying the system clipboard. |

---

## Meticulous Technical Details of Key Options

### 1. Compression Strategies (`--compress`)
* **`none`**: Preserves raw file content byte-for-byte.
* **`whitespace`** *(Default)*: Strips trailing whitespace from all lines, eliminating accidental right-margin inflation.
* **`aggressive`**: Trims both leading and trailing whitespace while filtering out completely empty lines.
* **`max-minify`**: Combines whitespace stripping with full comment block eradication and line-by-line minification.
* **`skeleton`**: Executes AST-lite regex parsing to retain signatures (`pub`, `fn`, `struct`, `class`, `def`) while dropping full function bodies.

### 2. Git Churn & Fractional Knapsack (`--max-tokens`)
When a maximum token cap is provided, OmniCTL queries `git log` to calculate the modification frequency (**churn**) of every file. It solves a fractional knapsack problem where files with higher historical change frequency take precedence over static files, ensuring your active working set fits securely within the context boundary.

### 3. Jaccard Lexical Deduplication (`--jaccard-dedup`)
To prevent token wastage on boilerplate configuration files or cloned templates, this feature tokenizes each file and calculates its intersection-over-union ratio against already packed files. Any file exceeding an **85% similarity threshold** is automatically omitted.

### 4. Semantic Sanitization & Security (`--sanitize`)
Before encoding tokens, OmniCTL parses lines against strict regular expressions to catch accidental secret leaks, replacing AWS Access Keys (`AKIA...`) and cryptographic private key headers (`-----BEGIN PRIVATE KEY-----`) with explicit redaction tags.