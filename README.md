# 🔐 Ratify

[![Crates.io](https://img.shields.io/crates/v/ratify.svg)](https://crates.io/crates/ratify)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Ratify** is a fast, reliable tool for creating and verifying cryptographic signatures of files and directory structures. It's designed as a modern alternative to tools like `cfv`, with enhanced features for file integrity verification, batch operations, and interactive updating.

## ✨ Features

- **Multiple Hash Algorithms**: Support for MD5, SHA-1, SHA-256, SHA-512, and BLAKE3
- **Directory-Wide Verification**: Recursively sign and verify entire directory trees
- **Interactive Updates**: Selectively update checksums for changed files
- **Batch Operations**: Efficiently process large numbers of files with parallel execution
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

This creates a signature catalog file (e.g., `dirname.sha256`) containing checksums for all files.

### Verifying Files

Verify the integrity of files against their signatures:

```bash
ratify test .
```

Ratify will check all files against the catalog and report any discrepancies.

## 🔧 Usage

### Available Commands

| Command | Description |
|---------|-------------|
| `sign` | Create a new signature catalog for a directory |
| `test` | Verify files against an existing catalog |
| `append` | Add new files to an existing catalog |
| `update` | Interactively update checksums for changed files |
| `list-algos` | Show all available hash algorithms |

### Supported Hash Algorithms

| Algorithm | Flag | Description |
|-----------|------|-------------|
| BLAKE3 | `blake3` | Fast, secure, modern hash function |
| SHA-256 | `sha256` | Industry standard, good balance of speed and security |
| SHA-512 | `sha512` | Higher security variant of SHA-2 |
| SHA-1 | `sha1` | Legacy support (consider upgrading to SHA-256+) |
| MD5 | `md5` | Legacy support (not recommended for security) |

### Detailed Examples

#### Creating Signatures with Different Algorithms

```bash
# Use BLAKE3 (fastest, most secure)
ratify sign -a blake3 /path/to/directory

# Use SHA-256 (widely compatible)
ratify sign -a sha256 ~/documents

# Recursive signing (default behavior)
ratify sign -a sha256 -r /path/to/directory
```

#### Verification and Reporting

```bash
# Basic verification
ratify test /path/to/directory

# Generate a JSON report
ratify test --report json --report-filename verification_report.json /path/to/directory

# Specify algorithm explicitly
ratify test -a sha256 /path/to/directory
```

#### Managing File Changes

```bash
# Add new files to existing catalog
ratify append /path/to/directory

# Interactively update changed files
ratify update /path/to/directory

# Update with specific algorithm
ratify update -a sha256 /path/to/directory
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

### Verbosity Control

Control output detail with the `-v` flag:

```bash
# Standard output
ratify test .

# Verbose output
ratify test -v .

# Debug output
ratify test -vv .
```

## 📋 Common Use Cases

### Archive Integrity Verification

Perfect for verifying downloaded archives, backup integrity, or ensuring file transfers completed successfully:

```bash
# Create signatures before backup
ratify sign -a blake3 ~/important_files

# Verify after restore
ratify test ~/important_files
```

### Software Distribution

Verify software packages and distributions:

```bash
# Sign release directory
ratify sign -a sha256 ./release_v1.0

# Users can verify download integrity
ratify test ./downloaded_release
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
```

## 🔍 Understanding Output

### Verification Status Codes

- **[OK]**: File verified successfully
- **[FAIL]**: Checksum mismatch (file modified)
- **[MISSING]**: File exists in catalog but not on disk
- **[UNKNOWN]**: File exists on disk but not in catalog

### Exit Codes

- `0`: Success (all files verified or operation completed)
- `-1`: Failure (verification errors or other issues)

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