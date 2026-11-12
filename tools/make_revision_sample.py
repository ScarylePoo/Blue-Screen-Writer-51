"""Make a sample document with 30 revisions spread over three weeks.

usage: mkrevs.py HISTORY_ROOT OUT_WPS
Writes HISTORY_ROOT/<id>/0000NN-<stamp>-<kind>.wps (mtime = stamp) and the
final version as OUT_WPS (to be converted to .docx).
"""
import os, sys, secrets, datetime as dt

INIT = ("#init|10200|13200|1200|1200|1200|1200|Courier New|1200|r,-1200L,-600L,0L,600L,1200L,1800L,"
        "2400L,3000L,3600L,4200L,4800L,5400L,6000L,6600L,7200L,7800L,8400L,9000L,9600L,10200L,10800L,"
        "11400L,12000L,12600L,13200L,13800L,14400L,15000L,15600L,16200L,16800L,17400L,18000L|F|100")

# The document grows and is corrected step by step.  Each step is a list of
# paragraphs; "**x**" = bold paragraph.
P = {}
P["title"] = "**Translation: Supply Agreement Bianchi S.r.l. / Hartmann GmbH**"
P["note"] = "Translated from the Italian original dated 3 October 2026."
P["a1_0"] = "Article 1 - Subject of the agreement"
P["a1_1"] = "The Supplier undertakes to deliver to the Buyer the goods listed in Annex A."
P["a1_2"] = "The Supplier undertakes to deliver to the Buyer the goods listed in Annex A, in the quantities and at the prices stated there."
P["a2_0"] = "Article 2 - Delivery"
P["a2_1"] = "Delivery takes place free at the Buyers warehouse in Stuttgart."
P["a2_2"] = "Delivery takes place free at the Buyer's warehouse in Stuttgart within 30 days of the order."
P["a2_3"] = "Delivery shall take place free at the Buyer's warehouse in Stuttgart within thirty (30) days of receipt of the order."
P["a3_0"] = "Article 3 - Prices and payment"
P["a3_1"] = "Prices are net of VAT. Payment is due within 60 days."
P["a3_2"] = "Prices are net of VAT. Invoices are payable within sixty (60) days of the invoice date by bank transfer."
P["a3_3"] = "Prices are stated net of VAT. Invoices are payable within sixty (60) days of the invoice date by bank transfer to the account given on the invoice."
P["a4_0"] = "Article 4 - Warranty"
P["a4_1"] = "The Supplier warrants that the goods are free of defects for 24 months."
P["a4_2"] = "The Supplier warrants that the goods are free from defects in material and workmanship for a period of twenty-four (24) months from delivery."
P["a5_0"] = "Article 5 - Governing law and jurisdiction"
P["a5_1"] = "This agreement is governed by Italian law. The court of Milan has jurisdiction."
P["a5_2"] = "This agreement is governed by Italian law, excluding the UN Convention on Contracts for the International Sale of Goods (CISG). The court of Milan shall have exclusive jurisdiction."
P["a6_0"] = "Article 6 - Final provisions"
P["a6_1"] = "Amendments must be made in writing."
P["a6_2"] = "Amendments and additions to this agreement must be made in writing. Should any provision be invalid, the remaining provisions shall remain in effect."
P["a6_r"] = "Amendments and additions to this agreement must be made in wri"
P["sign"] = "Milan / Stuttgart, 3 October 2026"
P["tn"] = "Translator's note: \"foro competente\" has been rendered as \"court ... shall have exclusive jurisdiction\"."
P["cert"] = "I certify that this is a true and complete translation of the Italian original. - Maria"

S = []
def step(*keys, kind="save"):
    S.append((kind, list(keys)))

