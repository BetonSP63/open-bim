# IFC support (phase 14)

Contract: **IFC4 ADD2 TC1** (`FILE_SCHEMA(('IFC4'))`), STEP physical file
(ISO 10303-21), length unit millimetre. IFC4.3 (IFC4X3) is tracked separately in
`docs/parity.csv` (row `ifc43`, open); IFC4X3 files are read as IFC4 with a warning
and 4.3-specific entities listed as unsupported. Import also reads **IFC2X3**
(files of other programs); older schemas are rejected. User guide:
[help/model-import.md](help/model-import.md).

## Supported subset (export and import)

| Model data | IFC |
|---|---|
| Beam / column members | `IfcBeam` (.BEAM.) / `IfcColumn` (.COLUMN.); `IfcMember` imported as beam; `*StandardCase` read as the base entity |
| Plates | `IfcPlate` (.SHEET.) |
| Body | `IfcShapeRepresentation` 'Body' 'SweptSolid' → one `IfcExtrudedAreaSolid` along profile Z |
| Profiles | `IfcIShapeProfileDef`, `IfcLShapeProfileDef`, `IfcUShapeProfileDef`, `IfcRectangleProfileDef`, `IfcCircleHollowProfileDef`, `IfcRectangleHollowProfileDef`; plates `IfcArbitraryClosedProfileDef` + `IfcPolyline` (rectangle accepted on import) |
| Shared geometry (import) | `IfcMappedItem` → `IfcRepresentationMap` + `IfcCartesianTransformationOperator3D` (unit scale) |
| Outline sections (import) | member bodies of `IfcArbitraryClosedProfileDef` / `IfcArbitraryProfileDefWithVoids` over `IfcPolyline`, `IfcIndexedPolyCurve`, `IfcCompositeCurve` (lines, `IfcTrimmedCurve` arcs by cartesian trims): matched to a ГОСТ section by bounding box in 8 orientations, area (6 %) and centroid side; else a rectangle; else kept as a plate-shaped prism with an issue |
| ГОСТ catalogue (import) | parametric sections replaced by the catalogue section with the same dimensions (0.6 mm) or the same designation (loose 8 % check); designation spellings of other programs are normalised (`crates/ifc/src/mapping.rs`). Report: `profiles`, `unmatched_profiles` |
| Placement | `IfcLocalPlacement` chain + `IfcAxis2Placement3D` (2D profile position folded in) |
| Spatial structure | `IfcProject` → `IfcSite` (project origin) → `IfcBuilding` → `IfcBuildingStorey`, `IfcRelContainedInSpatialStructure` |
| Material | `IfcMaterial` + `IfcRelAssociatesMaterial` (import also through `IfcMaterialList`, layer / profile / constituent sets and the element type via `IfcRelDefinesByType`; a ГОСТ grade in the name becomes the catalogue steel, report `materials`; IFC2x3 `IfcGeneralMaterialProperties`.MassDensity; without density: by name, steel 7850, concrete 2500, timber 500); density in `IfcMaterialProperties` 'Pset_MaterialCommon'.MassDensity (kg/m³ via `IfcDerivedUnit`) |
| GUID | `GlobalId`; stored on import in element property `IFC GUID`, reused on export; otherwise deterministic from the element ID |
| Properties | element properties ↔ `IfcPropertySet` 'Open BIM' of `IfcPropertySingleValue` (IfcLabel / IfcReal / IfcBoolean); other psets imported as `Pset.Name` keys |
| ГОСТ data | export only: `IfcPropertySet` 'Сведения по ГОСТ' per part with «Профиль», «Сортамент» (standard of the section; ГОСТ 19903-2015 for plates), «Сталь» and «Масса, кг» (`IfcMassMeasure`); skipped on import, since it is derived from the model |
| КСИ | an element property «КСИ» (code of the construction information classifier, entered by the user) becomes `IfcClassificationReference` under `IfcClassification` 'КСИ' via `IfcRelAssociatesClassification`; the code returns on import through the 'Open BIM' set. Open BIM ships no КСИ table: take codes from ksi.faufcc.ru |
| Assemblies (import) | `IfcElementAssembly` + `IfcRelAggregates` → model assemblies, longest part main, nesting kept |
| Holes, box cuts | `IfcOpeningElement` (circle / rectangle extrusion) + `IfcRelVoidsElement` |
| Units | `IfcSIUnit` mm (import also m, cm, dm, km, foot, inch) |

## Not in the subset (reported)

Export report lists bolt groups, welds, plane cuts, assemblies, connections,
grid axes and drawings (`ExportReport.skipped`). Import report lists every entity
type outside the subset with counts (`unsupported_entities`, e.g. `IfcWall`,
`IfcMechanicalFastener`, `IfcElementAssembly`) and per-object issues (Brep or
mapped bodies, non-Z extrusion, unsupported profile, `IfcBooleanClippingResult`
cuts ignored, missing material/density, invalid or duplicate GUID).

Import into an open model (`crates/ifc/src/merge.rs`): new ids after the
project's, materials and sections reused by name, a model farther than 1 km
from the origin shifted by whole metres; the desktop commits it as one undoable
transaction. Elements the model rejects are dropped one by one (`skipped`).

## Verification

* `crates/ifc/tests/foreign.rs`: an IFC2x3 file in metres in the style of
  other programs (foreign designation, outline angle, mapped column, material
  list, type material, assembly, wall) lands in the ГОСТ catalogue; bad parts
  are skipped without failing the file.
* `crates/ifc/tests/roundtrip.rs`: three models export → import → re-export; GUID,
  units, placement (0.01 mm, axes 1e-9), profile dimensions, lengths, contours,
  holes, properties and net masses (1e-6 kg) preserved; foreign entities reported.
* `tests/fixtures/ifc/verify.py` — IfcOpenShell 0.8.4 (independent): schema +
  EXPRESS rule validation with zero statements (negative control produces 10),
  unit scale 0.001 m, geometry kernel volumes equal phase-9 independent
  references for 12 drilled parts (relative 2e-3); `render.py` renders the files
  with IfcOpenShell tessellation (`*-ifcopenshell.png`).
* GUI: `tests/ifc_flow/` — export, new project, import in the native app; the
  exported file is byte-identical to the validated fixture.

The importer is bounded (512 MiB, 5 M entities, nesting 32, strings 1 MiB) and
returns errors instead of panicking.
