#!/usr/bin/env python3
"""Decompose the headline scalar from a genesis timeline RON dump.

Parses the regular pretty-RON emitted by `genesis run --timeline` (regex on
the fixed field layout — not a general RON parser) and reports, per run:
  - the argmax (structure, sample) of persistence x complexity with the
    full term split ln(size) + degree_entropy + ln(1 + mean_degree);
  - the max-complexity (structure, sample) with the same split;
  - the term trajectory of the eventual headline structure.
"""
import math
import re
import sys

SAMPLE_RE = re.compile(r"\(\s*tick: (\d+),\s*stats: \(", re.S)
STATS_RE = re.compile(
    r"tick: (\d+),\s*particles: (\d+),\s*bonds: (\d+),\s*components: (\d+),"
    r"\s*largest_component: (\d+)", re.S)
STRUCT_RE = re.compile(
    r"\(\s*id: (\d+),\s*size: (\d+),\s*persistence: (\d+),"
    r"\s*stability: ([\d.eE+-]+),\s*complexity: ([\d.eE+-]+),"
    r"\s*degree_entropy: ([\d.eE+-]+),\s*mean_degree: ([\d.eE+-]+),"
    r"\s*information: ([\d.eE+-]+),\s*\)", re.S)


def parse(path):
    text = open(path).read()
    # Split into samples on the top-level "(\n        tick:" boundary; each
    # sample owns the structures up to the next boundary.
    bounds = [m.start() for m in SAMPLE_RE.finditer(text)] + [len(text)]
    samples = []
    for a, b in zip(bounds, bounds[1:]):
        chunk = text[a:b]
        st = STATS_RE.search(chunk)
        rows = [dict(id=int(m[1]), size=int(m[2]), persistence=int(m[3]),
                     stability=float(m[4]), complexity=float(m[5]),
                     entropy=float(m[6]), mean_degree=float(m[7]),
                     information=float(m[8]))
                for m in STRUCT_RE.finditer(chunk)]
        samples.append(dict(tick=int(st[1]), particles=int(st[2]),
                            bonds=int(st[3]), components=int(st[4]),
                            largest=int(st[5]), rows=rows))
    return samples


def split(r):
    ln_s = math.log(r["size"])
    ln_d = math.log(1.0 + r["mean_degree"])
    return (f"size {r['size']:>6} -> ln {ln_s:6.3f} | H {r['entropy']:6.3f}"
            f" | mean_deg {r['mean_degree']:9.3f} -> ln(1+d) {ln_d:6.3f}"
            f" | C {r['complexity']:7.3f}")


def main(path):
    samples = parse(path)
    name = path.split("/")[-1].replace(".tl.ron", "")
    last = samples[-1]
    print(f"== {name}: {len(samples)} samples, final tick {last['tick']}, "
          f"particles {last['particles']}, bonds {last['bonds']}, "
          f"world mean_deg {2*last['bonds']/last['particles']:.2f}")

    best = max(((s, r) for s in samples for r in s["rows"]),
               key=lambda sr: sr[1]["persistence"] * sr[1]["complexity"],
               default=None)
    if best is None:
        print("   no structures ever observed")
        return
    s, r = best
    print(f"   headline {r['persistence'] * r['complexity']:9.3f} = "
          f"P {r['persistence']} x C {r['complexity']:.3f}  "
          f"@ tick {s['tick']} structure {r['id']}")
    print(f"   headline row: {split(r)}")

    cs, cr = max(((s, r) for s in samples for r in s["rows"]),
                 key=lambda sr: sr[1]["complexity"])
    print(f"   peak C row   (tick {cs['tick']}, id {cr['id']}, "
          f"P {cr['persistence']}): {split(cr)}")

    # Trajectory of the eventual headline structure, first/middle/last
    # samples where it exists.
    traj = [(s, row) for s in samples for row in s["rows"]
            if row["id"] == r["id"]]
    picks = sorted({0, len(traj) // 2, len(traj) - 1})
    print(f"   trajectory of structure {r['id']}:")
    for i in picks:
        ts, tr = traj[i]
        print(f"     tick {ts['tick']:>6}: {split(tr)}")


if __name__ == "__main__":
    for p in sys.argv[1:]:
        main(p)
        print()
