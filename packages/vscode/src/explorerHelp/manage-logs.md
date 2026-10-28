---
id: management.logs
title: Logs and diagnostics
route: manage/logs
area: management
aliases: logs, diagnostic report, troubleshooting
related_airk_help: troubleshooting, privacy
---
# Logs and diagnostics

Open this section from the tabs across the top of Management. Everyday controls remain in the Indexing tab: Search mode, Updates and Evidence detail.

Management › Logs opens recent native service events, builds a metadata-only diagnostic report and exports the retained service log ring.

## Steps

1. Choose **Generate report**. Paths and user or machine names are redacted unless you include them.
2. Choose **Open report**, **Copy report** or **Save report…** to open, copy or save that exact captured report.
3. For the full available native log history, choose **Save retained service logs…**. This exports the finite retained log ring separately from the report and recent-events display.

## Limits

Reports are never uploaded. They include the visible project page and up to 128 cached native events within a 256 KiB bound; omissions are explicit. The native log ring contains at most four 1 MiB files. Older rotated-away records are unavailable, and malformed records are counted and omitted.

## Log views

Recent service events appear in this tab, with observation status and explicit empty/disconnected states. **Refresh recent events** reloads the bounded service observation. **Open service logs** reads at most 32 native records and focuses the AIRK Service output channel. **Show extension output** focuses the separate AIRK output channel without reopening Explorer or requesting native service logs. Read failures retain diagnostic details in AIRK Service and report that channel as the next place to look.
