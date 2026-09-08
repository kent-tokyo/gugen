#![cfg(feature = "serde")]

use gugen::{NetworkLoadMode, ReactionNetwork, parse_network_records};

#[test]
fn fixture_import_reports_rejections_and_builds_only_qualified_graph() {
    let jsonl = include_str!("fixtures/network_records.jsonl");
    let (records, report) = parse_network_records(jsonl, NetworkLoadMode::Lenient).unwrap();

    assert_eq!(records.len(), 2);
    assert_eq!(report.accepted, 2);
    assert_eq!(report.qualified, 1);
    assert_eq!(report.explicitly_rejected, 1);
    assert_eq!(report.duplicates_collapsed, 1);
    assert_eq!(report.rejected.len(), 2);
    assert_eq!(report.rejected[0].line, 3);
    assert_eq!(report.rejected[1].line, 4);

    let network = ReactionNetwork::from_records(&records).unwrap();
    assert_eq!(network.reactions().len(), 1);
}
