use configurable_csv_analyzer::core::model::*;
use configurable_csv_analyzer::core::io::csv::*;
use configurable_csv_analyzer::core::transforms::*;
use configurable_csv_analyzer::core::rules::*;
use std::collections::HashMap;

#[test]
fn test_delimiter_sniffer() {
    let semi_data = b"id;name;ip\n1;sw1;10.0.0.1\n2;sw2;10.0.0.2\n";
    assert_eq!(sniff_delimiter(semi_data), ';');

    let comma_data = b"id,name,ip\n1,sw1,10.0.0.1\n2,sw2,10.0.0.2\n";
    assert_eq!(sniff_delimiter(comma_data), ',');

    let tab_data = b"id\tname\tip\n1\tsw1\t10.0.0.1\n2\tsw2\t10.0.0.2\n";
    assert_eq!(sniff_delimiter(tab_data), '\t');
}

#[test]
fn test_smart_add_row() {
    let mut table = TableData::new("t1".into(), "Switches".into());
    table.headers = vec!["Device".into(), "IP".into()];
    table.rows = vec![vec!["SW-01".into(), "192.168.1.10".into()]];

    let new_idx = smart_add_row(&mut table, Some(0));
    assert_eq!(new_idx, 1);
    assert_eq!(table.rows[1][0], "SW-02");
    assert_eq!(table.rows[1][1], "192.168.1.11");
}

#[test]
fn test_fill_and_split_column() {
    let mut table = TableData::new("t1".into(), "Test".into());
    table.headers = vec!["Host".into(), "Tag".into()];
    table.rows = vec![
        vec!["host1".into(), "".into()],
        vec!["host2".into(), "".into()],
    ];

    // Test fill increment
    fill_column(&mut table, 1, &[], FillMode::Increment { start: 100, step: 10, prefix: "TAG-".into(), pad_zeros: 4 });
    assert_eq!(table.rows[0][1], "TAG-0100");
    assert_eq!(table.rows[1][1], "TAG-0110");

    // Test split column
    table.headers = vec!["FullName".into()];
    table.rows = vec![vec!["Server-RackA-01".into()]];
    split_column(&mut table, 0, '-', &["Part1".into(), "Part2".into(), "Part3".into()]);
    assert_eq!(table.headers, vec!["FullName", "Part1", "Part2", "Part3"]);
    assert_eq!(table.rows[0], vec!["Server-RackA-01", "Server", "RackA", "01"]);
}

#[test]
fn test_rule_engine_bulk_field_and_calculation() {
    let mut table = TableData::new("t1".into(), "Inventory".into());
    table.headers = vec!["Item".into(), "Price".into(), "Qty".into(), "Status".into()];
    table.column_mappings.insert("ItemStatus".into(), "Status".into());
    table.column_mappings.insert("ItemPrice".into(), "Price".into());
    table.column_mappings.insert("ItemQty".into(), "Qty".into());

    table.rows = vec![
        vec!["Server A".into(), "100".into(), "2".into(), "".into()],
        vec!["Server B".into(), "200".into(), "3".into(), "TBD".into()],
    ];

    table.rules = vec![
        RuleDefinition::BulkField {
            id: "R1".into(),
            description: "Check Status".into(),
            concept: "ItemStatus".into(),
            check_all_for_empty: true,
            check_all_for_placeholder: true,
            placeholders: vec!["TBD".into()],
        },
        RuleDefinition::Calculation {
            id: "R2".into(),
            description: "Total Cost".into(),
            action_group: None,
            is_active: true,
            is_highlighted: true,
            target_column: "Total".into(),
            virtual_column_key: None,
            formula: "Price * Qty".into(),
        },
    ];

    let tables = HashMap::new();
    let res = RuleEngine::execute(&mut table, &tables);

    assert_eq!(res.summaries.len(), 2);
    // R1 should flag row 0 (empty) and row 1 (placeholder)
    assert_eq!(res.summaries[0].hit_count, 2);
    // R2 should calculate Total: 100*2 = 200, 200*3 = 600
    assert_eq!(table.headers.contains(&"Total".to_string()), true);
    assert_eq!(table.rows[0][4], "200");
    assert_eq!(table.rows[1][4], "600");
}

