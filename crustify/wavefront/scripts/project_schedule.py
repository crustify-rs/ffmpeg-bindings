"""Project the per-sub-campaign wavefront schedules into the campaign schedule.json."""
import json, csv, os, collections
csv.field_size_limit(1 << 30)
CID = "20261005"
d = json.load(open("crustify/subsystems.json"))
API = set(json.load(open("crustify/wavefront/configs/wavefront-config.json"))["api_headers"])
dep = json.load(open("crustify/wavefront/deprecated.json"))
dep_fields = {(x[1], x[2]) for x in dep["ff_api_gated"] + dep["attribute_deprecated"] if x[0] == "field"}
decl = collections.defaultdict(set)
for t, key, cols in (("functions", "f", ("decl_files", "def_file")), ("globals", "f", ("decl_files", "def_file")),
                     ("types", "t", ("decl_files", "def_file"))):
    for r in csv.DictReader(open(f"crustify/wavefront/codeql/t1/{t}.csv")):
        for c in cols:
            for h in r[c].split("|"):
                if h in API: decl[(key, r["name"])].add(h)
sub_of = {h: (lu["name"], s["name"]) for lu in d["link_units"] for s in lu["subsystems"] for h in s["api_headers"]}
home = lambda h: f"crustify/rust/ffmpeg/{h.split('/')[0]}/{os.path.splitext(os.path.basename(h))[0]}.rs"
def home_of(i, lu, sub):
    if i["defined_in"] in API: return home(i["defined_in"])
    hs = sorted(decl[("t" if i["kind"] == "type" else "f", i["name"])])
    stem = os.path.splitext(os.path.basename(i["defined_in"]))[0]
    hs.sort(key=lambda h: (sub_of[h] != (lu, sub), os.path.splitext(os.path.basename(h))[0] != stem, h))
    assert hs, i
    return home(hs[0])
def anchors(i):
    """Batch anchors are C field identifiers: a wavefront member path through a named
    nested aggregate (`u.mask`) anchors on its top-level field (`u`). A C11 unnamed
    member adds no segment (`(unknown field).l1` is the direct field `l1`)."""
    out = []
    for f in i["field_anchors"]:
        segs = [x for x in f.split(".") if x != "(unknown field)"]
        if not segs: continue
        top = segs[0]
        if (i["name"], f) not in dep_fields and (i["name"], top) not in dep_fields and top not in out:
            out.append(top)
    return out
def item(i, lu, sub):
    fa = anchors(i) if i["kind"] == "type" else []
    return {"name": i["name"], "defined_in": i["defined_in"], "kind": i["kind"], "field_anchors": fa, "home": home_of(i, lu, sub)}

owned, dropped = set(), collections.Counter()
sched = {"schema_version": 1, "link_units": []}
for lu in d["link_units"]:
    subs = []
    if lu["name"] == "libavutil":
        for t in ("void", "string"):
            subs.append({"name": f"raw-lifetime-{t}", "waves": [{"batches": [{"objective": "wrap", "items": [
                {"name": t, "defined_in": None, "kind": "raw-lifetime", "field_anchors": [], "home": "crustify/rust/ffmpeg/libavutil/mem.rs"}]}]}]})
    for s in lu["subsystems"]:
        if not s["api_headers"]: continue
        wf = json.load(open(f"crustify/wavefront/schedules/{lu['name']}/{s['name']}.json"))
        waves = []
        for w in wf["waves"]:
            batches = []
            for b in w["batches"]:
                its = []
                for i in b["items"]:
                    key = (i["name"], i["defined_in"])
                    if i["defined_in"] and i["defined_in"].startswith("/"): dropped["out-of-tree"] += 1; continue
                    if key in owned: dropped["owned-earlier"] += 1; continue
                    owned.add(key); its.append(item(i, lu["name"], s["name"]))
                if its: batches.append({"objective": "wrap", "items": its})
            if batches: waves.append({"batches": batches})
        subs.append({"name": s["name"], "waves": waves})
    if subs:
        sched["link_units"].append({"name": lu["name"], "subsystems": subs})
for li, lu in enumerate(sched["link_units"]):
    lu_ordered = {"index": li, "name": lu["name"], "subsystems": []}
    for si, s in enumerate(lu["subsystems"]):
        lu_ordered["subsystems"].append({"index": si, "name": s["name"],
            "waves": [{"index": wi, "batches": w["batches"]} for wi, w in enumerate(s["waves"])]})
    sched["link_units"][li] = lu_ordered
os.makedirs(f"crustify/campaigns/{CID}", exist_ok=True)
json.dump(sched, open(f"crustify/campaigns/{CID}/schedule.json", "w"), indent=2)
open(f"crustify/campaigns/{CID}/schedule.json", "a").write("\n")
c = collections.Counter(); homes = set()
for lu in sched["link_units"]:
    for s in lu["subsystems"]:
        c["sub-campaigns"] += 1; c["waves"] += len(s["waves"])
        for w in s["waves"]:
            c["batches"] += len(w["batches"])
            for b in w["batches"]:
                assert len({x["kind"] if x["kind"] != "callback" else "symbol" for x in b["items"]}) == 1
                for x in b["items"]: c[x["kind"]] += 1; c["fields"] += len(x["field_anchors"]); homes.add(x["home"])
print(dict(c), "homes", len(homes), "dropped", dict(dropped))
