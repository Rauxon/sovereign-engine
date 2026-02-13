# ADR 006: GPU memory reporting tuned for Strix Halo

**Status:** Accepted
**Date:** 2026-02-14

## Context

The metrics system reports GPU memory usage via the SSE event stream. AMD GPUs expose memory information through sysfs (`/sys/class/drm/card*/device/mem_info_*`).

The AMD Strix Halo APU (gfx1151) is a unified memory architecture that reports GPU memory in two pools:
- **VRAM** (`mem_info_vram_total/used`) — dedicated GPU memory
- **GTT** (`mem_info_gtt_total/used`) — system memory accessible by the GPU

On Strix Halo, both pools contribute to the GPU's usable memory. The standard approach of reporting only VRAM would significantly undercount available memory, since most of the usable memory is in the GTT pool.

## Decision

Sum GTT and VRAM totals/usage when reporting GPU memory for AMD GPUs. This gives an accurate picture of total memory available to GPU workloads on unified-memory APUs like Strix Halo.

The summing logic is in `MetricsBroadcaster`'s sysfs reader.

## Consequences

- **Positive:** Accurate memory reporting on the target hardware (Strix Halo)
- **Negative:** May overcount "GPU memory" on discrete AMD GPUs where GTT is a staging area, not primary GPU memory. Acceptable trade-off for a single-deployment system, but should be revisited if supporting diverse AMD hardware.
