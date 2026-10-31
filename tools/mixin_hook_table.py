#!/usr/bin/env python3
"""The vanilla methods that map to exactly one Rust function: where a Java mod's mixin could bind.

A mixin injects into one vanilla method (`@Inject(method = "hurtServer", at = @At("HEAD"))`). Rusty
Minecraft has no vanilla bytecode to weave into, so an injection runs only where the Rust port of that
method calls a hook point (docs/java-mod-runtime.md). That needs one Rust function that *is* the method:
a method ported across two functions, or folded into a bigger one, has no single start and end.

The table is built from the port ledger (docs/port-ledger.md):

- with a full build (`.local/port-ledger/ledger.json`, made from the 26.3 jar): its ported methods
  (rule 1, the house-style citation) and the verdict map's `port` rows, each checked against the jar;
- without one (no vanilla jar on this machine): the same two sources read from the Rust code alone, with
  the ledger's own parser. Citations are then not checked against the jar, and the header says so.

It holds names only: `Class.method`, the Rust function (`file::Impl::name`), and how it was matched. No
vanilla code. `tools/check_public_tree.py` accepts it.

usage: mixin_hook_table.py [--out FILE] [--check] [--ledger FILE] [--stats]
"""
import argparse
import json
import re
import sys
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import port_ledger as pl  # noqa: E402

ROOT = pl.ROOT
OUT = ROOT / "crates" / "horizon-port-pack" / "data" / "vanilla-one-to-one.tsv"
# The ledger's rule 1: a doc whose first line opens with a capital; every `Class.method` in that line.
LEAD = re.compile(r"^\s*///\s*`?[A-Z]")
CITE = re.compile(r"(?<![\w.])([A-Z][a-z][A-Za-z0-9]*(?:\.[A-Z][A-Za-z0-9]*)*)(?:\.|::|#)([a-z][A-Za-z0-9_]*)\b")
NOT_JAVA = {"Self", "Some", "None", "Ok", "Err", "Vec", "Option", "Result", "String", "Box", "Arc", "Rc", "HashMap",
            "HashSet", "BTreeMap", "BTreeSet", "Path", "PathBuf", "Instant", "Duration"}
HEADER = """\
# Vanilla methods that map to exactly one Rust function (tools/mixin_hook_table.py; do not edit).
# A Java mod's mixin can bind only to these (docs/java-mod-runtime.md). Names only, no vanilla code.
# source: {source}
# vanilla\trust\tmatch
"""


def fn_ref(fn):
    owner = f"{fn['owner']}::" if fn.get("owner") else ""
    return f"{fn['file']}::{owner}{fn['name']}"


def from_ledger(path):
    led = json.loads(Path(path).read_text())
    fns = led["fns"]
    ports = defaultdict(set)
    how = {}
    for k, ids in led["ported"].items():
        for i in ids:
            if not fns[i].get("test"):
                ports[k].add(fn_ref(fns[i]))
        how[k] = "cited"
    for k, m in led.get("mapped", {}).items():
        if m.get("verdict") == "port" and k not in ports:
            ports[k].add(fn_ref(fns[m["fn"]]))
            how[k] = "mapped"
    rows = {}
    for k, refs in ports.items():
        cls, name = k.split("#", 1)
        rows[f"{cls.rsplit('/', 1)[-1].replace('$', '.')}.{name}"] = (refs, how[k])
    return rows, f"port ledger built from {', '.join(Path(j).name for j in led.get('jars', [])) or 'sources'}"


def lead_cites(path):
    """(vanilla, fn ref) for each `Class.method` the first line of a non-test fn's doc cites."""
    parsed = pl.parse_rust(path)
    out = []
    for fn in parsed["fns"]:
        if fn["test"]:
            continue
        doc = [t for ln, t in fn.get("comments", []) if ln < fn["line"] and t.strip().startswith("///")]
        if not doc or not LEAD.match(doc[0]):
            continue
        for m in CITE.finditer(doc[0]):
            if m.group(1).split(".")[0] not in NOT_JAVA:
                out.append((f"{m.group(1)}.{m.group(2)}", fn_ref(fn)))
    return out


def from_rust():
    ports = defaultdict(set)
    how = {}
    with ProcessPoolExecutor() as pool:
        for cites in pool.map(lead_cites, pl.rust_files(), chunksize=8):
            for vanilla, ref in cites:
                ports[vanilla].add(ref)
                how[vanilla] = "cited"
    for row in pl.read_port_map():
        vanilla, verdict, where = row["vanilla"], row["verdict"], row["where"]
        if verdict == "port" and where and "." in vanilla and vanilla not in ports:
            ports[vanilla].add(where)
            how[vanilla] = "mapped"
    rows = {k: (v, how[k]) for k, v in ports.items()}
    return rows, "Rust citations and tools/port_map only (no vanilla jar here: names not checked against 26.3)"


def table(rows, source):
    lines = [HEADER.format(source=source)]
    for vanilla in sorted(rows):
        refs, how = rows[vanilla]
        if len(refs) == 1:
            lines.append(f"{vanilla}\t{next(iter(refs))}\t{how}\n")
    return "".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", default=str(OUT))
    ap.add_argument("--ledger", default=str(pl.LEDGER), help="a full ledger build, used when it exists")
    ap.add_argument("--check", action="store_true", help="fail when the committed table is out of date")
    ap.add_argument("--stats", action="store_true", help="say how many methods map to one, two or more functions")
    args = ap.parse_args()
    rows, source = from_ledger(args.ledger) if Path(args.ledger).is_file() else from_rust()
    text = table(rows, source)
    if args.stats:
        counts = defaultdict(int)
        for refs, _ in rows.values():
            counts[min(len(refs), 3)] += 1
        total = sum(counts.values())
        print(f"{total} vanilla methods with a port: {counts[1]} map to exactly one Rust function "
              f"({100 * counts[1] / max(1, total):.0f}%), {counts[2]} to two, {counts[3]} to three or more. "
              f"Source: {source}.")
    out = Path(args.out)
    if args.check:
        if not out.is_file() or out.read_text() != text:
            sys.exit(f"{out} is out of date: run python3 tools/mixin_hook_table.py")
        return
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text)
    print(f"{sum(1 for line in text.splitlines() if line and not line.startswith('#'))} rows -> {out}")


if __name__ == "__main__":
    main()
