# Bracketeer for Zed

Bracketeer manipulates the nearest bracket or quote pair around each active Zed
selection. It is a Zed port of the VS Code extension
[Bracketeer](https://github.com/Pustelto/Bracketeer).

## Commands

- `bracketeer.swapBrackets`
- `bracketeer.removeBrackets`
- `bracketeer.selectBracketContent`
- `bracketeer.changeBracketsTo.parentheses`
- `bracketeer.changeBracketsTo.square`
- `bracketeer.changeBracketsTo.curly`
- `bracketeer.changeBracketsTo.angle`
- `bracketeer.swapQuotes`
- `bracketeer.removeQuotes`
- `bracketeer.selectQuotesContent`
- `bracketeer.changeQuotesTo.single`
- `bracketeer.changeQuotesTo.double`
- `bracketeer.changeQuotesTo.backtick`

## Default Key Bindings

The extension declares default key bindings for editors and git diffs:

| Command | Linux/Windows | macOS |
| --- | --- | --- |
| Swap brackets | `ctrl-alt-shift-k` | `cmd-alt-shift-k` |
| Remove brackets | `ctrl-alt-shift-i` | `cmd-alt-shift-i` |
| Select bracket content | `ctrl-alt-shift-h` | `cmd-alt-shift-h` |
| Swap quotes | `ctrl-alt-shift-semicolon` | `cmd-alt-shift-semicolon` |
| Remove quotes | `ctrl-alt-shift-quote` | `cmd-alt-shift-quote` |
| Select quote content | `ctrl-alt-shift-0` | `cmd-alt-shift-0` |

## Development

This extension currently depends on unreleased Zed editor-command extension
APIs. Until those APIs are published in `zed_extension_api`, build it from a
checkout where `zed-bracketeer` and `zed` are sibling directories:

```sh
cargo test
cargo clippy --all-targets
cargo build --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/zed_bracketeer.wasm extension.wasm
```

When `zed_extension_api = "0.8.0"` is available on crates.io, replace the local
path dependency in `Cargo.toml` with the published crate version.

## License

MIT. This port is based on the MIT-licensed VS Code Bracketeer extension by
Tomas Pustelnik.
