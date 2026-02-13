# ADR 024: HuggingFace background download with progress

**Status:** Accepted
**Date:** 2026-02-17

## Context

Users download large model files (often 4–16 GB GGUF files) from HuggingFace. Downloading synchronously within an HTTP request would time out. The UI needs progress feedback to avoid the impression that the system has hung.

## Decision

The `/api/user/hf/download` endpoint spawns a background `tokio::spawn` task for the download. The task writes the file to the models directory, tracking bytes downloaded and estimated time remaining. A separate endpoint (`/api/user/hf/downloads`) returns the progress of active downloads. On completion, the model is automatically registered in the database.

## Consequences

- **Positive:** No HTTP timeout issues. UI can poll for progress and display a progress bar. Auto-registration removes the manual step of registering a downloaded model.
- **Negative:** Background tasks are lost on proxy restart — a partially downloaded file remains on disk without a corresponding active download. Users must re-trigger the download. Acceptable for the infrequent nature of model downloads.
