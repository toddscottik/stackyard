//! Optional real-artifact integration: see docs/evidence/S8A-INTEGRATE.md.
#[cfg(windows)]
#[test]
fn publisher_checked_php_and_composer_install_and_launch_from_owned_paths() {
    use stackyard_core::{
        find_package, install_reader, prepare_data_dir, select_for_project, Progress,
        DEFAULT_PROJECT,
    };
    use std::{fs, path::PathBuf};
    let Some(archive) = std::env::var_os("STACKYARD_TEST_PHP_ARCHIVE") else {
        eprintln!("not executed: set STACKYARD_TEST_PHP_ARCHIVE and STACKYARD_TEST_COMPOSER");
        return;
    };
    let composer =
        PathBuf::from(std::env::var_os("STACKYARD_TEST_COMPOSER").expect("Composer phar path"));
    let data =
        prepare_data_dir(&stackyard_core::disposable_dir("managed php проба space")).unwrap();
    let php = install_reader(
        &data,
        &find_package("php-8.5.12-1-windows-x64").unwrap(),
        fs::File::open(archive).unwrap(),
        &Progress::new(),
    )
    .unwrap();
    let phar = install_reader(
        &data,
        &find_package("composer-2.11.1-windows-x64").unwrap(),
        fs::File::open(composer).unwrap(),
        &Progress::new(),
    )
    .unwrap();
    assert!(PathBuf::from(&phar.launch).is_file());
    select_for_project(&data, DEFAULT_PROJECT, &php.id).unwrap();
    select_for_project(&data, DEFAULT_PROJECT, &phar.id).unwrap();
    let identity = stackyard_core::php::probe_php(std::path::Path::new(&php.launch)).unwrap();
    assert_eq!(identity.version, "8.5.12");
    let version = stackyard_core::php_project::probe_composer(
        std::path::Path::new(&php.launch),
        std::path::Path::new(&phar.launch),
    )
    .unwrap();
    assert!(version.starts_with("Composer version 2.11.1"));
    let mut corrupt = find_package("composer-2.11.1-windows-x64").unwrap();
    corrupt.id = "composer-corrupt-fixture".into();
    corrupt.version = "fixture".into();
    let error = install_reader(&data, &corrupt, &b"not a phar"[..], &Progress::new()).unwrap_err();
    assert!(matches!(
        error,
        stackyard_core::Error::DigestMismatch { .. }
    ));
    assert!(!data.join("tools/composer/fixture").exists());
    fs::remove_dir_all(data).unwrap();
}
