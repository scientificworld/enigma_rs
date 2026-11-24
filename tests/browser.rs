use super::*;

#[test]
fn malformed_inputs_return_errors_in_every_mode() {
    for mode in ["unpack", "runtime", "scan", "devirt"] {
        assert!(run(b"not a PE", "input.exe", mode, "1000", false).is_err());
    }
}

#[test]
fn rejects_archive_path_names_before_processing_input() {
    for name in [
        "",
        ".",
        "..",
        "../input.exe",
        "a/b.exe",
        "a\\b.exe",
        "C:input.exe",
    ] {
        let result = run(&[], name, "unpack", "", false);
        assert!(result.is_err_and(|error| error.to_string() == "invalid input filename"));
    }
}
