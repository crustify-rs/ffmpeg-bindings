"""Project per-subsystem wavefront schedules (schema 3) into Crustify schema-v2 schedules.
Homes and anchors follow crustify/tmp/scripts/project_schedule.py, which projected the v1 plan.
usage: python3 project.py WFDIR OUTDIR    (writes OUTDIR/<lu>/<sub>/schedule.json)"""
import json, csv, os, sys, collections
csv.field_size_limit(1 << 30)
WF, OUT = sys.argv[1], sys.argv[2]
d = json.load(open("crustify/artifacts/subsystems.json"))
API = set(json.load(open("crustify/wavefront/configs/wavefront-config.json"))["api_headers"])
dep = json.load(open("crustify/wavefront/deprecated.json"))
dep_fields = {(x[1], x[2]) for x in dep["ff_api_gated"] + dep["attribute_deprecated"] if x[0] == "field"}
dep_items = {x[1] for x in dep["ff_api_gated"] + dep["attribute_deprecated"] if x[0] in ("symbol", "type")}
decl = collections.defaultdict(set)
for t, key in (("functions", "f"), ("globals", "f"), ("types", "t")):
    for r in csv.DictReader(open(f"crustify/wavefront/codeql/t1/{t}.csv")):
        for c in ("decl_files", "def_file"):
            for h in r[c].split("|"):
                if h in API: decl[(key, r["name"])].add(h)
sub_of = {h: (lu["name"], s["name"]) for lu in d["link_units"] for s in lu["subsystems"] for h in s.get("api_headers", [])}
home = lambda h: f"crustify/rust/ffmpeg/{h.split('/')[0]}/{os.path.splitext(os.path.basename(h))[0]}.rs"
def home_of(i, lu, sub):
    if i["defined_in"] in API: return home(i["defined_in"])
    hs = sorted(decl[("t" if i["kind"] == "type" else "f", i["name"])])
    stem = os.path.splitext(os.path.basename(i["defined_in"]))[0]
    hs.sort(key=lambda h: (sub_of[h] != (lu, sub), os.path.splitext(os.path.basename(h))[0] != stem, h))
    assert hs, i
    return home(hs[0])
def anchors(i):
    out = []
    for f in i["field_anchors"]:
        segs = [x for x in f.split(".") if x != "(unknown field)"]
        if not segs: continue
        top = segs[0]
        if (i["name"], f) not in dep_fields and (i["name"], top) not in dep_fields and top not in out:
            out.append(top)
    return out
def item(i, lu, sub):
    return {"name": i["name"], "defined_in": i["defined_in"], "kind": i["kind"],
            "field_anchors": anchors(i) if i["kind"] == "type" else [], "home": home_of(i, lu, sub)}
owned, dropped = {}, collections.Counter()
stats = {}
for lu in d["link_units"]:
    for s in lu["subsystems"]:
        name = s["name"]
        if name.startswith("raw-lifetime-"):
            t = name.split("-")[-1]
            waves = [{"index": 0, "batches": [{"objective": "wrap", "items": [
                {"name": t, "defined_in": None, "kind": "raw-lifetime", "field_anchors": [], "home": "crustify/rust/ffmpeg/libavutil/mem.rs"}]}]}]
        elif not s.get("api_headers"):
            continue
        else:
            wf = json.load(open(f"{WF}/{lu['name']}/{name}.json"))
            waves = []
            for w in wf["waves"]:
                batches = []
                for b in w["batches"]:
                    its = []
                    for i in b["items"]:
                        key = (i["name"], i["defined_in"])
                        if i["defined_in"] and i["defined_in"].startswith("/"): dropped["out-of-tree"] += 1; continue
                        if i["name"] in dep_items: dropped["deprecated"] += 1; continue
                        if key in owned: dropped["owned-earlier"] += 1; print("owned earlier", key, owned[key]); continue
                        owned[key] = f"{lu['name']}/{name}"; its.append(item(i, lu["name"], name))
                    if its: batches.append({"objective": "wrap", "items": its})
                if batches: waves.append({"index": len(waves), "batches": batches})
        p = f"{OUT}/{lu['name']}/{name}/schedule.json"
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as fh:
            json.dump({"schema_version": 2, "waves": waves}, fh, indent=2); fh.write("\n")
        n_b = sum(len(w["batches"]) for w in waves); n_i = sum(len(b["items"]) for w in waves for b in w["batches"])
        stats[f"{lu['name']}/{name}"] = (len(waves), n_b, n_i)
for k, v in stats.items(): print(f"{k:32} waves {v[0]:3} batches {v[1]:4} items {v[2]:5}")
print("total", [sum(v[i] for v in stats.values()) for i in range(3)], "dropped", dict(dropped))
