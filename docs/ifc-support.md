# IFC support (phase 14)

Contract: **IFC4 ADD2 TC1** (`FILE_SCHEMA(('IFC4'))`), STEP physical file
(ISO 10303-21), length unit millimetre. IFC4.3 (IFC4X3) is tracked separately in
`docs/parity.csv` (row `ifc43`, open); IFC4X3 files are read as IFC4 with a warning
and 4.3-specific entities listed as unsupported. IFC2X3 is rejected.

## Supported subset (export and import)

| Model data | IFC |
|---|---|
| Beam / column members | `IfcBeam` (.BEAM.) / `IfcColumn` (.COLUMN.); `IfcMember` imported as beam |
| Plates | `IfcPlate` (.SHEET.) |
| Body | `IfcShapeRepresentation` 'Body' 'SweptSolid' → one `IfcExtrudedAreaSolid` along profile Z |
| Profiles | `IfcIShapeProfileDef`, `IfcLShapeProfileDef`, `IfcUShapeProfileDef`, `IfcRectangleProfileDef`, `IfcCircleHollowProfileDef`, `IfcRectangleHollowProfileDef`; plates `IfcArbitraryClosedProfileDef` + `IfcPolyline` (rectangle accepted on import) |
| Placement | `IfcLocalPlacement` chain + `IfcAxis2Placement3D` (2D profile position folded in) |
| Spatial structure | `IfcProject` → `IfcSite` (project origin) → `IfcBuilding` → `IfcBuildingStorey`, `IfcRelContainedInSpatialStructure` |
| Material | `IfcMaterial` + `IfcRelAssociatesMaterial`; density in `IfcMaterialProperties` 'Pset_MaterialCommon'.MassDensity (kg/m³ via `IfcDerivedUnit`) |
| GUID | `GlobalId`; stored on import in element property `IFC GUID`, reused on export; otherwise deterministic from the element ID |
| Properties | element properties ↔ `IfcPropertySet` 'Open BIM' of `IfcPropertySingleValue` (IfcLabel / IfcReal / IfcBoolean); other psets imported as `Pset.Name` keys |
| ГОСТ data | export only: `IfcPropertySet` 'Сведения по ГОСТ' per part with «Профиль», «Сортамент» (standard of the section; ГОСТ 19903-2015 for plates), «Сталь» and «Масса, кг» (`IfcMassMeasure`); skipped on import, since it is derived from the model |
| КСИ | an element property «КСИ» (code of the construction information classifier, entered by the user) becomes `IfcClassificationReference` under `IfcClassification` 'КСИ' via `IfcRelAssociatesClassification`; the code returns on import through the 'Open BIM' set. Open BIM ships no КСИ table: take codes from ksi.faufcc.ru |
| Holes, box cuts | `IfcOpeningElement` (circle / rectangle extrusion) + `IfcRelVoidsElement` |
| Units | `IfcSIUnit` mm (import also m, cm, dm, km, foot, inch) |

## Not in the subset (reported)

Export report lists bolt groups, welds, plane cuts, assemblies, connections,
grid axes and drawings (`ExportReport.skipped`). Import report lists every entity
type outside the subset with counts (`unsupported_entities`, e.g. `IfcWall`,
`IfcMechanicalFastener`, `IfcElementAssembly`) and per-object issues (Brep or
mapped bodies, non-Z extrusion, unsupported profile, `IfcBooleanClippingResult`
cuts ignored, missing material/density, invalid or duplicate GUID).

## Verification

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
