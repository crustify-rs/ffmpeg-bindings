"""Field-cast witnesses of each variant edge in casts-<x>.json: owner.field -> variant, with sites."""
import csv, json, sys, collections
csv.field_size_limit(1 << 30)
d = json.load(open(sys.argv[1]))
want = {(tuple(v["owner"]), tuple(v["variant"])) for v in d["variants"]}
own = {o for o, _ in want}
w = collections.defaultdict(list)
for r in csv.DictReader(open("crustify/wavefront/codeql/t2/field_casts.csv")):
    for a, b in (("src", "dst"), ("dst", "src")):
        if r[f"{a}_kind"] == "field" and r[f"{b}_kind"] == "type":
            o = (r[f"{a}_name"], r[f"{a}_def_file"]); t = (r[f"{b}_name"], r[f"{b}_def_file"])
            if (o, t) in want:
                w[(o, t)].append((r[f"{a}_member"], r["rel"], r["site_fn"], f'{r["site_file"]}:{r["line"]}'))
for k in sorted(want):
    sites = w.get(k, [])
    fields = collections.Counter(s[0] for s in sites)
    print(f"{k[0][0]}.{dict(fields)} -> {k[1][0]} ({k[1][1]}): {len(sites)} sites")
    for s in sites[: int(sys.argv[2]) if len(sys.argv) > 2 else 3]: print("     ", s)
