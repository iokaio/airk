---
id: management.logs
title: Logs and diagnostics
route: manage/logs
area: management
aliases: logs, diagnostic report, troubleshooting
related_airk_help: troubleshooting, privacy
---
# Logs and diagnostics

Management › Logs opens the bounded native service logs and builds a metadata-only diagnostic preview.

## Steps

1. Choose **Preview report**. Paths and user or machine names are redacted unless you include them.
2. Choose **Copy this preview** to copy exactly what is shown.

## Limits

Reports are never uploaded. Older or rotated log records may be unavailable.

## Log views

Recent service events appear in this tab, with observation status and explicit empty/disconnected states. **Refresh recent events** reloads the bounded service observation. **Open service logs** reads at most 32 native records and focuses the AIRK Service output channel. **Show extension output** focuses the separate AIRK output channel without reopening Explorer or requesting native service logs. Read failures retain diagnostic details in AIRK Service and report that channel as the next place to look.
