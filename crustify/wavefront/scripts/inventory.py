"""Built TUs (from the plain build's objects) and the private headers they reach."""
import csv, os, json, collections
LIBS = ["libavutil","libswresample","libswscale","libavcodec","libavformat","libavfilter","libavdevice"]
B = "crustify/builds/plain"
cfg = json.load(open("crustify/wavefront/configs/wavefront-config.json"))
API = set(cfg["api_headers"])
EXCL = {p for p in cfg["out_of_scope"]["paths"] if p.endswith(".h")}

def tus():
    out = {}
    for l in LIBS:
        for dp, _, fs in os.walk(f"{B}/{l}"):
            for f in fs:
                if not f.endswith(".o"): continue
                rel = os.path.relpath(os.path.join(dp, f[:-2]), B)
                if "/tests/" in rel + "/": continue
                srcs = [rel + e for e in (".c", ".asm", ".S") if os.path.exists(rel + e)]
                if not srcs:  # generated source in the build tree
                    continue
                assert len(srcs) == 1, srcs
                out[srcs[0]] = l
    return out

def private_headers(tu_set):
    inc = collections.defaultdict(set)
    for r in csv.DictReader(open("crustify/wavefront/codeql/t1/includes.csv")):
        inc[r["source_file"]].add(r["included_file"])
    seen, stack = set(), [t for t in tu_set if t.endswith(".c")]
    while stack:
        f = stack.pop()
        for g in inc.get(f, ()):
            if g not in seen:
                seen.add(g); stack.append(g)
    return sorted(h for h in seen if h.split("/")[0] in LIBS and h.endswith(".h")
                  and "/tests/" not in h and h not in API and h not in EXCL)
