"""Independent CSV reader check (Python csv module, RFC 4180 dialect)."""
import csv, json, sys
from pathlib import Path

here = Path(__file__).parent
expected = json.loads((here / "expected.json").read_text())
def rows(name):
    with open(here / name, newline="", encoding="utf-8-sig") as f:
        return list(csv.reader(f))
parts = rows("parts.csv")
assert parts[0][0] == "Позиция" and parts[-1][0] == "Итого"
body = parts[1:-1]
assert sum(int(r[4]) for r in body) == expected["parts"]
total = sum(float(r[7]) for r in body)
assert abs(float(parts[-1][7]) - expected["total_mass_kg"]) < 1e-3
assert abs(total - expected["total_mass_kg"]) < 5e-3
assemblies = rows("assemblies.csv")
assert abs(float(assemblies[-1][6]) - expected["total_mass_kg"]) < 1e-3
fasteners = rows("fasteners.csv")
assert sum(int(r[2]) for r in fasteners[1:] if r[0] == "Болт") == expected["bolts"]
escaped = rows("escaping.csv")
names = [r[1] for r in escaped]
assert 'Колонна "К-1",\nось А' in names, names
print(json.dumps({"parts_rows": len(body), "parts_total_kg": float(parts[-1][7]),
                  "assembly_rows": len(assemblies) - 2, "bolts": expected["bolts"],
                  "escaped_name_roundtrip": True}, ensure_ascii=False))
sys.exit(0)
