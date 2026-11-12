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
    assert_eq!(config.selected_storage_mode, InfoFileStorageMode::KollektionsBasispfad);
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

    // Check external apps default configuration
    assert_eq!(config.external_apps.len(), 2);
    assert_eq!(config.external_apps[0].name, "LibreOffice");
    assert_eq!(config.external_apps[1].name, "Notepad++");
    assert!(config.external_apps[0].command_or_path.contains("soffice"));
    assert!(config.external_apps[1].command_or_path.contains("notepad++"));
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

#[test]
fn test_create_and_load_ods() {
    use configurable_csv_analyzer::core::io::ods::{create_ods_file, list_ods_sheets, load_ods_sheet};

    let tmp_path = std::env::temp_dir().join("test_created_analyzer.ods");
    let _ = std::fs::remove_file(&tmp_path);

    let mut t1 = TableData::new("t1".into(), "Switches.csv".into());
    t1.headers = vec!["Device".into(), "IP".into()];
    t1.rows = vec![
        vec!["SW-01".into(), "10.0.0.1".into()],
        vec!["SW-02".into(), "10.0.0.2".into()],
    ];

    create_ods_file(&tmp_path, &[t1]).expect("create_ods_file sollte erfolgreich sein");
    assert!(tmp_path.exists());

    let sheets = list_ods_sheets(&tmp_path).expect("list_ods_sheets sollte erfolgreich sein");
    assert_eq!(sheets, vec!["Switches"]);

    let loaded = load_ods_sheet(&tmp_path, Some("Switches")).expect("load_ods_sheet sollte erfolgreich sein");
    assert_eq!(loaded.headers, vec!["Device", "IP"]);
    assert_eq!(loaded.rows.len(), 2);
    assert_eq!(loaded.rows[0], vec!["SW-01", "10.0.0.1"]);

    let _ = std::fs::remove_file(&tmp_path);
}

#[test]
fn test_empty_0byte_ods() {
    use configurable_csv_analyzer::core::io::ods::{list_ods_sheets, load_ods_sheet};
    let tmp_0byte = std::env::temp_dir().join("test_empty_0byte.ods");
    let _ = std::fs::write(&tmp_0byte, b""); // Create a 0-byte file
    assert_eq!(std::fs::metadata(&tmp_0byte).unwrap().len(), 0);

    let sheets = list_ods_sheets(&tmp_0byte).expect("0-byte ODS must be auto-initialized");
    assert_eq!(sheets, vec!["Tabelle1"]);

    let table = load_ods_sheet(&tmp_0byte, None).expect("0-byte ODS must load without error");
    assert_eq!(table.headers, vec!["Spalte1", "Spalte2", "Spalte3"]);
    let _ = std::fs::remove_file(&tmp_0byte);
}

#[test]
fn test_filter_empty_rows_and_cross_filter_counts() {
    let mut table = TableData::new("t_filter".into(), "FilterTest".into());
    table.headers = vec!["Rack".into(), "Device".into()];
    table.rows = vec![
        vec!["R1".into(), "SW-01".into()],
        vec!["R1".into(), "".into()],
        vec!["R2".into(), "SW-02".into()],
        vec!["".into(), "SW-03".into()],
    ];

    // Filter column 0 (Rack) to only "R1" (no empty)
    let mut selected = std::collections::HashSet::new();
    selected.insert("R1".to_string());
    table.active_filters.insert(
        0,
        FilterCondition {
            selected_values: selected,
            all_selected: false,
            include_empty: false,
            include_non_empty: true,
            text_search: String::new(),
        },
    );

    // Visible rows should be 0 and 1 (row 3 has empty Rack and must be filtered out!)
    let vis = table.visible_row_indices(false);
    assert_eq!(vis, vec![0, 1]);

    // Cross-filter counts for column 1 (Device) considering column 0's active filter ("R1")
    let rows_for_col1 = table.rows_matching_other_filters(1, false, "");
    assert_eq!(rows_for_col1, vec![0, 1]);

    // Now also filter column 1 (Device) to exclude empty values
    let mut selected_dev = std::collections::HashSet::new();
    selected_dev.insert("SW-01".to_string());
    table.active_filters.insert(
        1,
        FilterCondition {
            selected_values: selected_dev,
            all_selected: false,
            include_empty: false,
            include_non_empty: true,
            text_search: String::new(),
        },
    );

    // Only row 0 should remain visible (row 1 has empty Device!)
    let vis_both = table.visible_row_indices(false);
    assert_eq!(vis_both, vec![0]);
}

