---
id: sysinfo
ids: [cpu, gpu, memory, temperature, netspeed]
kind: module
title: System readings
summary: CPU, GPU, memory, temperature and network throughput as bar chips.
status: stable
compositor: any
config: [gpu, temperature]
commands: []
deps: [drm, libnvidia-ml]
popout: true
see_also: [dashboard, network]
---

# System readings

Five module ids from one place — `cpu`, `gpu`, `memory`, `temperature` and `netspeed`. They are documented
together because they share their producers and their rules, not because they share a chip.

## What they show

| Id | Reading | Source |
| --- | --- | --- |
| `cpu` | utilisation | `/proc` |
| `memory` | used / total, and swap where there is one | `/proc` |
| `temperature` | a hwmon sensor of your choosing | sysfs |
| `gpu` | utilisation, VRAM and temperature | `/sys/class/drm` (AMD, Intel) or NVML (NVIDIA) |
| `netspeed` | live upload and download rates | the kernel's byte counters |

## Interacting

None of them is a control. Each has a popout card with the fuller reading — per-core figures, the sensor's
name, totals rather than percentages.

Clicking does nothing on purpose: a readout that acted on a press would be a surprise.

## Configuring

`[temperature]` — `sensor`, `unit`, `warn`, `critical`.
`[gpu]` — `enabled`, `backend`, `card`.

Poll intervals are `[dashboard] resource_update_interval`, shared with the dashboard's performance page.

## What they need

**Nothing to install.** CPU, memory, storage and temperature read `/proc` and sysfs directly — no
`lm-sensors`, no helper daemon.

GPU is the exception, and it is two backends because the kernel only tells half the story: AMD publishes
utilisation and VRAM into sysfs, so a reading costs four file reads and no process; NVIDIA publishes none of
that and answers only **NVML**, which the shell `dlopen`s rather than forking `nvidia-smi` per reading. Intel
sits in between — a temperature from hwmon and no utilisation counter outside the perf interface — and reports
what it has rather than inventing the rest.

Every field is optional as a result. **A card that cannot answer says unknown; it never reads zero.**

> On NixOS, graphics drivers live in `/run/opengl-driver/lib`, outside the loader's search path. The NVML
> loader carries that absolute path as a fallback, which is why the GPU card works there.

## Why one service, not five

CPU, memory, storage and temperature are polled — the kernel has no "usage changed" signal — so they are
deliberately a **single** service with one timer and one publish per tick. A bar with a CPU chip and a
dashboard with five cards cost one wakeup a second between them, which is the difference between a shell that
idles and one that keeps a core warm.

`netspeed` is kept out of that group so a bar showing only a Wi-Fi icon never starts a throughput poller it has
no use for.

## Related

- [Dashboard](dashboard.md) — the same readings as a page, with history.
- [network](network.md) — link state, which is a different question from throughput.
