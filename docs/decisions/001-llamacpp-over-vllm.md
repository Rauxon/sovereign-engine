# ADR 001: llama.cpp over vLLM

**Status:** Accepted
**Date:** 2026-02-13

## Context

Sovereign Engine initially supported both vLLM and llama.cpp as inference backends. The target hardware is an AMD Strix Halo APU (gfx1151) running Debian.

vLLM requires ROCm support for AMD GPUs. At the time of this decision:
- ROCm on Debian is not officially supported (Ubuntu-only packages)
- The gfx1151 architecture (Strix Halo) is not in the ROCm supported GPU list
- vLLM's ROCm Docker images did not include gfx1151 support
- Building ROCm from source for gfx1151 on Debian was unreliable

Testing with custom gfx1151-specific vLLM ROCm images showed poor performance: 0.6–2.4 tokens/s compared to 4.5 tokens/s from llama.cpp with Vulkan on the same hardware and workloads.

llama.cpp, by contrast:
- Supports ROCm via HIP with straightforward compilation
- Works on gfx1151 with the `AMDGPU_TARGETS=gfx1151` build flag
- Also supports NVIDIA CUDA and CPU-only operation
- Has a smaller resource footprint

## Decision

Remove vLLM as a backend option. Use llama.cpp exclusively for all inference.

The existing `docker/vllm.rs` module was removed. A data migration (`20260213000002_remove_vllm.sql`) converts any existing vLLM-type models to llama.cpp.

## Consequences

- **Positive:** Simpler codebase (one backend instead of two), reliable ROCm support on target hardware, smaller Docker images
- **Negative:** No vLLM-specific features (e.g. continuous batching, PagedAttention). These are not currently needed for the single-user/small-team deployment model
- **Migration:** Existing deployments with vLLM models automatically converted on schema migration