#[test]
fn test_smart_add_dialog_logic_and_filter_visibility() {
    let mut table = TableData::new("t_smart".into(), "SmartTest".into());
    table.headers = vec![
        "ID".into(),
        "_Internal".into(),
        "OriginalRowNumber".into(),
        "Building".into(),
        "Room".into(),
        "Host".into(),
    ];
    table.is_virtual_col = vec![false, false, false, false, false, false];
    table.rows = vec![
        vec!["1".into(), "x".into(), "1".into(), "B1".into(), "101".into(), "srv-1".into()],
        vec!["2".into(), "x".into(), "2".into(), "B1".into(), "101".into(), "srv-2".into()],
        vec!["3".into(), "x".into(), "3".into(), "B2".into(), "202".into(), "srv-3".into()],
    ];

    // 1. Test with 2 marked rows (rows 0 and 1)
    table.marked_rows.insert(0);
    table.marked_rows.insert(1);

    let state_multi = build_smart_add_dialog_state(&table, false, "");
    // System columns ID, _Internal, OriginalRowNumber must be excluded -> only Building, Room, Host remain
    assert_eq!(state_multi.fields.len(), 3);
    assert_eq!(state_multi.fields[0].column_name, "Building");
    assert_eq!(state_multi.fields[1].column_name, "Room");
    assert_eq!(state_multi.fields[2].column_name, "Host");

    // Both marked rows have Building="B1" and Room="101", so they must be auto-prefilled; Host differs so ""
    assert_eq!(state_multi.fields[0].value, "B1");
    assert_eq!(state_multi.fields[1].value, "101");
    assert_eq!(state_multi.fields[2].value, "");

    // Because only a subset of rows is marked, suggestions must end with "... (Alle anzeigen)"
    assert!(state_multi.fields[0].suggestions.last().unwrap().is_load_more);

    // LoadAllSuggestionsForColumn loads all 3 rows (B1 (2), B2 (1)) without "... (Alle anzeigen)"
    let all_b_suggs = load_all_suggestions_for_column(&table, 3);
    assert_eq!(all_b_suggs.len(), 2);
    assert_eq!(all_b_suggs[0].display, "B1 (2)");
    assert_eq!(all_b_suggs[1].display, "B2 (1)");

    // 2. Test with 1 marked row (row 2) -> all non-empty values of row 2 are prefilled
    table.marked_rows.clear();
    table.marked_rows.insert(2);
    let state_single = build_smart_add_dialog_state(&table, false, "");
    assert_eq!(state_single.fields[0].value, "B2");
    assert_eq!(state_single.fields[1].value, "202");
    assert_eq!(state_single.fields[2].value, "srv-3");

    // 3. Test execute_smart_add_rows with active filter & EnsureRowIsVisibleInFilter
    let mut filter_b1 = std::collections::HashSet::new();
    filter_b1.insert("B1".to_string());
    table.active_filters.insert(
        3,
        FilterCondition {
            selected_values: filter_b1,
            all_selected: false,
            include_empty: false,
            include_non_empty: true,
            text_search: String::new(),
        },
    );

    // Add 3 new rows with Building="B3" (which was NOT in the active filter for Building!)
    let created = execute_smart_add_rows(
        &mut table,
        3,
        &[(3, "B3".to_string()), (4, "303".to_string()), (5, "srv-new".to_string())],
    );
    assert_eq!(created.len(), 3);
    assert_eq!(table.rows.len(), 6);

    // EnsureRowIsVisibleInFilter must have added "B3" to the active filter on column 3
    let vis_after = table.visible_row_indices(false);
    for &new_idx in &created {
        assert!(vis_after.contains(&new_idx), "Neue Zeile {} muss im aktiven Filter sichtbar sein", new_idx);
        assert!(table.dirty_cells.contains(&(new_idx, 3)));
    }
}

