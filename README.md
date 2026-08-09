# Tidy

Tidy is a Tauri desktop application for safe, metadata-only Google Drive
cleanup. It uses React, TypeScript, Rust, and SQLite.

## Development

```bash
npm install
npm run dev
```

Run the desktop application with:

```bash
npm run tauri dev
```

Validate the frontend and Rust backend with:

```bash
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```
