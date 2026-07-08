# 🔐 Ratify

[![Crates.io](https://img.shields.io/crates/v/ratify.svg)](https://crates.io/crates/ratify)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Ratify** is a fast, reliable tool for creating and verifying cryptographic signatures of files and directory structures. It's designed as a modern alternative to tools like `cfv`, with enhanced features for file integrity verification, batch operations, and interactive updating.

## ✨ Features

- **Multiple Hash Algorithms**: Support for MD5, SHA-1, SHA-256, SHA-512, and BLAKE3
- **Directory-Wide Verification**: Recursively sign and verify entire directory trees
- **File Exclusion**: Skip files from signing and verification with a gitignore-style `.ratify-ignore` file
- **Interactive Updates**: Selectively update checksums for changed files
- **Batch Operations**: Efficiently process large numbers of files with parallel execution
- **Existence-Only Checks**: Quickly find missing or new files without reading their contents
- **Progress Tracking**: Real-time progress bars for long-running operations
- **Flexible Reporting**: Generate verification reports in plain text or JSON format
- **Cross-Compatible**: Works with existing `cfv` signature files
- **Unknown File Detection**: Automatically detect new files not in the catalog

## 🚀 Installation

### Using Cargo (Recommended)

```bash
cargo install ratify
```

### From Source

```bash
git clone https://github.com/vmalloc/ratify.git
cd ratify
cargo build --release
```

## 📖 Quick Start

### Creating a Signature Catalog

Sign all files in the current directory using SHA-256:

```bash
ratify sign -a sha256 .
```

Or, set up a default algorithm in `~/.config/ratify.toml` and omit the flag:

```bash
# First, create ~/.config/ratify.toml with: default_sign_algo = "sha256"
ratify sign .
```

This creates a signature catalog file (e.g., `ratify-catalog.sha256`) in the directory, containing checksums for all files. For backward compatibility, ratify will also read catalogs named `ratify.<algo>` or `<dirname>.<algo>` (such as those written by `cfv`) if present.

#### Using a Custom Catalog File Location

You can specify a custom location for the catalog file using the `--catalog-file` flag:

```bash
# Create catalog with custom filename/location
ratify sign -a sha256 --catalog-file ./my-custom-catalog.sha256 .
```

**NOTE**: When using `--catalog-file`, the algorithm is inferred from the file's extension when possible (and, for `sign`, may come from `default_sign_algo` in your config). Specify `-a/--algo` explicitly if detection fails.

### Verifying Files

Verify the integrity of files against their signatures:

```bash
ratify test .
```

Ratify will check all files against the catalog and report any discrepancies.

#### Using a Custom Catalog File

When using a custom catalog file location, specify the same file for verification:

```bash
# Test using custom catalog file
ratify test --catalog-file ./my-custom-catalog.sha256 .

# Algorithm is auto-detected from file extension
ratify test --catalog-file checksums/backup.sha256 /path/to/directory

# If algorithm detection fails, specify explicitly
ratify test -a sha256 --catalog-file /tmp/custom-signatures .
```

### Excluding Files with `.ratify-ignore`

Place an optional `.ratify-ignore` file at the root of the directory you sign to exclude files from both signing and verification. It uses gitignore-style syntax:

- Bare names and globs (e.g. `*.log`, `cache`) match at **any depth** in the tree.
- A leading `/` **anchors** a pattern to the signed directory's root (e.g. `/build` matches only the top-level `build`, not `src/build`).
- Blank lines and `#` comments are ignored.
- The `.ratify-ignore` file itself is always excluded and never signed.

For example:

```gitignore
# Ignore all log files, anywhere in the tree
*.log

# Ignore a build directory at the root only
/build

# Ignore a specific file
secrets.env
```

Ignored files are omitted from the catalog during `sign`, and during `test`/`update` they are neither verified nor reported as `[UNKNOWN]`. If a file that is already in the catalog later becomes ignored, it is skipped during verification with a warning suggesting you re-sign to drop it from the catalog.

### Checking Existence Only

To quickly find missing or new files without reading and hashing every file, pass `--existence-only` to `test`. Cataloged files are checked only for their presence on disk (a path that no longer exists, or was replaced by a non-file such as a directory, is reported as `[MISSING]`), and new files are still reported as `[UNKNOWN]`:

```bash
# Fast inventory check: find missing or new files without hashing
ratify test --existence-only /path/to/directory
```

Note that `--existence-only` cannot detect content changes, since it never reads file contents.

## 🔧 Usage

### Available Commands

| Command | Description |
|---------|-------------|
| `sign` | Create a new signature catalog for a directory |
| `test` | Verify files against an existing catalog |
| `update` | Interactively update checksums for changed files |
| `list-algos` | Show all available hash algorithms |

### Common Flags

| Flag | Description | Applies to |
|------|-------------|-----------|
| `-a, --algo <ALGORITHM>` | Specify hash algorithm explicitly | `sign`, `test`, `update` |
| `--catalog-file <PATH>` | Use custom catalog file location instead of default | `sign`, `test`, `update` |
| `-v, --verbose` | Increase verbosity (use multiple times for more detail); global flag, place it before the subcommand | All commands |
| `--overwrite` | Overwrite an existing catalog file without prompting | `sign` |
| `--report <FORMAT>` | Generate report in specified format (plain/json) | `test` |
| `--report-filename <FILE>` | Write report to file instead of stderr | `test` |
| `--existence-only` | Only check that cataloged files exist; skip reading/hashing (fast way to find missing/new files) | `test` |
| `--confirm` | Auto-confirm all updates without prompting | `update` |

### Supported Hash Algorithms

| Algorithm | Flag | Description |
|-----------|------|-------------|
| BLAKE3 | `blake3` | Fast, secure, modern hash function |
| SHA-256 | `sha256` | Industry standard, good balance of speed and security |
| SHA-512 | `sha512` | Higher security variant of SHA-2 |
| SHA-1 | `sha1` | Legacy support (consider upgrading to SHA-256+) |
| MD5 | `md5` | Legacy support (not recommended for security) |

## ⚙️ Configuration

Ratify supports global configuration through a TOML file located at `~/.config/ratify.toml`. This allows you to set default preferences that apply across all operations.

### Configuration File Format

```toml
# Default algorithm to use when --algo is not specified for signing
default_sign_algo = "blake3"
```

### Supported Configuration Options

| Option | Type | Description | Example |
|--------|------|-------------|---------|
| `default_sign_algo` | String | Default hash algorithm for signing operations | `"blake3"`, `"sha256"`, `"sha512"`, `"sha1"`, `"md5"` |

### Detailed Examples

#### Creating Signatures with Different Algorithms

```bash
# Use BLAKE3 (fastest, most secure)
ratify sign -a blake3 /path/to/directory

# Use SHA-256 (widely compatible)
ratify sign -a sha256 ~/documents

# With configuration file (default_sign_algo = "blake3")
ratify sign ~/documents  # Uses blake3 from config

# Signing always recurses into subdirectories
ratify sign -a sha256 /path/to/directory

# Overwrite an existing catalog without the confirmation prompt
ratify sign -a sha256 --overwrite /path/to/directory

# Custom catalog file location
ratify sign -a sha256 --catalog-file /backup/checksums.sha256 ~/documents
```

#### Verification and Reporting

```bash
# Basic verification
ratify test /path/to/directory

# Fast check for missing/new files only (no hashing)
ratify test --existence-only /path/to/directory

# Generate a JSON report
ratify test --report json --report-filename verification_report.json /path/to/directory

# Specify algorithm explicitly
ratify test -a sha256 /path/to/directory

# Use custom catalog file
ratify test --catalog-file /backup/checksums.sha256 ~/documents

# Custom catalog with explicit algorithm (if auto-detection fails)
ratify test -a sha256 --catalog-file /tmp/custom-signatures ~/documents
```

#### Managing File Changes

```bash
# Interactively update changed files and add new files
ratify update /path/to/directory

# Update with specific algorithm
ratify update -a sha256 /path/to/directory

# Update using custom catalog file
ratify update --catalog-file /backup/checksums.sha256 ~/documents

# Auto-confirm all changes
ratify update --confirm --catalog-file /backup/checksums.sha256 ~/documents
```

### Interactive Update Mode

When you run `ratify update`, you'll be prompted for each file with discrepancies:

```
[FAIL] "document.txt"
  Status: Checksum mismatch
[S]kip [U]pdate [D]irectory [A]ll (default: Skip): u
```

- **Skip (S)**: Leave this file unchanged
- **Update (U)**: Update just this file's checksum
- **Directory (D)**: Update all files in this directory
- **All (A)**: Update all remaining files with discrepancies

After you've made your selections, ratify lists the files it's about to update and asks for a final confirmation (`Proceed with updates? [y/N]`) before writing the catalog. Pass `--confirm` to skip both the per-file prompts and this final confirmation.

### Verbosity Control

Control output detail with the `-v` flag. Verbosity is a global flag, so it must be placed **before** the subcommand:

```bash
# Standard output
ratify test .

# Verbose output
ratify -v test .

# Debug output
ratify -vv test .
```

## 📋 Common Use Cases

### Archive Integrity Verification

Perfect for verifying downloaded archives, backup integrity, or ensuring file transfers completed successfully:

```bash
# Create signatures before backup
ratify sign -a blake3 ~/important_files

# Verify after restore
ratify test ~/important_files

# Use custom catalog location for backups
ratify sign -a blake3 --catalog-file /backup/metadata/checksums.blake3 ~/important_files
ratify test --catalog-file /backup/metadata/checksums.blake3 ~/important_files
```

### Software Distribution

Verify software packages and distributions:

```bash
# Sign release directory
ratify sign -a sha256 ./release_v1.0

# Users can verify download integrity
ratify test ./downloaded_release

# Distribute catalog separately for security
ratify sign -a sha256 --catalog-file ../release_v1.0_checksums.sha256 ./release_v1.0
# Users verify with:
ratify test --catalog-file ../release_v1.0_checksums.sha256 ./downloaded_release
```

### Ongoing File Monitoring

Monitor directories for unauthorized changes:

```bash
# Initial signature
ratify sign -a sha256 /etc/configs

# Later, check for changes
ratify test /etc/configs

# Update authorized changes
ratify update /etc/configs

# Store catalog in secure location
ratify sign -a sha256 --catalog-file /secure/etc-configs.sha256 /etc/configs
ratify test --catalog-file /secure/etc-configs.sha256 /etc/configs
```

## 🔍 Understanding Output

### Verification Status Codes

- **[OK]**: File verified successfully
- **[FAIL]**: Checksum mismatch (file modified)
- **[MISSING]**: File exists in catalog but not on disk
- **[UNKNOWN]**: File exists on disk but not in catalog


## 🤝 Contributing

We welcome contributions! Here's how to get started:

1. **Fork the repository** on GitHub
2. **Create a feature branch**: `git checkout -b feature/amazing-feature`
3. **Make your changes** and add tests if applicable
4. **Run tests**: `cargo test`
5. **Check formatting**: `cargo fmt`
6. **Run linting**: `cargo clippy`
7. **Commit your changes**: `git commit -m 'Add amazing feature'`
8. **Push to the branch**: `git push origin feature/amazing-feature`
9. **Open a Pull Request**

### Development Setup

```bash
git clone https://github.com/vmalloc/ratify.git
cd ratify
cargo build
cargo test
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.


## 🔗 Links

- **Repository**: [github.com/vmalloc/ratify](https://github.com/vmalloc/ratify)
- **Crates.io**: [crates.io/crates/ratify](https://crates.io/crates/ratify)
- **Documentation**: Available via `ratify --help` and subcommand help

---

*Built with ❤️ in Rust*