// SPDX-License-Identifier: Apache-2.0
//! Bounded XML and solution membership; no property expansion or project loading.
use crate::build_facts::Facts;
use anyhow::{Result, ensure};
use quick_xml::{Reader, events::Event};

struct Element {
    name: String,
    start: usize,
    content: usize,
    property: bool,
    children: bool,
}

pub fn analyze(path: &str, text: &str) -> Result<crate::syntax::Analysis> {
    if path
        .rsplit('.')
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("sln"))
    {
        return solution(path, text);
    }
    let mut output = Facts::new(path, text, "msbuild")?;
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut count = 0usize;
    let mut partial = false;
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event()?;
        let end = reader.buffer_position() as usize;
        count += 1;
        ensure!(count <= 500_000, "MSBuild exceeds XML event limit");
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                ensure!(stack.len() < 128, "MSBuild exceeds XML depth limit");
                let name = std::str::from_utf8(element.name().as_ref())?.to_owned();
                ensure!(
                    !name.contains(':'),
                    "prefixed MSBuild elements are unsupported"
                );
                if stack.is_empty() {
                    ensure!(root.is_none(), "multiple XML roots");
                    let expected = if path
                        .rsplit('.')
                        .next()
                        .is_some_and(|s| s.eq_ignore_ascii_case("slnx"))
                    {
                        "Solution"
                    } else {
                        "Project"
                    };
                    ensure!(name == expected, "unexpected build XML root");
                    root = Some(name.clone());
                }
                let mut attrs = Vec::new();
                for attr in element.attributes() {
                    let attr = attr?;
                    ensure!(attrs.len() < 256, "MSBuild exceeds attribute limit");
                    let key = std::str::from_utf8(attr.key.as_ref())?.to_owned();
                    let value = attr.unescape_value()?.into_owned();
                    ensure!(value.len() <= 4096, "MSBuild attribute exceeds byte limit");
                    if key == "xmlns" {
                        ensure!(
                            value.is_empty()
                                || value == "http://schemas.microsoft.com/developer/msbuild/2003",
                            "unsupported build XML namespace"
                        );
                    }
                    attrs.push((key, value));
                }
                let parent = stack.last().map(|e| e.name.as_str());
                let project = root.as_deref() == Some("Project");
                let property = project && parent == Some("PropertyGroup");
                for (key, value) in &attrs {
                    let kind = if project {
                        match (name.as_str(), key.as_str()) {
                            ("ProjectReference", "Include") if parent == Some("ItemGroup") => {
                                Some("project_reference")
                            }
                            ("PackageReference" | "PackageVersion", "Include" | "Update")
                                if parent == Some("ItemGroup") =>
                            {
                                Some("package_reference")
                            }
                            ("Import", "Project") => Some("import_argument"),
                            ("Target", "Name") => Some("build_target"),
                            ("Target", "DependsOnTargets" | "BeforeTargets" | "AfterTargets") => {
                                Some("target_dependency")
                            }
                            (_, "Condition") => Some("build_condition"),
                            ("Project", "Sdk") => Some("sdk_reference"),
                            ("PackageReference" | "PackageVersion", "Version") => {
                                Some("package_version")
                            }
                            _ => None,
                        }
                    } else {
                        match (name.as_str(), key.as_str()) {
                            ("Project", "Path")
                                if matches!(parent, Some("Solution" | "Folder")) =>
                            {
                                Some("solution_member")
                            }
                            ("Folder", "Name") => Some("solution_folder"),
                            _ => None,
                        }
                    };
                    if let Some(kind) = kind {
                        output.push(start, end, kind, &format!("xml_{name}_{key}"), value)?;
                    }
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children = true;
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(Element {
                        name,
                        start,
                        content: end,
                        property,
                        children: false,
                    });
                } else if property {
                    output.push(start, end, "build_property", "xml_property", &name)?;
                }
            }
            Event::End(_) => {
                let element = stack
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("unmatched XML end"))?;
                if element.property {
                    output.push(
                        element.start,
                        end,
                        "build_property",
                        "xml_property",
                        &element.name,
                    )?;
                    if !element.children {
                        let value = &text[element.content..start];
                        if !value.trim().is_empty() {
                            output.push(
                                element.content,
                                start,
                                if matches!(
                                    element.name.as_str(),
                                    "TargetFramework" | "TargetFrameworks"
                                ) {
                                    "target_framework"
                                } else {
                                    "property_value"
                                },
                                "xml_property_value",
                                value,
                            )?;
                        }
                    } else {
                        partial = true;
                    }
                }
            }
            Event::DocType(_) => anyhow::bail!("MSBuild DTD declarations are unsupported"),
            Event::Text(value) if stack.is_empty() => ensure!(
                value.iter().all(u8::is_ascii_whitespace),
                "text outside XML root"
            ),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(root.is_some() && stack.is_empty(), "incomplete build XML");
    Ok(output.finish(partial))
}

