"""Independent check of the keyboard-only scenario: the script has no pointer
steps, the journal shows the whole cycle, parts sit exactly on the typed grid
nodes / keyboard-cursor snap points, the saved file matches the journal and
the PDF opens."""
import json
from pathlib import Path

from pypdf import PdfReader

D = Path(__file__).resolve().parent
POINTER = {"click", "right_click", "hover", "drag", "middle_drag", "scroll",
           "click_world", "hover_world", "drag_world", "scroll_world"}
script = json.loads((D / "script.json").read_text())
pointer = [s for s in script if isinstance(s, dict) and POINTER & s.keys()]
assert not pointer, pointer
keys = [s for s in script if isinstance(s, dict) and ("key" in s or "text" in s)]
print(f"script: {len(script)} steps, {len(keys)} key/text, 0 pointer")

journal = json.loads((D / "journal.json").read_text())
actions = [e["action"] for e in journal["events"]]
want = ["Новый проект", "Создать: Колонна", "Создать: Колонна", "Создать: Балка",
        "Создать: Контурная плита", "Свойства", "Создать чертёж КМД-1",
        "Экспорт drawings/КМД-1.pdf", "Сохранить"]
assert actions == want, actions
print("journal:", " → ".join(actions))

project = journal["project"]
axes = {a["name"]: a for a in project["axes"].values()}
x = {n: a["start"][0] for n, a in axes.items() if a["start"][0] == a["end"][0]}
y = {n: a["start"][1] for n, a in axes.items() if a["start"][1] == a["end"][1]}
node = lambda letter, digit, z=0.0: [x[digit], y[letter], z]


def world(frame, p):
    o, ax = frame["origin"], frame["axes"]
    return [o[i] + sum(ax[k][i] * p[k] for k in range(3)) for i in range(3)]


els = list(project["elements"].values())
cols = [e for e in els if e["name"] == "Колонна"]
beam = next(e for e in els if e["name"] == "Балка")
plate = next(e for e in els if e["name"] == "Контурная плита")
starts = sorted(c["shape"]["frame"]["origin"] for c in cols)
assert starts == sorted([node("А", "1"), node("Б", "2")]), starts
s = beam["shape"]
b0, b1 = s["frame"]["origin"], world(s["frame"], [0, 0, s["length"]])
assert b0 == node("А", "1", 3600.0) and b1 == node("А", "3", 3600.0), (b0, b1)
corners = [world(plate["shape"]["frame"], [c[0], c[1], 0]) for c in plate["shape"]["contour"]]
assert corners == [node("В", "3"), node("В", "4"), node("Г", "4"), node("Г", "3")], corners
marked = [e["id"] for e in els if e.get("properties", {}).get("Марка", {}).get("value") == "К-1"]
assert len(marked) == 1, marked
print("columns at", starts, "| beam", b0, "→", b1, "| plate", corners, "| Марка К-1 on", marked)

saved = json.loads((D / "keyboard-only.obim").read_text())
assert saved["project"] == project, "saved project differs from the journal"
print("saved .obim revision", saved["project"]["revision"], "sha256 field", saved["payload_sha256"][:16])

pdf = PdfReader(D / "drawings" / "КМД-1.pdf")
text = "".join(p.extract_text() or "" for p in pdf.pages)
# Graph 1: set designation «<base>-КМД» (ГОСТ Р 21.101, 4.2.4).
assert len(pdf.pages) >= 1 and "-КМД" in text, text[:200]
print(f"pdf: {len(pdf.pages)} page(s), contains «-КМД»")
print("KEYBOARD_ONLY_OK")
