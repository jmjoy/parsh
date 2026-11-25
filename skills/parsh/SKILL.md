---
name: parsh
description: Use when writing, reviewing, debugging, or running parsh scripts or commands. Load guidance from the installed parsh shell.
---

# parsh

Use the `parsh` binary on PATH to find guidance for the version you will use.
Start by listing its available topics.

```console
parsh skills list
```

Choose a topic from that list and read its Markdown guide before writing,
reviewing, or running parsh source. Load extra topics only when the task calls
for them.

```console
parsh skills get TOPIC
parsh skills get TOPIC --full
```

Replace `TOPIC` with the name from the list. Add `--full` when you need the
topic's supplementary material.
