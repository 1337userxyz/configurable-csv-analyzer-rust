use std::collections::{HashMap, HashSet};
use crate::core::model::{AppConfig, InfoFile, RuleDefinition, TableData, ValidationIssue, ValidationSeverity};

pub struct TableConfigValidator;

impl TableConfigValidator {
    pub fn validate(
        info: &InfoFile,
        table: &TableData,
        all_tables: &HashMap<String, TableData>,
        app_config: &AppConfig,
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // 1. Regel-Eindeutigkeit
        let mut seen_ids = HashSet::new();
        for rule in &info.rules {
            let id = rule.id().trim();
            if id.is_empty() {
                issues.push(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    tab_index: Self::rule_to_tab_index(rule),
                    field: "RuleId".to_string(),
                    message: "Regel-ID darf nicht leer sein.".to_string(),
                });
            } else if !seen_ids.insert(id.to_string()) {
                issues.push(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    tab_index: Self::rule_to_tab_index(rule),
                    field: format!("RuleId:{}", id),
                    message: format!("Doppelte Regel-ID '{}' gefunden.", id),
                });
            }
        }

        // 2. Konzept-Integrität & Spalten-Mappings (Tab 2)
        for (concept, csv_col) in &info.column_mappings {
            if concept.trim().is_empty() {
                issues.push(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    tab_index: 2,
                    field: "ConceptName".to_string(),
                    message: "Konzeptname darf nicht leer sein.".to_string(),
                });
            }
            if csv_col.trim().is_empty() {
                issues.push(ValidationIssue {
                    severity: ValidationSeverity::Error,
                    tab_index: 2,
                    field: format!("Mapping:{}", concept),
                    message: format!("Keine CSV-Spalte für Konzept '{}' zugewiesen.", concept),
                });
            } else if !table.headers.is_empty() && !table.headers.iter().any(|h| h.eq_ignore_ascii_case(csv_col)) {
                issues.push(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    tab_index: 2,
                    field: format!("Mapping:{}", concept),
                    message: format!("Gemappte CSV-Spalte '{}' existiert nicht in den Kopfzeilen.", csv_col),
                });
            }
        }

        // Helper to check if a concept or column exists
        let concept_or_col_exists = |c: &str| -> bool {
            let trimmed = c.trim();
            if trimmed.is_empty() {
                return false;
            }
            info.column_mappings.contains_key(trimmed)
                || table.headers.iter().any(|h| h.eq_ignore_ascii_case(trimmed))
        };

        // 3. Validierung einzelner Regeln
        for rule in &info.rules {
            let tab_idx = Self::rule_to_tab_index(rule);
            match rule {
                RuleDefinition::BulkField { id, concept, .. } => {
                    if !concept_or_col_exists(concept) {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            tab_index: tab_idx,
                            field: format!("{}:concept", id),
                            message: format!("Konzept '{}' in Regel '{}' ist nicht definiert.", concept, id),
                        });
                    }
                }
                RuleDefinition::Calculation { id, target_column, formula, .. } => {
                    if target_column.trim().is_empty() {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            tab_index: tab_idx,
                            field: format!("{}:target_column", id),
                            message: format!("Zielspalte für Berechnungsregel '{}' fehlt.", id),
                        });
                    }
                    if formula.trim().is_empty() {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            tab_index: tab_idx,
                            field: format!("{}:formula", id),
                            message: format!("Formel in Berechnungsregel '{}' darf nicht leer sein.", id),
                        });
                    } else {
                        // Validate syntax & braced concepts
                        let (syntax_ok, err_msg) = Self::validate_formula_syntax(formula, &concept_or_col_exists);
                        if !syntax_ok {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Error,
                                tab_index: tab_idx,
                                field: format!("{}:formula", id),
                                message: format!("Formelfehler in '{}': {}", id, err_msg),
                            });
                        }
                    }
                }
                RuleDefinition::GroupValidation {
                    id,
                    group_by_concept,
                    unique_concept,
                    numeric_range,
                    group_consistency,
                    mismatch_brush_key,
                    ..
                } => {
                    if !concept_or_col_exists(group_by_concept) {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            tab_index: tab_idx,
                            field: format!("{}:group_by_concept", id),
                            message: format!("Gruppierungskonzept '{}' in '{}' ist nicht definiert.", group_by_concept, id),
                        });
                    }
                    if let Some(uc) = unique_concept {
                        if !concept_or_col_exists(uc) {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                tab_index: tab_idx,
                                field: format!("{}:unique_concept", id),
                                message: format!("Eindeutigkeitskonzept '{}' in '{}' ist nicht definiert.", uc, id),
                            });
                        }
                    }
                    if let Some(nr) = numeric_range {
                        if !concept_or_col_exists(&nr.concept) {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                tab_index: tab_idx,
                                field: format!("{}:numeric_range", id),
                                message: format!("Bereichskonzept '{}' in '{}' ist nicht definiert.", nr.concept, id),
                            });
                        }
                    }
                    if let Some(gc) = group_consistency {
                        if !concept_or_col_exists(&gc.concept) {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                tab_index: tab_idx,
                                field: format!("{}:group_consistency", id),
                                message: format!("Konsistenzkonzept '{}' in '{}' ist nicht definiert.", gc.concept, id),
                            });
                        }
                    }
                    if let Some(bkey) = mismatch_brush_key {
                        if !bkey.is_empty() && app_config.find_brush_hex(bkey).is_none() {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                tab_index: tab_idx,
                                field: format!("{}:mismatch_brush_key", id),
                                message: format!("Farbe '{}' existiert nicht im Farbkatalog.", bkey),
                            });
                        }
                    }
                }
                RuleDefinition::VirtualRowJoin {
                    id,
                    key_concept,
                    target_table,
                    target_key_concept,
                    import_concepts,
                    link_steps,
                    virtual_columns,
                    ..
                } => {
                    if !link_steps.is_empty() {
                        let mut current_intermediate_table = table;
                        for (s_idx, step) in link_steps.iter().enumerate() {
                            let tgt_tbl = crate::core::rules::RuleEngine::resolve_table(&step.target_table_key, table, all_tables);
                            if tgt_tbl.is_none() {
                                issues.push(ValidationIssue {
                                    severity: ValidationSeverity::Warning,
                                    tab_index: tab_idx,
                                    field: format!("{}:step_{}:target_table", id, s_idx + 1),
                                    message: format!("Zieltabelle '{}' in Schritt {} ist aktuell nicht geöffnet.", step.target_table_key, s_idx + 1),
                                });
                            }

                            if step.key_mappings.is_empty() {
                                issues.push(ValidationIssue {
                                    severity: ValidationSeverity::Error,
                                    tab_index: tab_idx,
                                    field: format!("{}:step_{}:conditions", id, s_idx + 1),
                                    message: format!("Schritt {} in '{}' benötigt mindestens eine Bedingung.", s_idx + 1, id),
                                });
                            }

                            for (m_idx, mapping) in step.key_mappings.iter().enumerate() {
                                if mapping.manual_remote_value.is_none() && mapping.host_concept.trim().is_empty() {
                                    issues.push(ValidationIssue {
                                        severity: ValidationSeverity::Error,
                                        tab_index: tab_idx,
                                        field: format!("{}:step_{}:condition_{}:host", id, s_idx + 1, m_idx + 1),
                                        message: format!("Quellkonzept in Schritt {} Bedingung {} fehlt.", s_idx + 1, m_idx + 1),
                                    });
                                } else if mapping.manual_remote_value.is_none() {
                                    if current_intermediate_table.find_col_index(&mapping.host_concept).is_none() {
                                        issues.push(ValidationIssue {
                                            severity: ValidationSeverity::Error,
                                            tab_index: tab_idx,
                                            field: format!("{}:step_{}:condition_{}:host", id, s_idx + 1, m_idx + 1),
                                            message: format!("Quellkonzept '{}' in Schritt {} existiert nicht.", mapping.host_concept, s_idx + 1),
                                        });
                                    }
                                }

                                if mapping.remote_concept.trim().is_empty() {
                                    issues.push(ValidationIssue {
                                        severity: ValidationSeverity::Error,
                                        tab_index: tab_idx,
                                        field: format!("{}:step_{}:condition_{}:remote", id, s_idx + 1, m_idx + 1),
                                        message: format!("Zielkonzept in Schritt {} Bedingung {} fehlt.", s_idx + 1, m_idx + 1),
                                    });
                                } else if let Some(target) = tgt_tbl {
                                    if target.find_col_index(&mapping.remote_concept).is_none() {
                                        issues.push(ValidationIssue {
                                            severity: ValidationSeverity::Error,
                                            tab_index: tab_idx,
                                            field: format!("{}:step_{}:condition_{}:remote", id, s_idx + 1, m_idx + 1),
                                            message: format!("Zielkonzept '{}' existiert nicht in Zieltabelle '{}'.", mapping.remote_concept, step.target_table_key),
                                        });
                                    }
                                }
                            }

                            if let Some(target) = tgt_tbl {
                                current_intermediate_table = target;
                            }
                        }

                        let mut seen_keys = std::collections::HashSet::new();
                        for (c_idx, col) in virtual_columns.iter().enumerate() {
                            let k = col.virtual_column_key.trim();
                            if k.is_empty() {
                                issues.push(ValidationIssue {
                                    severity: ValidationSeverity::Error,
                                    tab_index: tab_idx,
                                    field: format!("{}:virtual_col_{}:key", id, c_idx + 1),
                                    message: format!("Bindungs-Key für virtuelle Spalte '{}' fehlt.", col.virtual_column_header),
                                });
                            } else {
                                let is_valid_ident = k.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
                                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                                if !is_valid_ident {
                                    issues.push(ValidationIssue {
                                        severity: ValidationSeverity::Error,
                                        tab_index: tab_idx,
                                        field: format!("{}:virtual_col_{}:key", id, c_idx + 1),
                                        message: format!("Bindungs-Key '{}' ist kein gültiger Identifier (nur a-z, 0-9, _).", k),
                                    });
                                }
                                if !seen_keys.insert(k.to_lowercase()) {
                                    issues.push(ValidationIssue {
                                        severity: ValidationSeverity::Error,
                                        tab_index: tab_idx,
                                        field: format!("{}:virtual_col_{}:key", id, c_idx + 1),
                                        message: format!("Bindungs-Key '{}' ist nicht eindeutig.", k),
                                    });
                                }
                            }
                        }
                    } else {
                        // Legacy single-step fallback
                        if !concept_or_col_exists(key_concept) {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Error,
                                tab_index: tab_idx,
                                field: format!("{}:key_concept", id),
                                message: format!("Schlüsselkonzept '{}' in '{}' ist nicht definiert.", key_concept, id),
                            });
                        }
                        if let Some(tgt_tbl) = all_tables.get(target_table) {
                            if tgt_tbl.find_col_index(target_key_concept).is_none() {
                                issues.push(ValidationIssue {
                                    severity: ValidationSeverity::Error,
                                    tab_index: tab_idx,
                                    field: format!("{}:target_key_concept", id),
                                    message: format!("Ziel-Schlüssel '{}' existiert nicht in Zieltabelle '{}'.", target_key_concept, target_table),
                                });
                            }
                            for imp in import_concepts {
                                if tgt_tbl.find_col_index(imp).is_none() {
                                    issues.push(ValidationIssue {
                                        severity: ValidationSeverity::Warning,
                                        tab_index: tab_idx,
                                        field: format!("{}:import_concepts", id),
                                        message: format!("Import-Feld '{}' existiert nicht in Zieltabelle '{}'.", imp, target_table),
                                    });
                                }
                            }
                        } else if !target_table.trim().is_empty() {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                tab_index: tab_idx,
                                field: format!("{}:target_table", id),
                                message: format!("Zieltabelle '{}' ist aktuell nicht geöffnet.", target_table),
                            });
                        }
                    }
                }
                RuleDefinition::CrossTableComparison {
                    id,
                    key_concept,
                    target_table,
                    target_key_concept,
                    mismatch_brush_key,
                    ..
                } => {
                    if !concept_or_col_exists(key_concept) {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            tab_index: tab_idx,
                            field: format!("{}:key_concept", id),
                            message: format!("Schlüsselkonzept '{}' in '{}' ist nicht definiert.", key_concept, id),
                        });
                    }
                    if let Some(tgt_tbl) = all_tables.get(target_table) {
                        if tgt_tbl.find_col_index(target_key_concept).is_none() {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Error,
                                tab_index: tab_idx,
                                field: format!("{}:target_key_concept", id),
                                message: format!("Ziel-Schlüssel '{}' existiert nicht in Zieltabelle '{}'.", target_key_concept, target_table),
                            });
                        }
                    } else if !target_table.trim().is_empty() {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Warning,
                            tab_index: tab_idx,
                            field: format!("{}:target_table", id),
                            message: format!("Zieltabelle '{}' ist aktuell nicht geöffnet.", target_table),
                        });
                    }
                    if let Some(bkey) = mismatch_brush_key {
                        if !bkey.is_empty() && app_config.find_brush_hex(bkey).is_none() {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                tab_index: tab_idx,
                                field: format!("{}:mismatch_brush_key", id),
                                message: format!("Farbe '{}' existiert nicht im Farbkatalog.", bkey),
                            });
                        }
                    }
                }
                RuleDefinition::Highlight {
                    id,
                    column_concept,
                    brush_key,
                    ..
                } => {
                    if !concept_or_col_exists(column_concept) {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Error,
                            tab_index: tab_idx,
                            field: format!("{}:column_concept", id),
                            message: format!("Konzept/Spalte '{}' in '{}' existiert nicht.", column_concept, id),
                        });
                    }
                    if !brush_key.is_empty() && app_config.find_brush_hex(brush_key).is_none() {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Warning,
                            tab_index: tab_idx,
                            field: format!("{}:brush_key", id),
                            message: format!("Farbe '{}' existiert nicht im Farbkatalog.", brush_key),
                        });
                    }
                }
                RuleDefinition::Apply {
                    id,
                    source_column_concepts,
                    target_table,
                    ..
                } => {
                    for src in source_column_concepts {
                        if !concept_or_col_exists(src) {
                            issues.push(ValidationIssue {
                                severity: ValidationSeverity::Warning,
                                tab_index: tab_idx,
                                field: format!("{}:source_concept", id),
                                message: format!("Quellkonzept '{}' in '{}' existiert nicht.", src, id),
                            });
                        }
                    }
                    if target_table != "$self" && !all_tables.contains_key(target_table) && !target_table.trim().is_empty() {
                        issues.push(ValidationIssue {
                            severity: ValidationSeverity::Warning,
                            tab_index: tab_idx,
                            field: format!("{}:target_table", id),
                            message: format!("Zieltabelle '{}' ist aktuell nicht geöffnet.", target_table),
                        });
                    }
                }
            }
        }

        // 4. DirtyCellBrushKey Check (Tab 0)
        if let Some(ref bkey) = info.dirty_cell_brush_key {
            if !bkey.is_empty() && app_config.find_brush_hex(bkey).is_none() {
                issues.push(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    tab_index: 0,
                    field: "DirtyCellBrushKey".to_string(),
                    message: format!("DirtyCellBrush '{}' existiert nicht im Farbkatalog.", bkey),
                });
            }
        }

        issues
    }

    pub fn has_errors(issues: &[ValidationIssue]) -> bool {
        issues.iter().any(|i| i.severity == ValidationSeverity::Error)
    }

    pub fn validate_formula_syntax<F>(formula: &str, concept_checker: &F) -> (bool, String)
    where
        F: Fn(&str) -> bool,
    {
        let mut clean = formula.to_string();

        // Find braced placeholders {Concept}
        let mut start_idx = None;
        let mut placeholders = Vec::new();

        for (i, c) in formula.char_indices() {
            if c == '{' {
                start_idx = Some(i);
            } else if c == '}' {
                if let Some(s) = start_idx {
                    let concept = &formula[s + 1..i];
                    placeholders.push(concept.to_string());
                    start_idx = None;
                }
            }
        }

        // Check braced placeholders exist
        for ph in &placeholders {
            if !concept_checker(ph) {
                return (false, format!("Unbekannter Platzhalter '{{{}}}'", ph));
            }
            clean = clean.replace(&format!("{{{}}}", ph), "1");
        }

        // Substitute raw identifiers with dummy number 1 if they match concept_checker
        // But for evalexpr syntax testing, let's just parse with evalexpr
        match evalexpr::build_operator_tree::<evalexpr::DefaultNumericTypes>(&clean) {
            Ok(_) => (true, "Gültig".to_string()),
            Err(e) => (false, e.to_string()),
        }
    }

    fn rule_to_tab_index(rule: &RuleDefinition) -> usize {
        match rule {
            RuleDefinition::BulkField { .. } => 0,
            RuleDefinition::Calculation { .. } => 3,
            RuleDefinition::VirtualRowJoin { .. } => 4,
            RuleDefinition::CrossTableComparison { .. } => 5,
            RuleDefinition::GroupValidation { .. } => 6,
            RuleDefinition::Highlight { .. } => 7,
            RuleDefinition::Apply { .. } => 8,
        }
    }
}
