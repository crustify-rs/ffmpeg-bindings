"""Where a public type's C lives: for each named type, its owner in a modules.json, how often the
owner's own files (TU and internal headers) name the type and access its fields, and the in-scope
TUs that access its fields most. Reads the CodeQL T2 tables (`wavefront c --config
../crustify/wavefront/configs/wavefront-config.json extract-ql` regenerates them).

usage: python3 type_homes.py <modules.json> TYPE...   (run from the repository root)"""
import csv, collections, json, re, sys
csv.field_size_limit(1 << 30)
mj, names = sys.argv[1], sys.argv[2:]
M = json.load(open(mj))
files, owner = {}, {}
for lu in M["link_units"]:
    for m in lu["modules"]:
        fs = ([m["translation_unit"]["path"]] if m["translation_unit"] else []) + [h["path"] for h in m["internal_headers"]]
        files[m["name"]] = fs
        for h in m["api_headers"]:
            for n in names:
                inc, exc = h["items"]["included"], h["items"]["excluded"]
                if n in inc or (not inc and n not in exc and re.search(rf"\}}\s*{n}\s*;|struct\s+{n}\s*\{{", open("c/" + h["path"], errors="replace").read())):
                    owner.setdefault(n, m["name"])
scope = {f for fs in files.values() for f in fs}
fa = collections.defaultdict(collections.Counter)
for r in csv.DictReader(open("crustify/wavefront/codeql/t2/field_accesses.csv")):
    if r["struct_name"] in names and r["access_file"] in scope:
        fa[r["struct_name"]][r["access_file"]] += 1
for n in names:
    o = owner.get(n)
    pat = re.compile(rf"\b{n}\b")
    mentions = sum(len(pat.findall(open("c/" + f, errors="replace").read())) for f in files.get(o, []))
    own_fa = sum(fa[n][f] for f in files.get(o, []))
    print(f"{n}: owner {o}; owner files name it {mentions}x, access its fields {own_fa}x; "
          f"field accesses by in-scope file: {fa[n].most_common(4)}")
