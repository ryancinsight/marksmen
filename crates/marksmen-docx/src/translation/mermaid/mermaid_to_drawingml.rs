use docx_rs::CustomItem;
use marksmen_mermaid::layout::coordinate_assign::SpacedGraph;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Writer;
use std::io::Cursor;
use uuid::Uuid;

/// Translates a mathematically resolved Mermaid `SpacedGraph` into a native OpenXML DrawingML `<w:drawing>` vector payload.
/// Bypasses the empirical headless Chromium SVG-to-PNG dependency graph.
pub struct DrawingMlAstGenerator;

impl DrawingMlAstGenerator {
    /// Renders a mathematical layout struct into a DOCX compatible tuple.
    pub fn render_graph(graph: &SpacedGraph) -> Vec<(String, String)> {
        let mut custom_items = Vec::new();
        
        let cxn_id_start = 1000;
        let mut cxn_id = cxn_id_start;

        // Render edges first
        for edge in &graph.edges {
             if edge.path.len() < 2 { continue; }
             
             let p1 = edge.path[0];
             let p2 = edge.path[edge.path.len() - 1];
             
             let x1 = (p1.0 * 12700.0) as i64;
             let y1 = (p1.1 * 12700.0) as i64;
             let x2 = (p2.0 * 12700.0) as i64;
             let y2 = (p2.1 * 12700.0) as i64;
             
             let cx = x1.min(x2);
             let cy = y1.min(y2);
             let cw = (x2 - x1).abs().max(1);
             let ch = (y2 - y1).abs().max(1);
             
             // Wrap in generic drawing node that Word's renderer can evaluate as an absolute page overlay
             let xml = format!(
                 r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{}" cy="{}"/><wp:effectExtent b="0" l="0" r="0" t="0"/><wp:wrapNone/><wp:docPr id="{}" name="edge_{}"/><wp:cNvGraphicFramePr/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="line"><a:avLst/></a:prstGeom><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#,
                 cx, cy, cw, ch, cxn_id, cxn_id, cx, cy, cw, ch
             );
             custom_items.push((format!("edge_{}", cxn_id), xml));
             cxn_id += 1;
        }
        
        // Render node shapes
        let node_id_start = 2000;
        let mut node_idx = node_id_start;
        for node in &graph.nodes {
             let cx = (node.1.x * 12700.0) as i64;
             let cy = (node.1.y * 12700.0) as i64;
             let cw = (node.1.width * 12700.0) as i64;
             let ch = (node.1.height * 12700.0) as i64;
             
             let xml = format!(
                 r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{}" cy="{}"/><wp:effectExtent b="0" l="0" r="0" t="0"/><wp:wrapNone/><wp:docPr id="{}" name="node_{}"/><wp:cNvGraphicFramePr/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="F0F0F0"/></a:solidFill><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#,
                 cx, cy, cw, ch, node_idx, node_idx, cx, cy, cw, ch, node.1.label
             );
             custom_items.push((format!("node_{}", node_idx), xml));
             node_idx += 1;
        }
        
        custom_items
    }
}

