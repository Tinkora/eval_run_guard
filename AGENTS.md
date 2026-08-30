# Repository instructions

- Use English Conventional Commits and English code comments.
- Keep the default README in English with a Chinese entry point.
- Preserve the privacy boundary: explicit input only, offline/read-only, no content values or absolute paths in output.
- Use TDD for behavior changes.
- Before committing run `cargo fmt --all -- --check`, `cargo test --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Any HTML or user-facing web UI work must use the `ui-ux-pro-max` skill first.