fn solution(path: &str, text: &str) -> Result<crate::syntax::Analysis> {
    let mut output = Facts::new(path, text, "msbuild")?;
    let mut offset = 0;
    let mut partial = false;
    ensure!(
        text.trim_start_matches('\u{feff}')
            .starts_with("Microsoft Visual Studio Solution File, Format Version "),
        "unsupported solution header"
    );
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("Project(") {
            let fields = trimmed.split_once(" = ").map(|(_, fields)| fields);
            let values = fields.and_then(solution_fields);
            if let Some(values) = values {
                let folder =
                    trimmed
                        .get(..trimmed.find(" = ").unwrap_or(0))
                        .is_some_and(|prefix| {
                            let prefix = prefix.to_ascii_uppercase();
                            prefix.contains("{2150E333-8FDC-42A3-9474-1A3956D46DE8}")
                                || prefix.contains("{66A26720-8FB5-11D2-AA7E-00C04F688DDE}")
                        });
                let member = values[1];
                output.push(
                    offset,
                    offset + line.len(),
                    if folder {
                        "solution_folder"
                    } else {
                        "solution_member"
                    },
                    "sln_project",
                    member,
                )?;
            } else {
                partial = true;
            }
        }
        offset += line.len();
    }
    Ok(output.finish(partial))
}

fn solution_fields(mut input: &str) -> Option<[&str; 3]> {
    let mut fields = [""; 3];
    for (index, field) in fields.iter_mut().enumerate() {
        input = input.trim_start().strip_prefix('"')?;
        let end = input.find('"')?;
        *field = &input[..end];
        input = input[end + 1..].trim_start();
        if index < 2 {
            input = input.strip_prefix(',')?;
        }
    }
    input.trim().is_empty().then_some(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conditional_references_properties_and_original_locations() -> Result<()> {
        let text = "<Project Sdk=\"Example.Sdk\">\r\n<PropertyGroup Condition=\"'$(Flavor)' == 'small'\"><TargetFrameworks>net8.0;net9.0</TargetFrameworks></PropertyGroup>\r\n<ItemGroup><ProjectReference Include=\"../beacon/beacon.csproj\"/><PackageReference Include=\"Fiction.Core\" Version=\"1.2\"/></ItemGroup>\n<Import Project=\"$(Root)/custom.props\"/><Target Name=\"Assemble\" DependsOnTargets=\"Prepare;Compile\"/></Project>";
        let result = analyze("sample.csproj", text)?;
        for kind in [
            "build_condition",
            "target_framework",
            "project_reference",
            "package_reference",
            "package_version",
            "import_argument",
            "build_target",
            "target_dependency",
        ] {
            assert!(
                result.facts.iter().any(|f| f.kind == kind),
                "missing {kind}"
            );
        }
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "build_condition" && f.name.contains("$(Flavor)"))
        );
        assert!(result.facts.iter().all(|f| {
            let span = f.code.as_ref().unwrap().span.as_ref().unwrap();
            text.get(span.start_byte..span.end_byte).is_some()
        }));
        Ok(())
    }
    #[test]
    fn solution_formats_and_xml_failures() -> Result<()> {
        assert_eq!(
            solution_fields("\"Beacon, Core\", \"src/Beacon,Core.csproj\", \"{ID}\"").unwrap()[1],
            "src/Beacon,Core.csproj"
        );
        let sln = "Microsoft Visual Studio Solution File, Format Version 12.00\r\nProject(\"{TYPE}\") = \"Beacon\", \"src\\Beacon.csproj\", \"{ID}\"\r\nEndProject\r\n";
        assert_eq!(analyze("x.sln", sln)?.facts[0].name, "src\\Beacon.csproj");
        assert_eq!(analyze("x.slnx", "<Solution><Folder Name=\"src\"><Project Path=\"beacon.csproj\"/></Folder></Solution>")?.facts.len(), 2);
        for bad in [
            "<!DOCTYPE Project SYSTEM 'file:///secret'><Project/>",
            "<Project><Target></Project>",
            "<Project Sdk='a' Sdk='b'/>",
            "<Other/>",
            "<Project>",
        ] {
            assert!(analyze("x.csproj", bad).is_err(), "accepted {bad}");
        }
        Ok(())
    }
}
