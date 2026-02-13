# ADR 014: Model resolution chain

**Status:** Accepted
**Date:** 2026-02-17

## Context

API tokens can be scoped to a specific model or a category of models. The OpenAI API `model` field in request bodies can be a model ID, a HuggingFace repo name, or a category name. The system needs a consistent resolution strategy.

## Decision

Resolve models in priority order:

1. Token's `specific_model_id` (if set) — direct lookup, ignores request body
2. Token's `category_id` (if set) — use preferred model in category, or any loaded model in category
3. Request body `model` field — try as model ID, then HuggingFace repo (`hf_repo` column), then category name
4. Error: model not found

This chain is implemented in `scheduler/resolver.rs`.

## Consequences

- **Positive:** Token-level constraints enable fine-grained access control (e.g., "this key only works with Model X"). Category fallback provides flexibility when specific models are unavailable
- **Negative:** Resolution logic is implicit — users may be surprised that a token with `category_id` ignores the `model` field in the request body. Documented in API.md
