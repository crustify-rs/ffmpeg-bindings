import json, sys
for p in sys.argv[1:]:
    print("==", p)
    for w in json.load(open(p))["waves"]:
        for bi, b in enumerate(w["batches"]):
            print(f"  w{w['index']} b{bi} {b['items'][0]['kind']:8}", [i["name"] + (f"[{len(i['field_anchors'])}]" if i["kind"] == "type" else "") for i in b["items"]])
