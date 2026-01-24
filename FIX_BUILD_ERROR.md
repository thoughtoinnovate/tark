# Build Fix: Unstable `unsigned_is_multiple_of`

## Issue
The build failed during the Docker push stage with the following error:
```
error[E0658]: use of unstable library feature `unsigned_is_multiple_of`
   --> src/tools/shell.rs:260:59
    |
260 | ...                   if stderr_lines.len().is_multiple_of(50) {
    |                                             ^^^^^^^^^^^^^^
    |
    = note: see issue #128101 <https://github.com/rust-lang/rust/issues/128101> for more information
```

This occurred because `is_multiple_of` is an unstable feature in Rust and requires a nightly compiler or a feature flag, but the project is being built on stable Rust.

## Resolution
Replaced the unstable method `.is_multiple_of(n)` with the stable equivalent `% n == 0` in `src/tools/shell.rs`.

### Changes
1.  **File:** `src/tools/shell.rs`
    *   Replaced `if stderr_lines.len().is_multiple_of(50)` with `if stderr_lines.len() % 50 == 0`.
    *   Replaced `if (stdout_lines.len() + stderr_lines.len()).is_multiple_of(50)` with `if (stdout_lines.len() + stderr_lines.len()) % 50 == 0`.

## Verification
Ran `cargo check` locally, which passed successfully.
