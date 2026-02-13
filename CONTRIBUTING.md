# Contributing to Sovereign Engine

## Getting Started

1. **Fork** the repository on GitHub.
2. **Clone** your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/SovereignEngine.git
   cd SovereignEngine
   ```
3. **Add the upstream remote:**
   ```bash
   git remote add upstream https://github.com/rauxon/SovereignEngine.git
   ```
4. **Set up your development environment** — see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md#first-time-setup) for first-time setup instructions.
5. **Create a branch** from `main`:
   ```bash
   git checkout -b feat/your-feature-name
   ```
6. **Make your changes**, ensuring tests pass (see [Testing](#testing) below).
7. **Commit** with a clear message (see [Commit Conventions](#commit-conventions)).
8. **Push** to your fork:
   ```bash
   git push origin feat/your-feature-name
   ```
9. **Open a Pull Request** against `main` on the upstream repository.

### Staying Up to Date

```bash
git fetch upstream
git rebase upstream/main
```

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Code Style

### Rust
- **Format:** `cargo fmt` (enforced)
- **Lint:** `cargo clippy -- -D warnings` (zero warnings policy)
- All public types should derive `Debug`
- Use `anyhow::Result` for fallible functions, `thiserror` for library-style error types

### TypeScript / React
- **Lint:** `npm run lint` (ESLint with React Compiler plugin)
- **Type check:** `npm run typecheck` (`tsc --noEmit`)
- Use TypeScript interfaces (not `any`) for all API response types
- Keep API types in `ui/src/types.ts` matching the contracts in `docs/API.md`

## Testing

Before submitting a PR, ensure:

```bash
# Rust
cd proxy && cargo test && cargo clippy -- -D warnings && cargo fmt --check

# React
cd ui && npm run lint && npm run typecheck
```

## Pull Request Process

1. **Branch naming:** `feat/description`, `fix/description`, or `docs/description`
2. **One change per PR** — keep PRs focused and reviewable
3. **Update docs** if your change affects the API, configuration, or architecture
4. **Update migrations** table in `docs/DEVELOPMENT.md` and `docs/ARCHITECTURE.md` if adding a migration
5. **Run the full test suite** before marking as ready for review

## Commit Conventions

- Use imperative mood: "Add feature" not "Added feature"
- First line: concise summary (50 chars ideal, 72 max)
- Body (optional): explain why, not what
- Reference issues where applicable: `Fixes #123`

## Architecture Decision Records

When making a significant design decision (new dependency, architectural pattern, trade-off), create an ADR:

1. Create `docs/decisions/NNN-short-title.md`
2. Use the format: Status / Date / Context / Decision / Consequences
3. Number sequentially from the last ADR in `docs/decisions/`
4. Link from `docs/ARCHITECTURE.md` if the decision affects the module map or system overview

See existing ADRs in `docs/decisions/` for examples.

## Questions?

Open an issue for discussion before starting large changes.
