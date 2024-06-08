# Ratify

Ratify is a tool for signing and verification of files and directory structures. It is an alternative to tools like `cfv`.

# Installation

```
$ cargo install ratify
```

# Usage

Sign a directory with files using a specific hash:

```
$ ratify sign -a sha1 .
```

This generates a DIRNAME.sha1 in the requested directory, which can later be verified by:

```
$ ratify test .
```