#[test]
fn test_multi_sheet_ods_unique_ids() {
    use configurable_csv_analyzer::core::io::ods::{create_ods_file, list_ods_sheets, load_ods_sheet};
    let dir = std::env::temp_dir().join("csv_analyzer_multi_ods_test");
    let _ = std::fs::create_dir_all(&dir);
    let ods_path = dir.join("multi_test.ods");

    let mut t1 = TableData::new("t1".into(), "Tabelle A".into());
    t1.headers = vec!["Col1".into(), "Col2".into()];
    t1.rows = vec![vec!["A1".into(), "A2".into()]];

    let mut t2 = TableData::new("t2".into(), "Tabelle B".into());
    t2.headers = vec!["X".into(), "Y".into()];
    t2.rows = vec![vec!["B1".into(), "B2".into()]];

    create_ods_file(&ods_path, &[t1, t2]).unwrap();

    let sheets = list_ods_sheets(&ods_path).unwrap();
    assert_eq!(sheets.len(), 2);
    assert_eq!(sheets[0], "Tabelle A");
    assert_eq!(sheets[1], "Tabelle B");

    let loaded_a = load_ods_sheet(&ods_path, Some(&sheets[0])).unwrap();
    let loaded_b = load_ods_sheet(&ods_path, Some(&sheets[1])).unwrap();

    // Verify unique IDs
    assert_ne!(loaded_a.id, loaded_b.id);
    assert!(loaded_a.id.ends_with("::Tabelle A"));
    assert!(loaded_b.id.ends_with("::Tabelle B"));
    assert_eq!(loaded_a.headers, vec!["Col1", "Col2"]);
    assert_eq!(loaded_b.headers, vec!["X", "Y"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_relocate_all_and_migrate_to_selected_mode() {
    use configurable_csv_analyzer::core::io::info::move_info_file;
    use configurable_csv_analyzer::core::model::{InfoFileStorageMode, ViewDefinition, ViewFileItem};

    let dir = std::env::temp_dir().join("test_relocate_all");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let csv_dir = dir.join("csv_files");
    let base_dir = dir.join("collection_base");
    let central_dir = dir.join("central_configs");
    std::fs::create_dir_all(&csv_dir).unwrap();
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::create_dir_all(&central_dir).unwrap();

    // Create a CSV and an info file in central
    let csv1 = csv_dir.join("table1.csv");
    std::fs::write(&csv1, "a,b\n1,2\n").unwrap();
    let info1 = central_dir.join("table1.csv.info");
    std::fs::write(&info1, "{}").unwrap();

    // Create a CSV without an info file
    let csv2 = csv_dir.join("table2.csv");
    std::fs::write(&csv2, "x,y\n3,4\n").unwrap();

    let mut view = ViewDefinition {
        name: "TestView".to_string(),
        is_default_view: true,
        files: vec![
            ViewFileItem {
                relative_path: csv1.to_string_lossy().to_string(),
                full_path: Some(csv1.to_string_lossy().to_string()),
                info_file_path: Some(info1.to_string_lossy().to_string()),
                storage_mode: Some("Stammverzeichnis".to_string()),
            },
            ViewFileItem {
                relative_path: csv2.to_string_lossy().to_string(),
                full_path: Some(csv2.to_string_lossy().to_string()),
                info_file_path: None,
                storage_mode: Some("Dateipfad".to_string()),
            },
        ],
        spreadsheet_path: None,
        base_path: Some(base_dir.to_string_lossy().to_string()),
        color_pattern: None,
        export_color_pattern: None,
        export_paths: vec![],
        column_widths: Default::default(),
    };

    // Simulate "Alle in Basispfad verschieben":
    let target_mode = InfoFileStorageMode::KollektionsBasispfad;
    for f in view.files.iter_mut() {
        let csv_p = std::path::PathBuf::from(&f.relative_path);
        let f_name = csv_p.file_name().unwrap().to_string_lossy();
        let expected = base_dir.join(format!("{}.info", f_name));

        let current_p = f
            .info_file_path
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                configurable_csv_analyzer::core::io::info::default_info_path_for_table(&csv_p)
            });

        if current_p.exists() && current_p != expected {
            let _ = move_info_file(&csv_p, &current_p, &target_mode, &central_dir, Some(&base_dir));
        }

        f.info_file_path = Some(expected.to_string_lossy().to_string());
        f.storage_mode = Some(target_mode.as_str().to_string());
    }

    // Verify file 1 was moved to base_dir
    assert!(base_dir.join("table1.csv.info").exists());
    assert!(!central_dir.join("table1.csv.info").exists());
    assert_eq!(view.files[0].storage_mode, Some("KollektionsBasispfad".to_string()));

    // Verify file 2 has target expected path and KollektionsBasispfad mode set even though info didn't exist yet
    assert_eq!(view.files[1].storage_mode, Some("KollektionsBasispfad".to_string()));
    assert_eq!(
        view.files[1].info_file_path,
        Some(base_dir.join("table2.csv.info").to_string_lossy().to_string())
    );

    let _ = std::fs::remove_dir_all(&dir);
}


