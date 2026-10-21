//! IFC4 export of the declared subset (see docs/ifc-support.md).
use crate::step::{encode, guid, real, valid_guid};
use crate::{GOST_PSET_NAME, GUID_PROPERTY, KSI_PROPERTY, PSET_NAME};
use open_bim_model::{
    DetailKind, ElementShape, Frame, MemberKind, ObjectId, ProfileShape, Project, PropertyValue,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ExportReport {
    pub beams: usize,
    pub columns: usize,
    pub plates: usize,
    pub openings: usize,
    /// Model data outside the IFC subset, by kind, with object IDs.
    pub skipped: BTreeMap<String, Vec<ObjectId>>,
}

/// Deterministic GUID from a namespace and a model ID.
pub fn derived_guid(namespace: &str, id: u64) -> String {
    let digest = Sha256::digest(format!("open-bim:{namespace}:{id}").as_bytes());
    let mut bits = [0u8; 16];
    bits.copy_from_slice(&digest[..16]);
    // RFC 4122 version 5 style marker bits keep the value a well-formed UUID.
    bits[6] = (bits[6] & 0x0f) | 0x50;
    bits[8] = (bits[8] & 0x3f) | 0x80;
    guid(u128::from_be_bytes(bits))
}

pub fn element_guid(project: &Project, id: ObjectId) -> String {
    match project
        .elements
        .get(&id)
        .and_then(|e| e.properties.get(GUID_PROPERTY))
    {
        Some(PropertyValue::Text(g)) if valid_guid(g) => g.clone(),
        _ => derived_guid("element", id.0),
    }
}

struct Writer {
    out: String,
    next: u64,
}

impl Writer {
    fn add(&mut self, entity: String) -> u64 {
        let id = self.next;
        self.next += 1;
        let _ = writeln!(self.out, "#{id}={entity};");
        id
    }
    fn point(&mut self, p: [f64; 3]) -> u64 {
        self.add(format!(
            "IFCCARTESIANPOINT(({},{},{}))",
            real(p[0]),
            real(p[1]),
            real(p[2])
        ))
    }
    fn direction(&mut self, d: [f64; 3]) -> u64 {
        self.add(format!(
            "IFCDIRECTION(({},{},{}))",
            real(d[0]),
            real(d[1]),
            real(d[2])
        ))
    }
    fn placement(&mut self, origin: [f64; 3], z: [f64; 3], x: [f64; 3]) -> u64 {
        let (p, z, x) = (self.point(origin), self.direction(z), self.direction(x));
        self.add(format!("IFCAXIS2PLACEMENT3D(#{p},#{z},#{x})"))
    }
}

fn refs(ids: &[u64]) -> String {
    ids.iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn label(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Text(t) => format!("IFCLABEL({})", encode(t)),
        PropertyValue::Number(n) => format!("IFCREAL({})", real(*n)),
        PropertyValue::Boolean(b) => format!("IFCBOOLEAN(.{}.)", if *b { "T" } else { "F" }),
    }
}

fn profile_def(w: &mut Writer, name: &str, shape: &ProfileShape) -> u64 {
    let n = encode(name);
    w.add(match *shape {
        ProfileShape::ISection {
            height,
            width,
            web,
            flange,
        } => format!(
            "IFCISHAPEPROFILEDEF(.AREA.,{n},$,{},{},{},{},$,$,$)",
            real(width),
            real(height),
            real(web),
            real(flange)
        ),
        ProfileShape::Angle {
            height,
            width,
            thickness,
        } => format!(
            "IFCLSHAPEPROFILEDEF(.AREA.,{n},$,{},{},{},$,$,$)",
            real(height),
            real(width),
            real(thickness)
        ),
        ProfileShape::Channel {
            height,
            width,
            web,
            flange,
        } => format!(
            "IFCUSHAPEPROFILEDEF(.AREA.,{n},$,{},{},{},{},$,$,$)",
            real(height),
            real(width),
            real(web),
            real(flange)
        ),
        ProfileShape::CircularHollow {
            diameter,
            thickness,
        } => format!(
            "IFCCIRCLEHOLLOWPROFILEDEF(.AREA.,{n},$,{},{})",
            real(diameter / 2.),
            real(thickness)
        ),
        ProfileShape::RectangularHollow {
            height,
            width,
            thickness,
        } => format!(
            "IFCRECTANGLEHOLLOWPROFILEDEF(.AREA.,{n},$,{},{},{},$,$)",
            real(width),
            real(height),
            real(thickness)
        ),
        ProfileShape::Rectangle { height, width } => format!(
            "IFCRECTANGLEPROFILEDEF(.AREA.,{n},$,{},{})",
            real(width),
            real(height)
        ),
    })
}

/// Element origin relative to the project origin (site placement).
fn local(frame: &Frame, origin: open_bim_model::Point3) -> [f64; 3] {
    std::array::from_fn(|i| frame.origin.0[i] - origin.0[i])
}

/// IFC4 STEP text and a report of model data that was not exported.
/// Largest dimension of a part (length or section), for opening sizes.
fn part_extent(project: &Project, element: &open_bim_model::Element) -> f64 {
    match &element.shape {
        ElementShape::Member {
            length, profile, ..
        } => {
            let section = project
                .profiles
                .get(profile)
                .and_then(|p| p.shape.solid_contour().ok())
                .map(|c| c.iter().flatten().map(|v| v.abs()).fold(0., f64::max))
                .unwrap_or(1000.);
            length.value() + 2. * section
        }
        ElementShape::Plate {
            contour, thickness, ..
        } => contour.iter().flatten().map(|v| v.abs()).fold(0., f64::max) * 2. + thickness.value(),
    }
}

pub fn export(project: &Project) -> Result<(String, ExportReport), String> {
    project.validate().map_err(|e| e.to_string())?;
    let mut w = Writer {
        out: String::new(),
        next: 1,
    };
    let mut report = ExportReport::default();
    let origin = w.placement([0.; 3], [0., 0., 1.], [1., 0., 0.]);
    let context = w.add(format!(
        "IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{origin},$)"
    ));
    let body = w.add(format!(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,#{context},$,.MODEL_VIEW.,$)"
    ));
    let mm = w.add("IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)".into());
    let area = w.add("IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.)".into());
    let volume = w.add("IFCSIUNIT(*,.VOLUMEUNIT.,$,.CUBIC_METRE.)".into());
    let angle = w.add("IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.)".into());
    let kg = w.add("IFCSIUNIT(*,.MASSUNIT.,.KILO.,.GRAM.)".into());
    let metre = w.add("IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.)".into());
    let kg_e = w.add(format!("IFCDERIVEDUNITELEMENT(#{kg},1)"));
    let m_e = w.add(format!("IFCDERIVEDUNITELEMENT(#{metre},-3)"));
    let density = w.add(format!(
        "IFCDERIVEDUNIT((#{kg_e},#{m_e}),.MASSDENSITYUNIT.,$)"
    ));
    let units = w.add(format!(
        "IFCUNITASSIGNMENT((#{mm},#{area},#{volume},#{angle},#{kg},#{density}))"
    ));
    let project_id = w.add(format!(
        "IFCPROJECT('{}',$,{},$,$,$,$,(#{context}),#{units})",
        derived_guid("project", 0),
        encode(&project.name)
    ));
    let site_place = w.placement(project.origin.0, [0., 0., 1.], [1., 0., 0.]);
    let site_lp = w.add(format!("IFCLOCALPLACEMENT($,#{site_place})"));
    let site = w.add(format!(
        "IFCSITE('{}',$,'Площадка',$,$,#{site_lp},$,$,.ELEMENT.,$,$,$,$,$)",
        derived_guid("site", 0)
    ));
    let building_place = w.placement([0.; 3], [0., 0., 1.], [1., 0., 0.]);
    let building_lp = w.add(format!("IFCLOCALPLACEMENT(#{site_lp},#{building_place})"));
    let building = w.add(format!(
        "IFCBUILDING('{}',$,'Здание',$,$,#{building_lp},$,$,.ELEMENT.,$,$,$)",
        derived_guid("building", 0)
    ));
    let storey_place = w.placement([0.; 3], [0., 0., 1.], [1., 0., 0.]);
    let storey_lp = w.add(format!("IFCLOCALPLACEMENT(#{building_lp},#{storey_place})"));
    let storey = w.add(format!(
        "IFCBUILDINGSTOREY('{}',$,'Уровень 0',$,$,#{storey_lp},$,$,.ELEMENT.,0.)",
        derived_guid("storey", 0)
    ));
    w.add(format!(
        "IFCRELAGGREGATES('{}',$,$,$,#{project_id},(#{site}))",
        derived_guid("aggregate", 0)
    ));
    w.add(format!(
        "IFCRELAGGREGATES('{}',$,$,$,#{site},(#{building}))",
        derived_guid("aggregate", 1)
    ));
    w.add(format!(
        "IFCRELAGGREGATES('{}',$,$,$,#{building},(#{storey}))",
        derived_guid("aggregate", 2)
    ));
    let z = w.direction([0., 0., 1.]);
    let identity = w.placement([0.; 3], [0., 0., 1.], [1., 0., 0.]);
    let mut products = Vec::new();
    let mut by_material: BTreeMap<ObjectId, Vec<u64>> = BTreeMap::new();
    let mut profiles: BTreeMap<ObjectId, u64> = BTreeMap::new();
    let mut ksi: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for element in project.elements.values() {
        let frame = element.shape.frame();
        let place = w.placement(local(frame, project.origin), frame.axes[2], frame.axes[0]);
        let lp = w.add(format!("IFCLOCALPLACEMENT(#{storey_lp},#{place})"));
        let (profile, depth) = match &element.shape {
            ElementShape::Member {
                profile, length, ..
            } => {
                let def = match profiles.get(profile) {
                    Some(def) => *def,
                    None => {
                        let p = &project.profiles[profile];
                        let def = profile_def(&mut w, &p.name, &p.shape);
                        profiles.insert(*profile, def);
                        def
                    }
                };
                (def, length.value())
            }
            ElementShape::Plate {
                contour, thickness, ..
            } => {
                let mut points: Vec<u64> = contour
                    .iter()
                    .map(|p| {
                        w.add(format!(
                            "IFCCARTESIANPOINT(({},{}))",
                            real(p[0]),
                            real(p[1])
                        ))
                    })
                    .collect();
                points.push(points[0]);
                let poly = w.add(format!("IFCPOLYLINE(({}))", refs(&points)));
                (
                    w.add(format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{poly})")),
                    thickness.value(),
                )
            }
        };
        let solid = w.add(format!(
            "IFCEXTRUDEDAREASOLID(#{profile},#{identity},#{z},{})",
            real(depth)
        ));
        let rep = w.add(format!(
            "IFCSHAPEREPRESENTATION(#{body},'Body','SweptSolid',(#{solid}))"
        ));
        let shape = w.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{rep}))"));
        let gid = element_guid(project, element.id);
        let name = encode(&element.name);
        let tag = format!("'{}'", element.id.0);
        let product = w.add(match &element.shape {
            ElementShape::Member {
                member_kind: MemberKind::Column,
                ..
            } => {
                report.columns += 1;
                format!("IFCCOLUMN('{gid}',$,{name},$,$,#{lp},#{shape},{tag},.COLUMN.)")
            }
            ElementShape::Member { .. } => {
                report.beams += 1;
                format!("IFCBEAM('{gid}',$,{name},$,$,#{lp},#{shape},{tag},.BEAM.)")
            }
            ElementShape::Plate { .. } => {
                report.plates += 1;
                format!("IFCPLATE('{gid}',$,{name},$,$,#{lp},#{shape},{tag},.SHEET.)")
            }
        });
        products.push(product);
        by_material
            .entry(element.material)
            .or_default()
            .push(product);
        let props: Vec<u64> = element
            .properties
            .iter()
            .filter(|(k, _)| k.as_str() != GUID_PROPERTY)
            .map(|(k, v)| {
                w.add(format!(
                    "IFCPROPERTYSINGLEVALUE({},$,{},$)",
                    encode(k),
                    label(v)
                ))
            })
            .collect();
        if !props.is_empty() {
            let pset = w.add(format!(
                "IFCPROPERTYSET('{}',$,{},$,({}))",
                derived_guid("pset", element.id.0),
                encode(PSET_NAME),
                refs(&props)
            ));
            w.add(format!(
                "IFCRELDEFINESBYPROPERTIES('{}',$,$,$,(#{product}),#{pset})",
                derived_guid("defines", element.id.0)
            ));
        }
        // Section, its standard, steel and mass as Russian specifications
        // give them.
        let (section, standard) = match &element.shape {
            ElementShape::Member { profile, .. } => {
                let p = &project.profiles[profile];
                (p.name.clone(), p.standard.clone())
            }
            ElementShape::Plate { thickness, .. } => (
                format!("—{}", real(thickness.value()).trim_end_matches('.')),
                Some(open_bim_model::library::PLATE_STANDARD.into()),
            ),
        };
        let text = |w: &mut Writer, name: &str, value: &str| {
            w.add(format!(
                "IFCPROPERTYSINGLEVALUE({},$,IFCLABEL({}),$)",
                encode(name),
                encode(value)
            ))
        };
        let mut gost = vec![text(&mut w, "Профиль", &section)];
        if let Some(standard) = standard {
            gost.push(text(&mut w, "Сортамент", &standard));
        }
        if let Some(m) = project.materials.get(&element.material) {
            gost.push(text(&mut w, "Сталь", &m.name));
        }
        if let Ok(mass) = project.element_mass_kg(element.id) {
            gost.push(w.add(format!(
                "IFCPROPERTYSINGLEVALUE({},$,IFCMASSMEASURE({}),$)",
                encode("Масса, кг"),
                real((mass * 10.).round() / 10.)
            )));
        }
        let gost_set = w.add(format!(
            "IFCPROPERTYSET('{}',$,{},$,({}))",
            derived_guid("gost-pset", element.id.0),
            encode(GOST_PSET_NAME),
            refs(&gost)
        ));
        w.add(format!(
            "IFCRELDEFINESBYPROPERTIES('{}',$,$,$,(#{product}),#{gost_set})",
            derived_guid("gost-defines", element.id.0)
        ));
        if let Some(PropertyValue::Text(code)) = element.properties.get(KSI_PROPERTY)
            && !code.trim().is_empty()
        {
            ksi.entry(code.trim().to_string())
                .or_default()
                .push(product);
        }
        // Holes and axis-aligned box cuts become openings voiding the element.
        for detail in project
            .details
            .values()
            .filter(|d| d.kind.part() == Some(element.id))
        {
            let (frame_local, profile, depth) = match &detail.kind {
                DetailKind::Hole {
                    frame: hole,
                    diameter,
                    depth,
                    ..
                } => (
                    *hole,
                    w.add(format!(
                        "IFCCIRCLEPROFILEDEF(.AREA.,$,$,{})",
                        real(diameter / 2.)
                    )),
                    *depth,
                ),
                DetailKind::BoxCut { origin, size, .. } => {
                    let centre = [
                        origin.0[0] + size[0] / 2.,
                        origin.0[1] + size[1] / 2.,
                        origin.0[2],
                    ];
                    (
                        Frame {
                            origin: open_bim_model::Point3(centre),
                            ..Frame::default()
                        },
                        w.add(format!(
                            "IFCRECTANGLEPROFILEDEF(.AREA.,$,$,{},{})",
                            real(size[0]),
                            real(size[1])
                        )),
                        size[2],
                    )
                }
                DetailKind::PrismCut {
                    frame,
                    contour,
                    length,
                    ..
                } => {
                    let mut points: Vec<u64> = contour
                        .iter()
                        .map(|p| {
                            w.add(format!(
                                "IFCCARTESIANPOINT(({},{}))",
                                real(p[0]),
                                real(p[1])
                            ))
                        })
                        .collect();
                    points.push(points[0]);
                    let poly = w.add(format!("IFCPOLYLINE(({}))", refs(&points)));
                    (
                        *frame,
                        w.add(format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{poly})")),
                        *length,
                    )
                }
                // A plane cut voids everything on its removed side: a block
                // larger than the part, its base on the plane.
                DetailKind::PlaneCut { normal, offset, .. } => {
                    let len =
                        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
                            .sqrt();
                    let n = normal.map(|v| v / len);
                    let helper = if n[0].abs() < 0.9 {
                        [1., 0., 0.]
                    } else {
                        [0., 1., 0.]
                    };
                    let x = {
                        let c = [
                            helper[1] * n[2] - helper[2] * n[1],
                            helper[2] * n[0] - helper[0] * n[2],
                            helper[0] * n[1] - helper[1] * n[0],
                        ];
                        let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
                        c.map(|v| v / l)
                    };
                    let y = [
                        n[1] * x[2] - n[2] * x[1],
                        n[2] * x[0] - n[0] * x[2],
                        n[0] * x[1] - n[1] * x[0],
                    ];
                    let size = 4. * part_extent(project, element);
                    (
                        Frame {
                            origin: open_bim_model::Point3(n.map(|v| v * offset / len)),
                            axes: [x, y, n],
                        },
                        w.add(format!(
                            "IFCRECTANGLEPROFILEDEF(.AREA.,$,$,{},{})",
                            real(size),
                            real(size)
                        )),
                        size,
                    )
                }
                _ => continue,
            };
            let op_place = w.placement(
                frame_local.origin.0,
                frame_local.axes[2],
                frame_local.axes[0],
            );
            let op_lp = w.add(format!("IFCLOCALPLACEMENT(#{lp},#{op_place})"));
            let op_solid = w.add(format!(
                "IFCEXTRUDEDAREASOLID(#{profile},#{identity},#{z},{})",
                real(depth)
            ));
            let op_rep = w.add(format!(
                "IFCSHAPEREPRESENTATION(#{body},'Body','SweptSolid',(#{op_solid}))"
            ));
            let op_shape = w.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{op_rep}))"));
            let opening = w.add(format!(
                "IFCOPENINGELEMENT('{}',$,'Отверстие',$,$,#{op_lp},#{op_shape},'{}',.OPENING.)",
                derived_guid("opening", detail.id.0),
                detail.id.0
            ));
            w.add(format!(
                "IFCRELVOIDSELEMENT('{}',$,$,$,#{product},#{opening})",
                derived_guid("voids", detail.id.0)
            ));
            report.openings += 1;
        }
    }
    for detail in project.details.values() {
        let kind = match detail.kind {
            DetailKind::BoltGroup { .. } => "bolt_group",
            DetailKind::Weld { .. } => "weld",
            _ => continue,
        };
        report
            .skipped
            .entry(kind.into())
            .or_default()
            .push(detail.id);
    }
    for id in project.assemblies.keys() {
        report
            .skipped
            .entry("assembly".into())
            .or_default()
            .push(*id);
    }
    for id in project.connections.keys() {
        report
            .skipped
            .entry("connection".into())
            .or_default()
            .push(*id);
    }
    for id in project.axes.keys() {
        report
            .skipped
            .entry("grid_axis".into())
            .or_default()
            .push(*id);
    }
    for id in project.drawings.keys() {
        report
            .skipped
            .entry("drawing".into())
            .or_default()
            .push(*id);
    }
    if !products.is_empty() {
        w.add(format!(
            "IFCRELCONTAINEDINSPATIALSTRUCTURE('{}',$,$,$,({}),#{storey})",
            derived_guid("contained", 0),
            refs(&products)
        ));
    }
    for (material, items) in by_material {
        let m = &project.materials[&material];
        let mat = w.add(format!("IFCMATERIAL({},$,$)", encode(&m.name)));
        let prop = w.add(format!(
            "IFCPROPERTYSINGLEVALUE('MassDensity',$,IFCMASSDENSITYMEASURE({}),$)",
            real(m.density_kg_m3)
        ));
        w.add(format!(
            "IFCMATERIALPROPERTIES('Pset_MaterialCommon',$,(#{prop}),#{mat})"
        ));
        w.add(format!(
            "IFCRELASSOCIATESMATERIAL('{}',$,$,$,({}),#{mat})",
            derived_guid("material", material.0),
            refs(&items)
        ));
    }
    // КСИ codes the user put on elements, one reference per code.
    if !ksi.is_empty() {
        let system = w.add(format!(
            "IFCCLASSIFICATION($,$,$,{},$,'https://ksi.faufcc.ru',$)",
            encode("КСИ")
        ));
        for (n, (code, items)) in ksi.iter().enumerate() {
            let reference = w.add(format!(
                "IFCCLASSIFICATIONREFERENCE($,{},$,#{system},$,$)",
                encode(code)
            ));
            w.add(format!(
                "IFCRELASSOCIATESCLASSIFICATION('{}',$,$,$,({}),#{reference})",
                derived_guid("ksi", n as u64),
                refs(items)
            ));
        }
    }
    let mut text = String::new();
    text.push_str("ISO-10303-21;\nHEADER;\n");
    text.push_str("FILE_DESCRIPTION(('ViewDefinition [DesignTransferView]'),'2;1');\n");
    let _ = writeln!(
        text,
        "FILE_NAME({},'1970-01-01T00:00:00',(''),(''),'Open BIM','Open BIM {}','');",
        encode(&format!("{}.ifc", project.name)),
        env!("CARGO_PKG_VERSION")
    );
    text.push_str("FILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n");
    text.push_str(&w.out);
    text.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok((text, report))
}
