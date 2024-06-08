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

Ratify warns about new files not found in the catalog, and allows you to add them using `ratify append` (note that this does not modify signatures for existing entries).

The catalog created by Ratify is compatible with `cfv`, so Ratify can be used to verify `cfv`-created signatures as well