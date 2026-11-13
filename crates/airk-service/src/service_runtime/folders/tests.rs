// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::folder_browse::FolderListing;

fn found(name: &str, hidden: bool) -> Found {
    Found {
        name: name.into(),
        kind: FolderKind::Folder,
        hidden,
    }
}

fn names(listing: &FolderListing) -> Vec<&str> {
    listing
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect()
}

#[test]
fn pages_order_by_folded_name_skip_hidden_and_continue_after_a_name() {
    let folders = vec![
        found("beta", false),
        found("Alpha", false),
        found(".git", true),
        found("gamma", false),
    ];
    let (page_one, next) = page(folders.clone(), false, None);
    let listed: Vec<_> = page_one.iter().map(|folder| folder.name.as_str()).collect();
    assert_eq!(listed, ["Alpha", "beta", "gamma"]);
    assert_eq!(next, None);
    let (with_hidden, _) = page(folders.clone(), true, None);
    assert_eq!(with_hidden[0].name, ".git");
    let (after, _) = page(folders, false, Some("beta"));
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].name, "gamma");
    let many = (0..MAX_FOLDER_ENTRIES + 5)
        .map(|n| found(&format!("f{n:04}"), false))
        .collect();
    let (first, next) = page(many, false, None);
    assert_eq!(first.len(), MAX_FOLDER_ENTRIES);
    assert_eq!(next.as_deref(), Some(first.last().unwrap().name.as_str()));
}

#[test]
fn marks_name_installation_locations_and_index_roots_by_compared_path() {
    let locations = Locations {
        program: "C:/AIRK Service/program".into(),
        data: "C:/AIRK Service/data/indexes".into(),
        state: "C:/AIRK Service/state/service".into(),
        cache: "C:/AIRK Service/cache/service".into(),
        logs: "C:/AIRK Service/logs/service".into(),
    };
    let marks = Marks::new(
        Some(&locations),
        vec![(
            "airk".into(),
            "D:/code/airk".into(),
            "C:/AIRK Service/data/indexes/airk".into(),
        )],
        true,
    );
    let (location, indexes) = marks.mark("C:/AIRK Service/data/indexes/");
    assert_eq!(location, Some(StorageLocationKind::Data));
    assert!(indexes.is_empty());
    let (_, indexes) = marks.mark("D:/code/airk");
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].use_, FolderUse::IndexedRoot);
    let (_, indexes) = marks.mark("C:/AIRK Service/data/indexes/airk");
    assert_eq!(indexes[0].use_, FolderUse::IndexData);
    assert!(marks.mark("D:/code").1.is_empty());
    if cfg!(windows) {
        assert_eq!(key(r"\\?\D:\Code\AIRK\"), key("d:/code/airk"));
    }
}

#[test]
fn the_starting_list_offers_drives_home_and_the_data_location() {
    let locations = Locations {
        program: "/srv/airk/program".into(),
        data: "/srv/airk/data/indexes".into(),
        state: "/srv/airk/state".into(),
        cache: "/srv/airk/cache".into(),
        logs: "/srv/airk/logs".into(),
    };
    let marks = Marks::new(Some(&locations), Vec::new(), true);
    let listing = starting_list(
        &marks,
        &[PathBuf::from("/")],
        Some(PathBuf::from("/home/fictional")),
    )
    .unwrap();
    let starts: Vec<_> = listing.entries.iter().map(|entry| entry.start).collect();
    assert_eq!(
        starts,
        [
            Some(FolderStart::Drive),
            Some(FolderStart::Home),
            Some(FolderStart::AirkLocation)
        ]
    );
    assert_eq!(
        listing.entries[2].airk_location,
        Some(StorageLocationKind::Data)
    );
    assert_eq!(listing.entries[1].name, "fictional");
    assert!(listing.path.is_none() && listing.parent.is_none());
}

#[test]
fn a_folder_lists_folders_only_and_reports_its_place() -> Result<()> {
    let root = tempfile::tempdir()?;
    let base = root.path().canonicalize()?;
    let base = PathBuf::from(shown(&base));
    for name in ["alpha", "Beta", ".hidden"] {
        std::fs::create_dir(base.join(name))?;
    }
    std::fs::write(base.join("notes.txt"), "not listed")?;
    let marks = Marks::default();
    let listing = folder(&base, &marks, false, None)?;
    assert_eq!(names(&listing), ["alpha", "Beta"]);
    assert_eq!(listing.path.as_deref(), Some(shown(&base).as_str()));
    assert_eq!(listing.parent, parent(&base));
    assert_eq!(
        listing.breadcrumb.last().map(|crumb| crumb.path.as_str()),
        Some(shown(&base).as_str())
    );
    assert!(!listing.truncated && listing.continuation.is_none());
    let hidden = folder(&base, &marks, true, None)?;
    assert_eq!(names(&hidden), [".hidden", "alpha", "Beta"]);
    assert!(hidden.entries[0].hidden);
    Ok(())
}

#[test]
fn missing_files_and_relative_paths_answer_their_codes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let base = root.path().canonicalize()?;
    std::fs::write(base.join("file.txt"), "x")?;
    let marks = Marks::default();
    let code = |path: &Path| {
        let error = folder(path, &marks, false, None).unwrap_err();
        super::super::storage::failure_code(&error, "other")
    };
    assert_eq!(code(&base.join("missing")), "folder_not_found");
    assert_eq!(code(&base.join("file.txt")), "folder_not_a_folder");
    assert_eq!(code(Path::new("relative/path")), "folder_path_not_absolute");
    Ok(())
}

#[test]
fn breadcrumbs_and_parents_stop_at_the_root() {
    if cfg!(windows) {
        let crumbs = crumbs(Path::new(r"D:\code\airk"));
        let steps: Vec<_> = crumbs.iter().map(|crumb| crumb.name.as_str()).collect();
        assert_eq!(steps, ["D:", "code", "airk"]);
        assert_eq!(crumbs[0].path, r"D:\");
        assert_eq!(parent(Path::new(r"D:\code")).as_deref(), Some(r"D:\"));
        assert_eq!(parent(Path::new(r"D:\")), None);
    } else {
        let crumbs = crumbs(Path::new("/srv/code"));
        let steps: Vec<_> = crumbs.iter().map(|crumb| crumb.name.as_str()).collect();
        assert_eq!(steps, ["/", "srv", "code"]);
        assert_eq!(parent(Path::new("/srv")).as_deref(), Some("/"));
        assert_eq!(parent(Path::new("/")), None);
    }
}

#[test]
fn a_folder_without_subfolders_lists_as_empty() -> Result<()> {
    let root = tempfile::tempdir()?;
    let base = PathBuf::from(shown(&root.path().canonicalize()?));
    std::fs::write(base.join("only-a-file.txt"), "x")?;
    let listing = folder(&base, &Marks::default(), false, None)?;
    assert!(listing.entries.is_empty() && listing.continuation.is_none());
    Ok(())
}
