# Language support

## Supported

Rust, Python, TypeScript, JavaScript, TSX.

## Awaiting promotion

These languages are candidates and are promoted only after they clear the gates
described in `CONTRIBUTING.md`:

Go, Java, C#, C, C++, Ruby, PHP.

## Permanently excluded

These are out of scope because they carry no call graph for corbel to map:

| Language | Reason                                              |
| -------- | --------------------------------------------------- |
| HTML     | Markup, not executable call structure.              |
| CSS      | Styling rules, not executable call structure.       |
| JSON     | Data, not executable call structure.                |
| Bash     | Shell scripting outside corbel's resolution model.  |

## The LanguageSupport contract

Each supported language provides an implementation that supplies:

- File extension and grammar association.
- Symbol extraction from a parse tree.
- Reference extraction from a parse tree.
- `extract_signature` for a symbol.
- `is_public` for a symbol.
- `build_scope` for a file.