#[test]
fn test_rule_engine_group_validation_and_joins() {
    let mut table_a = TableData::new("ta".into(), "TableA".into());
    table_a.headers = vec!["DeviceID".into(), "Rack".into(), "VLAN".into()];
    table_a.column_mappings.insert("DevKey".into(), "DeviceID".into());
    table_a.column_mappings.insert("RackGroup".into(), "Rack".into());
    table_a.column_mappings.insert("VlanNum".into(), "VLAN".into());

    table_a.rows = vec![
        vec!["D1".into(), "Rack-1".into(), "10".into()],
        vec!["D1".into(), "Rack-1".into(), "10".into()], // Duplicate key!
        vec!["D2".into(), "Rack-1".into(), "999".into()], // VLAN out of range!
    ];

    table_a.rules = vec![
        RuleDefinition::GroupValidation {
            id: "GV1".into(),
            description: "Check Rack".into(),
            action_group: None,
            is_active: true,
            is_highlighted: true,
            group_by_concept: "RackGroup".into(),
            validation_type: Some("numericRange".into()),
            is_unique: true,
            unique_concept: Some("DevKey".into()),
            numeric_range: Some(NumericRangeCheck {
                concept: "VlanNum".into(),
                min: Some(1.0),
                max: Some(100.0),
            }),
            group_consistency: None,
            mismatch_brush_key: None,
            mismatch_target: None,
            mismatch_priority: None,
        }
    ];

    let tables = HashMap::new();
    let res = RuleEngine::execute(&mut table_a, &tables);
    assert!(res.summaries[0].hit_count >= 2);
}

#[test]
fn test_app_config_and_brushes() {
    let mut config = AppConfig::default();
    assert_eq!(config.selected_storage_mode, InfoFileStorageMode::Dateipfad);
    assert!(config.brushes.len() >= 20);
    assert_eq!(config.find_brush_hex("ErrorBrush"), Some("#ef5350"));

    // Add custom brush
    config.brushes.push(BrushDefinition {
        key: "CustomMarker".into(),
        hex_value: "#123456".into(),
        category: BrushCategory::Textmarker,
        is_system: false,
    });
    assert_eq!(config.find_brush_hex("CustomMarker"), Some("#123456"));
}

#[test]
fn test_table_config_validator() {
    use configurable_csv_analyzer::core::validation::TableConfigValidator;

    let app_config = AppConfig::default();
    let mut table = TableData::new("t1".into(), "TestTable".into());
    table.headers = vec!["Host".into(), "IP".into()];

    let mut info = InfoFile::default();
    info.column_mappings.insert("DeviceName".into(), "Host".into());
    info.column_mappings.insert("BadMapping".into(), "NonExistentCol".into());

    info.rules = vec![
        RuleDefinition::Calculation {
            id: "Calc1".into(),
            description: "Invalid Formula".into(),
            action_group: None,
            is_active: true,
            is_highlighted: true,
            target_column: "Out".into(),
            virtual_column_key: None,
            formula: "{UnknownConcept} + 2".into(), // Unknown placeholder!
        },
        RuleDefinition::Calculation {
            id: "Calc1".into(), // Duplicate ID!
            description: "Dup Calc".into(),
            action_group: None,
            is_active: true,
            is_highlighted: true,
            target_column: "Out2".into(),
            virtual_column_key: None,
            formula: "1 + 1".into(),
        },
    ];

    let tables = HashMap::new();
    let issues = TableConfigValidator::validate(&info, &table, &tables, &app_config);

    assert!(TableConfigValidator::has_errors(&issues));
    assert!(issues.iter().any(|i| i.message.contains("Doppelte Regel-ID")));
    assert!(issues.iter().any(|i| i.message.contains("Formelfehler")));
    assert!(issues.iter().any(|i| i.message.contains("NonExistentCol")));
}

#[test]
fn test_rule_engine_highlight_and_live_ratio() {
    let mut table = TableData::new("t1".into(), "Test".into());
    table.headers = vec!["Status".into()];
    table.rows = vec![
        vec!["ACTIVE".into()],
        vec!["INACTIVE".into()],
        vec!["active".into()],
    ];

    table.rules = vec![
        RuleDefinition::Highlight {
            id: "H1".into(),
            description: "Active items".into(),
            action_group: None,
            is_active: true,
            is_highlighted: true,
            column_concept: "Status".into(),
            condition: "Equals".into(),
            value_to_match: "active".into(),
            multiple_values: vec![],
            invert_condition: false,
            brush_key: "PositiveBrush".into(),
            target: "Cell".into(),
            priority: 10,
        }
    ];

    let tables = HashMap::new();
    let res = RuleEngine::execute(&mut table, &tables);
    assert_eq!(res.summaries[0].hit_count, 2); // ACTIVE and active

    // Test join match ratio
    let mut table_b = TableData::new("t2".into(), "Target".into());
    table_b.headers = vec!["Status".into()];
    table_b.rows = vec![vec!["ACTIVE".into()]];

    let (matched, total, ratio) = RuleEngine::calculate_join_match_ratio(&table, &table_b, "Status", "Status");
    assert_eq!(matched, 2);
    assert_eq!(total, 3);
    assert!((ratio - 66.666).abs() < 1.0);
}

#[test]
fn test_color_cache() {
    use configurable_csv_analyzer::core::color_cache::cached_color;

    // Test named brush key
    let c1 = cached_color("DirtyCellBrush", 255);
    let c2 = cached_color("DirtyCellBrush", 255);
    assert_eq!(c1, c2);

    // Test hex color
    let hex_c = cached_color("#123456", 200);
    assert_eq!(hex_c.red(), 0x12);
    assert_eq!(hex_c.green(), 0x34);
    assert_eq!(hex_c.blue(), 0x56);
    assert_eq!(hex_c.alpha(), 200);

    // Test fallback unknown key
    let fb = cached_color("UnknownNonHexBrushKey", 255);
    assert_eq!(fb.alpha(), 0);
}

