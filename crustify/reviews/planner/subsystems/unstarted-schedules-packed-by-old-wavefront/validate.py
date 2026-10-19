"""Validate the final plan: item schema, routes, uniqueness, and cross-subsystem dependency order."""
import json, collections, sys
S = sys.argv[1] if len(sys.argv) > 1 else "crustify/tmp/planner-run-1"
ss = json.load(open("crustify/artifacts/subsystems.json"))
order, own = [], {}
for lu in ss["link_units"]:
    for s in lu["subsystems"]:
        order.append(f'{lu["name"]}/{s["name"]}')
        for f in s.get("implementation_files", []) + s.get("api_headers", []):
            own.setdefault(f, f'{lu["name"]}/{s["name"]}')   # the sharer listed first is scheduled first
pos = {k: i for i, k in enumerate(order)}
ROUTE = {"type": "type", "symbol": "symbol", "callback": "symbol", "raw-lifetime": "raw-lifetime"}
bad = 0; cuts = collections.Counter(); seen = {}
for j in order:
    try: sch = json.load(open(f"{S}/plan-v1/{j}/schedule.json"))
    except FileNotFoundError: continue
    assert sch["schema_version"] == 2
    for wi, w in enumerate(sch["waves"]):
        assert w["index"] == wi and w["batches"]
        for b in w["batches"]:
            assert set(b) == {"objective", "items"} and b["objective"] == "wrap" and b["items"]
            assert len({ROUTE[i["kind"]] for i in b["items"]}) == 1
            keys = [(i["name"], i["defined_in"]) for i in b["items"]]; assert len(keys) == len(set(keys))
            for i in b["items"]:
                assert set(i) == {"name", "defined_in", "kind", "field_anchors", "home"}, i
                assert i["home"].startswith("crustify/rust/ffmpeg/") and i["home"].endswith(".rs")
                assert len(i["field_anchors"]) == len(set(i["field_anchors"]))
                assert i["kind"] == "type" or not i["field_anchors"]
                k = (i["name"], i["defined_in"]); assert k not in seen, (k, seen.get(k), j); seen[k] = j
    if j.split("/")[1].startswith("raw-lifetime"): continue
    wf = json.load(open(f"{S}/wf-v1/{j}.json"))
    for w in wf["waves"]:
        for b in w["batches"]:
            for i in b["items"]:
                for d in i["deps"]["types"] + i["deps"]["symbols"]:
                    o = own.get(d["defined_in"])
                    if o and o != j and pos[o] > pos[j]:
                        cuts[(j, o)] += 1
print("items", len(seen))
print("cross-subsystem deps on a later subsystem (implied cuts):")
for (a, b), n in sorted(cuts.items()): print(f"  {a} -> {b}: {n}")
