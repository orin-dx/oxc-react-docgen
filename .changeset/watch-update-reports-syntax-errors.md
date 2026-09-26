---
oxc-react-docgen-core: patch
---

`WatchSession::update_file` now reports a re-read file's syntax errors and other parse diagnostics, as a cold extraction does. An edit that broke a file used to produce no diagnostic at all, so `watch` showed nothing and its exit code stayed 0.