#[test]
fn test_virtual_columns_multi_step_joins() {
    // 1. Values match logic
    assert!(!RuleEngine::values_match("", "", false));
    assert!(!RuleEngine::values_match("   ", "", false));
    assert!(RuleEngine::values_match("10", "10.0", false));
    assert!(RuleEngine::values_match("05", "5", false));
    assert!(RuleEngine::values_match("Patch-01", "patch-01", false));
    assert!(!RuleEngine::values_match("Patch-01", "patch-01", true));

    // 2. Setup tables: Host (Patchpanel) -> Intermediate (Switches) -> Target (Rooms)
    let mut patchpanel = TableData::new("patchpanel".into(), "patchpanel.csv".into());
    patchpanel.headers = vec!["PortID".into(), "SwitchID".into()];
    patchpanel.rows = vec![
        vec!["P1".into(), "SW1".into()],
        vec!["P2".into(), "SW2".into()],
        vec!["P3".into(), "SW_UNKNOWN".into()],
    ];

    let mut switches = TableData::new("switches".into(), "switches.csv".into());
    switches.headers = vec!["SwitchID".into(), "SwitchIP".into(), "RoomCode".into()];
    switches.rows = vec![
        vec!["SW1".into(), "10.0.0.1".into(), "R101".into()],
        vec!["SW2".into(), "10.0.0.2".into(), "R102".into()],
    ];

    let mut rooms = TableData::new("rooms".into(), "rooms.csv".into());
    rooms.headers = vec!["RoomCode".into(), "Building".into(), "Floor".into()];
    rooms.rows = vec![
        vec!["R101".into(), "Gebäude A".into(), "1".into()],
        vec!["R102".into(), "Gebäude B".into(), "2".into()],
    ];

    let mut all_tables = HashMap::new();
    all_tables.insert("switches.csv".to_string(), switches);
    all_tables.insert("rooms.csv".to_string(), rooms);

    // Multi-step link steps:
    // Step 1: patchpanel -> switches (SwitchID == SwitchID)
    // Step 2: switches -> rooms (RoomCode == RoomCode)
    let steps = vec![
        LinkStep {
            target_table_key: "switches.csv".to_string(),
            key_mappings: vec![KeyMapping::new("SwitchID", "SwitchID")],
        },
        LinkStep {
            target_table_key: "rooms.csv".to_string(),
            key_mappings: vec![KeyMapping::new("RoomCode", "RoomCode")],
        },
    ];

    // Live Step hit count and chain analysis
    let analysis = RuleEngine::analyze_chain(&patchpanel, &steps, &all_tables);
    assert_eq!(analysis.total_matched_rows, 2);
    assert_eq!(analysis.relationship, JoinRelationshipType::OneToOne);
    assert!(analysis.relationship_badge.contains("1:1"));

    // Virtual columns
    let v_cols = vec![
        VirtualColumnMapping {
            remote_column_concept: "Building".to_string(),
            virtual_column_header: "Gebäude".to_string(),
            virtual_column_key: "v_first_building".to_string(),
            multi_result_display: "First".to_string(),
            brush_key: Some("PositiveBrush".to_string()),
            priority: Some(5),
        },
    ];

    let rule = RuleDefinition::VirtualRowJoin {
        id: "Join_Gebaeude".to_string(),
        description: "Patchpanel zu Gebäude via Switch".to_string(),
        action_group: Some("Netzwerk".to_string()),
        is_active: true,
        is_highlighted: true,
        key_concept: "SwitchID".to_string(),
        target_table: "switches.csv".to_string(),
        target_key_concept: "SwitchID".to_string(),
        import_concepts: vec!["Building".to_string()],
        injected_column_names: Vec::new(),
        link_steps: steps.clone(),
        virtual_columns: v_cols.clone(),
    };

    patchpanel.rules.push(rule);
    let res = RuleEngine::execute(&mut patchpanel, &all_tables);
    assert_eq!(res.summaries[0].hit_count, 2);

    // Verify injected column
    let bld_col_idx = patchpanel.headers.iter().position(|h| h == "Gebäude").expect("Spalte 'Gebäude' sollte existieren");
    assert_eq!(patchpanel.rows[0][bld_col_idx], "Gebäude A");
    assert_eq!(patchpanel.rows[1][bld_col_idx], "Gebäude B");
    assert_eq!(patchpanel.rows[2][bld_col_idx], "");

    // Autotext generation
    let autotext = VirtualColumnMapping::generate_autotext_description("Join_Gebaeude", true, Some("Netzwerk"), "patchpanel.csv", &steps, &v_cols);
    assert!(autotext.contains("Automatisierte Join-Beschreibung"));
    assert!(autotext.contains("switches.csv"));
    assert!(autotext.contains("rooms.csv"));
}
