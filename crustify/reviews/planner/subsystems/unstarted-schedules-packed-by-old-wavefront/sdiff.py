"""Diff two schema-v2 schedules: counts, items added/dropped, wave moves, batch regroupings, anchor/home changes.
usage: sdiff.py A.json B.json [--brief]"""
import json, sys
def load(p):
    d = json.load(open(p)); pos = {}; batches = []
    for w in d["waves"]:
        for bi, b in enumerate(w["batches"]):
            keys = []
            for i in b["items"]:
                k = (i["name"], i["defined_in"]); pos[k] = (w["index"], bi, i); keys.append(k)
            batches.append((w["index"], frozenset(keys)))
    return d, pos, batches
a, pa, ba = load(sys.argv[1]); b, pb, bb = load(sys.argv[2])
cnt = lambda d: (len(d["waves"]), sum(len(w["batches"]) for w in d["waves"]), sum(len(x["items"]) for w in d["waves"] for x in w["batches"]))
print(f"  waves/batches/items {cnt(a)} -> {cnt(b)}")
add = sorted(set(pb) - set(pa)); drop = sorted(set(pa) - set(pb))
if add: print("  added:", [f"{k[0]} ({k[1]}) w{pb[k][0]}" for k in add])
if drop: print("  dropped:", [f"{k[0]} ({k[1]}) w{pa[k][0]}" for k in drop])
moves = [(k, pa[k][0], pb[k][0]) for k in sorted(set(pa) & set(pb)) if pa[k][0] != pb[k][0]]
if moves: print("  wave moves:", [f"{k[0]} w{x}->w{y}" for k, x, y in moves])
sa = {s for _, s in ba}; sb = {s for _, s in bb}
regroup = [s for s in sb if s not in sa]
if regroup: print(f"  batches not in A: {len(regroup)}; batches of A not in B: {len([s for s in sa if s not in sb])}")
for k in sorted(set(pa) & set(pb)):
    ia, ib = pa[k][2], pb[k][2]
    for f in ("kind", "home", "field_anchors"):
        if ia[f] != ib[f]:
            if f == "field_anchors" and set(ia[f]) == set(ib[f]): print(f"  {k[0]}: field_anchors reordered"); continue
            print(f"  {k[0]}: {f} {ia[f]} -> {ib[f]}")
