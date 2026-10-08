use anyhow::{anyhow, Result};
use lopdf::{dictionary, Document, Object, ObjectId};
use std::collections::BTreeMap;

/// Basic PDF info.
pub fn info(path: &str) -> Result<String> {
    let doc = Document::load(path)?;
    let pages = doc.get_pages().len();
    let mut s = format!("PDF : {path}\n  version : {}\n  pages   : {pages}\n", doc.version);
    if let Ok(info) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Ok(dict) = doc.get_object(info).and_then(|o| o.as_dict()) {
            for (k, label) in [
                (b"Title".as_slice(), "titre"),
                (b"Author".as_slice(), "auteur"),
                (b"Subject".as_slice(), "sujet"),
                (b"Producer".as_slice(), "producteur"),
            ] {
                if let Ok(v) = dict.get(k).and_then(|o| o.as_str()) {
                    s.push_str(&format!("  {label} : {}\n", String::from_utf8_lossy(v)));
                }
            }
        }
    }
    Ok(s)
}

/// Extract text (uses pdf-extract).
pub fn text(path: &str) -> Result<String> {
    let t = pdf_extract::extract_text(path)?;
    let t = t.trim();
    if t.is_empty() {
        Ok("(aucun texte extractible — PDF scanné/image ?)".to_string())
    } else {
        let max = 20_000;
        if t.len() > max {
            Ok(format!("{}…\n(tronqué à {max} caractères)", &t[..max]))
        } else {
            Ok(t.to_string())
        }
    }
}

/// Split: keep pages `first..=last` into a new PDF.
pub fn split(path: &str, output: &str, first: u32, last: u32) -> Result<String> {
    let mut doc = Document::load(path)?;
    let pages: Vec<u32> = doc.get_pages().keys().cloned().collect();
    let to_delete: Vec<u32> = pages
        .iter()
        .cloned()
        .filter(|p| *p < first || *p > last)
        .collect();
    if !to_delete.is_empty() {
        doc.delete_pages(&to_delete);
    }
    doc.save(output)?;
    Ok(format!("Pages {first}-{last} extraites → {output}"))
}

/// Merge several PDFs into one.
pub fn merge(inputs: &[String], output: &str) -> Result<String> {
    if inputs.len() < 2 {
        return Err(anyhow!("merge : au moins 2 fichiers requis"));
    }
    let mut max_id = 1;
    let mut documents_pages: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut documents_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut document = Document::with_version("1.5");

    for input in inputs {
        let mut doc = Document::load(input)?;
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;
        for (_, object_id) in doc.get_pages() {
            let object = doc.get_object(object_id)?.to_owned();
            documents_pages.insert(object_id, object);
        }
        documents_objects.extend(doc.objects);
    }

    document.objects.extend(documents_objects);
    document.objects.extend(documents_pages.clone());

    let pages_id = document.new_object_id();
    let kids: Vec<Object> = documents_pages
        .keys()
        .map(|id| Object::Reference(*id))
        .collect();
    let count = kids.len() as i64;
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => Object::Integer(count),
    };
    document.objects.insert(pages_id, Object::Dictionary(pages));
    for id in documents_pages.keys() {
        if let Ok(page) = document.get_object_mut(*id).and_then(|o| o.as_dict_mut()) {
            page.set("Parent", Object::Reference(pages_id));
        }
    }
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    });
    document.trailer.set("Root", Object::Reference(catalog_id));
    document.max_id = document.objects.len() as u32;
    document.renumber_objects();
    document.compress();
    document.save(output)?;
    Ok(format!("{} pages fusionnées → {output}", documents_pages.len()))
}