step("title")
step("title", "note", kind="autosave")
step("title", "note", "a1_0", "a1_1")
step("title", "note", "a1_0", "a1_1", "a2_0", "a2_1", kind="autosave")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_1")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_1", "a3_0", "a3_1", kind="autosave")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_2", "a3_0", "a3_1")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_2", "a3_0", "a3_1", "a4_0", kind="discarded")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_2", "a3_0", "a3_1", "a4_0", "a4_1")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_2", "a3_0", "a3_2", "a4_0", "a4_1", kind="autosave")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_2", "a3_0", "a3_2", "a4_0", "a4_1", "a5_0", "a5_1")
# client looked at it in Word and changed something
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_2", "a4_0", "a4_1", "a5_0", "a5_1", kind="outside")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_2", "a4_0", "a4_2", "a5_0", "a5_1")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_2", "a4_0", "a4_2", "a5_0", "a5_1", "a6_0", kind="autosave")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_2", "a4_0", "a4_2", "a5_0", "a5_1", "a6_0", "a6_1")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_2", "a4_0", "a4_2", "a5_0", "a5_1", "a6_0", "a6_1", "sign", kind="autosave")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_3", "a4_0", "a4_2", "a5_0", "a5_1", "a6_0", "a6_1", "sign")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_3", "a4_0", "a4_2", "a5_0", "a5_2", "a6_0", "a6_r", "sign", kind="recovered")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_3", "a4_0", "a4_2", "a5_0", "a5_2", "a6_0", "a6_1", "sign")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_3", "a4_0", "a4_2", "a5_0", "a5_2", "a6_0", "a6_2", "sign", kind="autosave")
step("title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_3", "a4_0", "a4_2", "a5_0", "a5_2", "a6_0", "a6_2", "sign", "tn")
FULL = ["title", "note", "a1_0", "a1_2", "a2_0", "a2_3", "a3_0", "a3_3", "a4_0", "a4_2", "a5_0", "a5_2", "a6_0", "a6_2", "sign", "tn", "cert"]
step(*FULL, kind="autosave")
# a few final small touches: each one changes a word so every revision differs
tweaks = [
    ("a1_2", "goods listed in Annex A", "goods listed in Annex A (Schedule of Goods)"),
    ("note", "dated 3 October 2026", "dated 3 October 2026 (4 pages)"),
    ("a4_2", "from delivery", "from the date of delivery"),
    ("a6_2", "remain in effect", "remain in full force and effect"),
    ("sign", "Milan / Stuttgart", "Milan and Stuttgart"),
    ("tn", "Translator's note", "Translator's note (1)"),
    ("cert", "true and complete", "true, accurate and complete"),
    ("a3_3", "bank transfer", "SEPA bank transfer"),
]
for i, (k, a, b) in enumerate(tweaks):
    P[k] = P[k].replace(a, b)
    step(*FULL, kind="autosave" if i % 3 == 1 else "save")
assert len(S) == 30, len(S)

def render(keys):
    out = []
    for n, k in enumerate(keys):
        t = P[k] if isinstance(k, str) else k
        if n:
            out += ["kHRt", "kHRt"]
        if k.endswith("_0"):  # article headings bold + underlined
            out += ["kA+0", "t" + t, "kA-0"]
        elif t.startswith("**"):
            out += ["kA+0", "t" + t.strip("*"), "kA-0"]
        else:
            out.append("t" + t)
    return out

hist_root, out_wps = sys.argv[1], sys.argv[2]
doc_id = secrets.token_hex(16)
d = os.path.join(hist_root, doc_id)
os.makedirs(d)
t = dt.datetime(2026, 10, 19, 9, 12, 5)
final = None
for i, (kind, keys) in enumerate(S):
    # snapshot of the texts at this step (P mutates during tweaks, so render now)
    pass
# Render progressively: P changes during tweaks, so rebuild texts per step.
# (Steps before the tweaks use the original texts.)
import copy
P_orig = copy.deepcopy(P)
for k, a, b in tweaks:
    P_orig[k] = P_orig[k].replace(b, a)
P_now = copy.deepcopy(P_orig)
tw = 0
first_tweak_step = 30 - len(tweaks)
for i, (kind, keys) in enumerate(S):
    if i >= first_tweak_step:
        k, a, b = tweaks[tw]; tw += 1
        P_now[k] = P_now[k].replace(a, b)
    P.clear(); P.update(P_now)
    body = "\n".join(render(keys))
    path_line = "#PATH C:\\Users\\Public\\Documents\\Revisions Sample.docx" if i else "#PATH "
    text = f"{path_line}\n#WPSTREAM 1\n{INIT}\n#id|{doc_id}\n#sum|||||\n{body}\n"
    stamp = t.strftime("%Y%m%d-%H%M%S")
    f = os.path.join(d, f"{i+1:06}-{stamp}-{kind}.wps")
    with open(f, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    epoch = t.timestamp()  # naive = local time (same zone as Windows)
    os.utime(f, (epoch, epoch))
    final = text.split("\n", 1)[1]
    # next save: same day a bit later, or the next working day
    if kind == "autosave":
        t += dt.timedelta(minutes=5)
    elif i % 3 == 2:
        t = (t + dt.timedelta(days=1 if t.weekday() < 4 else 3)).replace(hour=8 + i % 3, minute=(i * 7) % 60, second=i)
    else:
        t += dt.timedelta(minutes=37 + i, seconds=13 * i % 60)
with open(out_wps, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(final)
print(doc_id, t)
