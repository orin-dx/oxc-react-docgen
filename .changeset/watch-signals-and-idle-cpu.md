---
oxc-react-docgen: patch
---

`watch` no longer spins a CPU core when stdin isn't a terminal, and ends on SIGINT, SIGTERM, SIGHUP and SIGQUIT instead of ignoring them, restoring the terminal and exiting with the session's exit code. Only Ctrl-C quits from the keyboard, not a bare `c`.
