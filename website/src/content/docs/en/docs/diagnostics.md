---
title: "Errors & exit codes"
description: "Source positions, batch failures, and native exit statuses."
---

Diagnostics include the source, line, and column:

```text
interactive:1:6: undefined variable $missing
```

An ordinary nonzero command status stops the current script or submitted batch.
Conditions may inspect nonzero statuses, but missing programs and other startup
errors still fail. A pipeline reports its rightmost nonzero stage status, even
if the last stage succeeds (including upstream SIGPIPE). Every stage is reaped.
`exit` accepts a code from 0 to 255 and exits immediately, including from a
function or nested expression. Its default is 0.

An unexpected top-level closing delimiter (`)`, `]`, or `}`) is consumed and diagnosed, then reading continues. A mismatched delimiter inside an expression is an error; an unfinished expression reports its unclosed opening delimiter. Scripts exit with status 2 for reader errors unless a runtime failure or explicit exit supersedes them. Command startup failures use 127 for a missing program and 126 for permission denied. Other language errors use 2.

In the playground, a syntax error preserves the draft without executing the batch; fix the draft or clear it before submitting new input. A runtime error stops the current batch and a fresh input can still run. Unavailable commands produce a capability diagnostic. Use the reset button after a browser session exits.
