#!/usr/bin/env python3
"""Resolve git conflict blocks by keeping both sides (ours first, then theirs).
Lines in the theirs side that also appear in ours are not duplicated."""
import sys, re, os
NODEDUP = os.environ.get("DEDUP") != "1"
for path in sys.argv[1:]:
    out = []
    state = None
    ours, theirs = [], []
    for line in open(path, encoding="utf-8").read().split("\n"):
        if line.startswith("<<<<<<< "):
            state = "ours"; ours, theirs = [], []
            continue
        if line.startswith("=======") and state == "ours":
            state = "theirs"
            continue
        if line.startswith(">>>>>>> ") and state == "theirs":
            out.extend(ours)
            out.extend(theirs if NODEDUP else [l for l in theirs if l.strip() == "" or l not in ours])
            state = None
            continue
        if state == "ours": ours.append(line)
        elif state == "theirs": theirs.append(line)
        else: out.append(line)
    open(path, "w", encoding="utf-8").write("\n".join(out))
