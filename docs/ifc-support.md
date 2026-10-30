# IFC support (phase 14)

Contract: **IFC4 ADD2 TC1** (`FILE_SCHEMA(('IFC4'))`) and **IFC4.3 ADD2**
(`FILE_SCHEMA(('IFC4X3_ADD2'))`, ISO 16739-1:2024), STEP physical file
(ISO 10303-21), length unit millimetre. Import also reads **IFC2X3** (files of
other programs); older schemas are rejected, the drafts IFC4X1/IFC4X2 are read
as IFC4.3 with a warning.

## IFC4.3

* «Экспорт IFC4.3» (ribbon «Обмен», file menu, command line; batch API step
  `export_ifc43`) writes `<project>-IFC4X3.ifc` next to the IFC4 file
  (`open_bim_ifc::export_schema(project, Schema::Ifc4x3)`). The subset is the
  same; IFC4.3 adds the optional `IfcDerivedUnit.Name` and
  `IfcCartesianPointList3D.TagList` (written as `$`). Header view definition:
  `notYetAssigned` (IFC4.3 has no design transfer view).
* Predefined types new in IFC4.3 can be chosen on a part («Подтип», marked
  «(IFC4.3)»): beams `GIRDER_SEGMENT`, `PIERCAP`, `EDGEBEAM`, …; columns
  `PIERSTEM`, …; members `TIEBAR`, `STIFFENING_RIB`, cables, …; plates
  `BASE_PLATE`, `GUSSET_PLATE`, `STIFFENER_PLATE`, `SPLICE_PLATE`, `FLANGE_PLATE`,
  `WEB_PLATE`, `COVER_PLATE`; railings `FENCE`; slabs `TRACKSLAB`, `PAVING`, …;
  and the class `IfcBearing` (`POT`, `ELASTOMERIC`, …). An IFC4 export writes
  them as `.USERDEFINED.` with the type name as ObjectType (IfcBearing as
  `IfcBuildingElementProxy`) and lists the parts in `ExportReport.downgraded`;
  import turns such a USERDEFINED name back into the IFC4.3 type.
* Import keeps a predefined type other than the default (`BEAM`, `COLUMN`,
  `SHEET`, `NOTDEFINED`) and a USERDEFINED ObjectType in the part properties
  `ifc_predefined_type` / `ifc_object_type`, so they survive re-export.
* IFC4.3 facilities (`IfcBridge`, `IfcRoad`, `IfcRailway`, `IfcMarineFacility`,
  `IfcFacility`) and their parts are read like a building: elements contained
  in them are imported with the full placement chain. Infrastructure objects
  outside the subset are listed in words (`IfcBearing` «опорные части»,
  `IfcAlignment` «трассы», courses and kerbs «дорожная одежда», earthworks,
  track elements). Placements along an alignment (`IfcLinearPlacement`) are
  not supported: such a part is skipped with an issue. User guide:
[help/model-import.md](help/model-import.md).

## Supported subset (export and import)

| Model data | IFC |
|---|---|
| Beam / column members | `IfcBeam` (.BEAM.) / `IfcColumn` (.COLUMN.); `IfcMember` imported as beam (keeps `ifc_entity = IfcMember`); `*StandardCase` read as the base entity |
| Chosen IFC class | Part properties `ifc_entity`, `ifc_predefined_type`, `ifc_object_type` (property pane «Экспорт в IFC») export the part as IfcBeam, IfcColumn, IfcMember, IfcPlate, IfcRailing, IfcFooting, IfcPile, IfcSlab or IfcBuildingElementProxy; a predefined type not allowed for the class becomes NOTDEFINED; USERDEFINED writes ObjectType. Import reads back only beam, column, member and plate |
| Plates | `IfcPlate` (.SHEET.) |
| Body | `IfcShapeRepresentation` 'Body' 'SweptSolid' → one `IfcExtrudedAreaSolid` along profile Z |
| Profiles | `IfcIShapeProfileDef`, `IfcLShapeProfileDef`, `IfcUShapeProfileDef`, `IfcRectangleProfileDef`, `IfcCircleHollowProfileDef`, `IfcRectangleHollowProfileDef`; plates `IfcArbitraryClosedProfileDef` + `IfcPolyline` (rectangle accepted on import) |
| Shared geometry (import) | `IfcMappedItem` → `IfcRepresentationMap` + `IfcCartesianTransformationOperator3D` (unit scale) |
| Outline sections (import) | member bodies of `IfcArbitraryClosedProfileDef` / `IfcArbitraryProfileDefWithVoids` over `IfcPolyline`, `IfcIndexedPolyCurve`, `IfcCompositeCurve` (lines, `IfcTrimmedCurve` arcs by cartesian trims): matched to a ГОСТ section by bounding box in 8 orientations, area (6 %) and centroid side; else a rectangle; else kept as a plate-shaped prism with an issue |
| ГОСТ catalogue (import) | parametric sections replaced by the catalogue section with the same dimensions (0.6 mm) or the same designation (loose 8 % check); designation spellings of other programs are normalised (`crates/ifc/src/mapping.rs`). Report: `profiles`, `unmatched_profiles` |
| Placement | `IfcLocalPlacement` chain + `IfcAxis2Placement3D` (2D profile position folded in) |
| Spatial structure | `IfcProject` → `IfcSite` (project origin) → `IfcBuilding` → `IfcBuildingStorey`, `IfcRelContainedInSpatialStructure`. Site and building names and the storeys come from «Иерархия здания»; a part with a storey is contained in it, others in «Уровень 0» |
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
* `crates/ifc/tests/ifc43.rs` + `tests/fixtures/ifc/ifc4x3/verify.py` —
  IfcOpenShell 0.9.0 validates the IFC4.3 files of the three models, a curved
  (tessellated) beam, IFC4.3 types and their IFC4 fallback with zero statements
  (schema, attribute counts and EXPRESS rules, checked by file path; a negative
  control gives 3); element volumes equal those of the IFC4 files; a bridge
  file of another program (IfcBridge, IfcBridgePart, girder, bearing,
  alignment) validates and is imported.
* GUI: `tests/ifc_flow/` — export, new project, import in the native app; the
  exported file is byte-identical to the validated fixture.

The importer is bounded (512 MiB, 5 M entities, nesting 32, strings 1 MiB) and
returns errors instead of panicking.
