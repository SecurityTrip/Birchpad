use birchpad_cli::associations::*;

#[test]
fn what_only_reads_changes_nothing() {
    // Nothing of Birchpad's to take away, and asking `xdg-mime` changes nothing (without it,
    // Birchpad is the default of nothing).
    unregister().unwrap();
    let _ = is_default("txt");
}
