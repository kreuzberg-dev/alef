//! R visitor callback rendering.

use crate::e2e::escape::{escape_r, r_template_to_paste0};
use crate::e2e::fixture::{CallbackAction, TemplateReturnForm};

/// Build an R visitor list and add setup line.
pub(super) fn build_r_visitor(setup_lines: &mut Vec<String>, visitor_spec: &crate::e2e::fixture::VisitorSpec) {
    use std::fmt::Write as FmtWrite;
    // Collect each callback as a separate string, then join with ",\n" to avoid
    // trailing commas — R's list() does not accept a trailing comma.
    let methods: Vec<String> = visitor_spec
        .callbacks
        .iter()
        .map(|(method_name, action)| {
            let mut buf = String::new();
            emit_r_visitor_method(&mut buf, method_name, action);
            // strip the trailing ",\n" added by emit_r_visitor_method
            buf.trim_end_matches(['\n', ',']).to_string()
        })
        .collect();
    let mut visitor_obj = String::new();
    let _ = writeln!(visitor_obj, "list(");
    let _ = write!(visitor_obj, "{}", methods.join(",\n"));
    let _ = writeln!(visitor_obj);
    let _ = writeln!(visitor_obj, "  )");

    setup_lines.push(format!("visitor <- {visitor_obj}"));
}

/// Emit an R visitor method for a callback action.
fn emit_r_visitor_method(out: &mut String, method_name: &str, action: &CallbackAction) {
    use std::fmt::Write as FmtWrite;

    // R uses visit_ prefix (matches binding signature)
    let params = match method_name {
        "visit_link" => "ctx, href, text, title",
        "visit_image" => "ctx, src, alt, title",
        "visit_heading" => "ctx, level, text, id",
        "visit_code_block" => "ctx, lang, code",
        "visit_code_inline"
        | "visit_strong"
        | "visit_emphasis"
        | "visit_strikethrough"
        | "visit_underline"
        | "visit_subscript"
        | "visit_superscript"
        | "visit_mark"
        | "visit_button"
        | "visit_summary"
        | "visit_figcaption"
        | "visit_definition_term"
        | "visit_definition_description" => "ctx, text",
        "visit_text" => "ctx, text",
        "visit_list_item" => "ctx, ordered, marker, text",
        "visit_blockquote" => "ctx, content, depth",
        "visit_table_row" => "ctx, cells, is_header",
        "visit_custom_element" => "ctx, tag_name, html",
        "visit_form" => "ctx, action_url, method",
        "visit_input" => "ctx, input_type, name, value",
        "visit_audio" | "visit_video" | "visit_iframe" => "ctx, src",
        "visit_details" => "ctx, open",
        "visit_element_end" | "visit_table_end" | "visit_definition_list_end" | "visit_figure_end" => "ctx, output",
        "visit_list_start" => "ctx, ordered",
        "visit_list_end" => "ctx, ordered, output",
        _ => "ctx",
    };

    let _ = writeln!(out, "    {method_name} = function({params}) {{");
    match action {
        CallbackAction::Skip => {
            let _ = writeln!(out, "      \"Skip\"");
        }
        CallbackAction::Continue => {
            let _ = writeln!(out, "      \"Continue\"");
        }
        CallbackAction::PreserveHtml => {
            let _ = writeln!(out, "      \"PreserveHtml\"");
        }
        CallbackAction::Custom { output } => {
            let escaped = escape_r(output);
            let _ = writeln!(out, "      list(Custom = \"{escaped}\")");
        }
        CallbackAction::CustomTemplate { template, return_form } => {
            let r_expr = r_template_to_paste0(template);
            match return_form {
                TemplateReturnForm::BareString => {
                    let _ = writeln!(out, "      {r_expr}");
                }
                TemplateReturnForm::Dict => {
                    let _ = writeln!(out, "      list(Custom = {r_expr})");
                }
            }
        }
    }
    let _ = writeln!(out, "    }},");
}
