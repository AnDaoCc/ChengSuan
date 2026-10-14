# ChengSuan engineering rules

- Keep `core` pure TypeScript. No React, Tauri, file system or database imports.
- Store money, quantities and exchange rates as decimal strings; never use native floating point for business arithmetic.
- Features consume public services and repository interfaces. SQL and file access belong to Rust/infrastructure.
- Resolve portable storage from the executable, never the working directory. Never fall back to AppData.
- Use SQLite consistent backups and transactional migrations. Never overwrite a database without confirmation.
- Exporters consume CalculationResult snapshots and never recalculate.
- Verify types, Vitest, Rust tests and a Windows no-bundle release. Report unverified environments honestly.
