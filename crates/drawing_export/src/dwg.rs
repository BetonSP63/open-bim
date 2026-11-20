//! DWG export using the same sheet entities and layers as DXF.
use acadrust::{DwgWriter, DxfReader, DxfVersion};
use open_bim_drawings::Sheet;
use std::io::Cursor;

/// AC1018 (AutoCAD 2004) uses a drawing code page. Paper units are mm.
pub fn render(sheet: &Sheet) -> Result<Vec<u8>, String> {
    let dxf = crate::dxf::render(sheet);
    let mut document = DxfReader::from_reader(Cursor::new(dxf.into_bytes()))
        .and_then(DxfReader::read)
        .map_err(|error| format!("Подготовка листа для DWG: {error}"))?;
    document.version = DxfVersion::AC1018;
    document.header.code_page = "ANSI_1251".into();
    document.header.insertion_units = 4;
    DwgWriter::write_to_vec(&document).map_err(|error| format!("Запись DWG: {error}"))
}
