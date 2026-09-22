---
oxc-react-docgen-core: patch
---

Concurrent runs sharing one DTS cache directory — the CLI and the Vite plugin in the same project, or parallel builds — no longer report a spurious `IO_ERROR` "Failed to persist the DTS cache" diagnostic. Every writer used the same temp file name, so the second rename found it already moved. Temp names are now unique per process and write.
