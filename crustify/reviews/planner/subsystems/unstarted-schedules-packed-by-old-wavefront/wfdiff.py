"""Wave moves and batch counts between two raw wavefront schedules. usage: wfdiff.py A.json B.json"""
import json, sys
def load(p):
    d = json.load(open(p)); pos = {}
    for wi, w in enumerate(d["waves"]):
        for b in w["batches"]:
            for i in b["items"]: pos[(i["name"], i["defined_in"])] = wi
    return d, pos
a, pa = load(sys.argv[1]); b, pb = load(sys.argv[2])
moves = [f"{k[0]} w{pa[k]}->w{pb[k]}" for k in sorted(set(pa) & set(pb)) if pa[k] != pb[k]]
print(f"  waves {len(a['waves'])}->{len(b['waves'])} batches {a['summary']['batch_count']}->{b['summary']['batch_count']} units {a['summary']['unit_count']}->{b['summary']['unit_count']}")
if moves: print("  moves:", moves)
