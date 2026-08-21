use super::*;

#[test]
fn cover_meta_requires_explicit_cover_name_or_property() {
    let xml = r#"
        <package>
          <metadata>
            <dc:title>Test</dc:title>
            <meta name="dcterms:modified" content="2026-01-01T00:00:00Z"/>
            <meta name="generator" content="sigil"/>
          </metadata>
          <manifest>
            <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine><itemref idref="chapter1"/></spine>
        </package>
    "#;

    let (metadata, _, _) = EpubParser::parse_opf(xml).unwrap();
    assert!(metadata.cover_href.is_none());
}

#[test]
fn cover_meta_with_name_maps_to_manifest_href() {
    let xml = r#"
        <package>
          <metadata>
            <dc:title>Test</dc:title>
            <meta name="dcterms:modified" content="2026-01-01T00:00:00Z"/>
            <meta name="cover" content="cover-image"/>
          </metadata>
          <manifest>
            <item id="cover-image" href="images/cover.jpg" media-type="image/jpeg"/>
            <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine><itemref idref="chapter1"/></spine>
        </package>
    "#;

    let (metadata, _, _) = EpubParser::parse_opf(xml).unwrap();
    assert_eq!(metadata.cover_href.as_deref(), Some("images/cover.jpg"));
}

#[test]
fn cover_meta_with_property_maps_to_manifest_href() {
    let xml = r#"
        <package>
          <metadata>
            <dc:title>Test</dc:title>
            <meta property="dcterms:modified" content="2026-01-01T00:00:00Z"/>
            <meta property="cover" content="cover-image"/>
          </metadata>
          <manifest>
            <item id="cover-image" href="images/cover.png" media-type="image/png"/>
            <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine><itemref idref="chapter1"/></spine>
        </package>
    "#;

    let (metadata, _, _) = EpubParser::parse_opf(xml).unwrap();
    assert_eq!(metadata.cover_href.as_deref(), Some("images/cover.png"));
}
