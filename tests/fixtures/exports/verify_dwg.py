"""Read the DWG control set independently using GNU LibreDWG and ezdxf."""
from collections import Counter
from pathlib import Path
from tempfile import TemporaryDirectory
import argparse
import json
import subprocess

import ezdxf
from ezdxf import recover

HERE = Path(__file__).resolve().parent


def read(path):
    doc, auditor = recover.readfile(path)
    assert not auditor.has_errors, (path.name, auditor.errors)
    return doc


def layers(doc):
    return sorted(layer.dxf.name for layer in doc.layers)


def counts(doc):
    return Counter((entity.dxf.layer, entity.dxftype()) for entity in doc.modelspace())


def position(entity):
    point = entity.dxf.get("align_point", entity.dxf.insert)
    return [round(point.x, 3), round(point.y, 3)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dwg2dxf", default="dwg2dxf")
    args = parser.parse_args()
    version = subprocess.check_output([args.dwg2dxf, "--version"], text=True).splitlines()[0]
    summary = json.loads((HERE / "summary.json").read_text())
    results = {}
    with TemporaryDirectory() as directory:
        for name, info in sorted(summary.items()):
            converted = Path(directory) / f"{name}.dxf"
            dwg = HERE / f"{name}.dwg"
            assert dwg.read_bytes()[:6] == b"AC1018", name
            log = subprocess.run(
                [args.dwg2dxf, "--as", "r2004", "-o", str(converted), str(dwg)],
                check=True, capture_output=True, text=True,
            )
            assert not any(line.startswith("ERROR:") for line in log.stderr.splitlines()), (name, log.stderr)
            actual = read(converted)
            expected = read(HERE / f"{name}.dxf")
            assert actual.header["$INSUNITS"] == expected.header["$INSUNITS"] == 4, name
            assert layers(actual) == layers(expected), name
            assert counts(actual) == counts(expected), name
            size = list(actual.header["$EXTMAX"])[:2]
            assert size == info["size_mm"], (name, size)
            actual_frame = actual.modelspace().query("LINE[layer=='OB_FRAME']")
            expected_frame = expected.modelspace().query("LINE[layer=='OB_FRAME']")
            coords = lambda lines: sorted((tuple(e.dxf.start), tuple(e.dxf.end)) for e in lines)
            assert coords(actual_frame) == coords(expected_frame), name
            labels = list(actual.modelspace().query("TEXT"))
            sample = next(e.dxf.text for e in labels if "-КМ" in e.dxf.text)
            results[name] = {
                "sheet_mm": size,
                "entities": sum(counts(actual).values()),
                "cyrillic_sample": sample,
                "reader_diagnostics": log.stderr.splitlines(),
            }
            if name == "beam-a3":
                label = next(e for e in labels if e.dxf.text == "3000")
                expected_label = next(e for e in expected.modelspace().query("TEXT") if e.dxf.text == "3000")
                assert position(label) == position(expected_label)
                lines = actual.modelspace().query("LINE[layer=='OB_THIN']")
                assert any(abs(abs(e.dxf.start.x - e.dxf.end.x) - 300.) < 0.01
                           and abs(e.dxf.start.y - e.dxf.end.y) < 0.01 for e in lines)
                results[name]["dimension_3000_at_mm"] = position(label)
    print(json.dumps({"libredwg": version, "ezdxf": ezdxf.__version__, "sheets": results},
                     ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
