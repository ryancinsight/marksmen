use pulldown_cmark::Event;
use marksmen_core::config::Config;
use anyhow::Result;
use std::path::Path;

/// Represents the mathematically abstracted XML documents required by the OpenDocument format.
/// These strings are strictly compliant with the `urn:oasis:names:tc:opendocument:xmlns:office:1.0` schema.
pub struct OdtDom {
    pub content_xml: String,
    pub styles_xml: String,
    pub meta_xml: String,
}

pub mod translator;

/// Iterates structurally over the parsed `Event` stream and sequentially constructs
/// the XML nodes for the OpenDocument DOM representation.
pub fn translate<'a>(events: &[Event<'a>], config: &Config, input_dir: &Path) -> Result<OdtDom> {
    let body_nodes = translator::translate_events(events, config, input_dir);
    
    // Generate the full OpenXML representation
    let content_xml = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content 
  xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
  xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
  <office:automatic-styles>
    <style:style style:name="S_Bold" style:family="text">
      <style:text-properties fo:font-weight="bold" style:font-weight-asian="bold" style:font-weight-complex="bold"/>
    </style:style>
    <style:style style:name="S_Italic" style:family="text">
      <style:text-properties fo:font-style="italic" style:font-style-asian="italic" style:font-style-complex="italic"/>
    </style:style>
    <style:style style:name="S_MathInline" style:family="text">
      <style:text-properties fo:font-style="italic" style:font-style-asian="italic" style:font-style-complex="italic" style:font-name="Cambria Math" fo:font-family="Cambria Math"/>
    </style:style>
    <style:style style:name="S_Code" style:family="text">
      <style:text-properties style:font-name="Consolas" fo:font-family="Consolas" style:font-family-generic="modern"/>
    </style:style>
    <style:style style:name="S_Underline" style:family="text">
      <style:text-properties style:text-underline-style="solid" style:text-underline-width="auto"/>
    </style:style>
    <style:style style:name="S_Sub" style:family="text">
      <style:text-properties style:text-position="sub 58%"/>
    </style:style>
    <style:style style:name="S_Sup" style:family="text">
      <style:text-properties style:text-position="super 58%"/>
    </style:style>
    <style:style style:name="P_CodeBlock" style:family="paragraph">
      <style:paragraph-properties fo:background-color="#f5f5f5" fo:padding="0.1in" fo:border="0.05pt solid #cccccc"/>
      <style:text-properties style:font-name="Consolas" fo:font-family="Consolas" style:font-family-generic="modern"/>
    </style:style>
    <style:style style:name="P_Rule" style:family="paragraph">
      <style:paragraph-properties fo:text-align="center"/>
    </style:style>
    <style:style style:name="P_DisplayMath" style:family="paragraph">
      <style:paragraph-properties fo:text-align="center"/>
      <style:text-properties fo:font-style="italic" style:font-style-asian="italic" style:font-style-complex="italic" style:font-name="Cambria Math" fo:font-family="Cambria Math"/>
    </style:style>
    <style:style style:name="P_Quote" style:family="paragraph">
      <style:paragraph-properties fo:margin-left="0.35in" fo:border-left="1pt solid #cccccc" fo:padding-left="0.12in"/>
    </style:style>
    <style:style style:name="P_HiddenMeta" style:family="paragraph">
      <style:text-properties fo:font-size="1pt" fo:color="#ffffff" text:display="none"/>
    </style:style>
    <style:style style:name="T_Title" style:family="paragraph">
      <style:paragraph-properties fo:text-align="center"/>
      <style:text-properties fo:font-size="24pt" fo:font-weight="bold"/>
    </style:style>
    <style:style style:name="T_Author" style:family="paragraph">
      <style:paragraph-properties fo:text-align="center"/>
      <style:text-properties fo:font-size="14pt"/>
    </style:style>
    <style:style style:name="P_Break" style:family="paragraph">
      <style:paragraph-properties fo:break-before="page"/>
    </style:style>
    <text:list-style style:name="L1">
      <text:list-level-style-bullet text:level="1" text:style-name="Bullet_20_Symbols" text:bullet-char="•">
        <style:list-level-properties text:list-level-position-and-space-mode="label-alignment">
          <style:list-level-label-alignment text:label-followed-by="listtab" text:list-tab-stop-position="0.5in" fo:text-indent="-0.25in" fo:margin-left="0.5in"/>
        </style:list-level-properties>
      </text:list-level-style-bullet>
    </text:list-style>
  </office:automatic-styles>
  <office:body>
    <office:text>
      {}
    </office:text>
  </office:body>
</office:document-content>"##,
        body_nodes
    );

    let styles_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles 
  xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
  xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
  <office:styles>
    <style:default-style style:family="paragraph">
      <style:paragraph-properties fo:line-height="115%"/>
      <style:text-properties fo:font-family="Arial" fo:font-size="11pt"/>
    </style:default-style>
  </office:styles>
  <office:automatic-styles>
    <style:page-layout style:name="PL_Default">
      <style:page-layout-properties fo:page-width="8.5in" fo:page-height="11in" 
                                    fo:margin-top="1in" fo:margin-bottom="1in" 
                                    fo:margin-left="1in" fo:margin-right="1in"/>
    </style:page-layout>
  </office:automatic-styles>
  <office:master-styles>
    <style:master-page style:name="Standard" style:page-layout-name="PL_Default"/>
  </office:master-styles>
</office:document-styles>"##.to_string();

    let meta_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0">
</office:document-meta>"#.to_string();

    Ok(OdtDom {
        content_xml,
        styles_xml,
        meta_xml,
    })
}
