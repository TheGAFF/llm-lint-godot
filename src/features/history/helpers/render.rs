use super::super::models::{Group, Report, Table};
use crate::shared::models::Outcome;
use std::{
    fmt::Write as _,
    fs::OpenOptions,
    io::{self, Write as _},
    path::Path,
};

const REPORT_NOTE: &str = "Pass rates use completed runs. Baseline comparisons show observed changes during recorded work.";

impl Report {
    pub(crate) fn print(&self) -> Outcome<()> {
        let mut output = String::from("Lint history\n");
        if self.groups.is_empty() {
            writeln!(output, "No matching runs.")?;
        } else {
            writeln!(output, "{REPORT_NOTE}")?;
        }
        for group in &self.groups {
            writeln!(output, "\n{}", terminal_text(&group_title(group)))?;
            for table in &group.tables {
                terminal_table(&mut output, table)?;
            }
            if !self.detail {
                continue;
            }
            for run in &group.runs {
                terminal_table(&mut output, &run.detail())?;
            }
        }
        io::stdout().lock().write_all(output.as_bytes())?;
        Ok(())
    }

    pub(crate) fn export(&self, path: &Path) -> Outcome<()> {
        let content = self.html()?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    fn html(&self) -> Outcome<String> {
        let mut output = String::from(include_str!("report_header.html"));
        if self.groups.is_empty() {
            output.push_str("<p>No matching runs.</p>\n");
        } else {
            writeln!(output, "<p>{REPORT_NOTE}</p>")?;
        }
        for group in &self.groups {
            writeln!(output, "<section><h2>{}</h2>", escaped(&group_title(group)))?;
            for table in &group.tables {
                html_table(&mut output, table)?;
            }
            for run in &group.runs {
                let title = format!("Run {} · {} · {}", run.id, run.model, run.status());
                writeln!(output, "<details><summary>{}</summary>", escaped(&title))?;
                html_table(&mut output, &run.detail())?;
                output.push_str("</details>\n");
            }
            output.push_str("</section>\n");
        }
        output.push_str("</main></body></html>\n");
        Ok(output)
    }
}

fn group_title(group: &Group) -> String {
    let policy: String = group.policy.chars().take(12).collect();
    format!("Linter {} · Policy {policy}", group.version)
}

fn terminal_table(output: &mut String, table: &Table) -> std::fmt::Result {
    writeln!(output, "\n{}", table.title)?;
    if table.rows.is_empty() {
        return writeln!(output, "  No entries.");
    }
    let rows: Vec<Vec<_>> = table
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| terminal_text(cell)).collect())
        .collect();
    let mut widths: Vec<_> = table
        .columns
        .iter()
        .map(|label| label.chars().count())
        .collect();
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    terminal_row(output, table.columns.iter().copied(), &widths)?;
    for row in &rows {
        terminal_row(output, row.iter().map(String::as_str), &widths)?;
    }
    Ok(())
}

fn terminal_row<'a>(
    output: &mut String,
    cells: impl Iterator<Item = &'a str>,
    widths: &[usize],
) -> std::fmt::Result {
    let padded: Vec<_> = cells
        .zip(widths)
        .map(|(cell, width)| format!("{cell:width$}"))
        .collect();
    writeln!(output, "{}", padded.join("  ").trim_end())
}

fn terminal_text(text: &str) -> String {
    let mut safe = String::new();
    for ch in text.chars() {
        if ch.is_control() {
            safe.extend(ch.escape_default());
        } else {
            safe.push(ch);
        }
    }
    safe
}

fn html_table(output: &mut String, table: &Table) -> std::fmt::Result {
    if table.rows.is_empty() {
        return writeln!(output, "<p>{} · No entries.</p>", table.title);
    }
    writeln!(
        output,
        "<div class=\"table\"><table><caption>{}</caption><thead><tr>",
        table.title
    )?;
    for header in table.columns {
        write!(output, "<th scope=\"col\">{header}</th>")?;
    }
    output.push_str("</tr></thead><tbody>\n");
    for row in &table.rows {
        output.push_str("<tr>");
        for cell in row {
            write!(output, "<td>{}</td>", escaped(cell))?;
        }
        output.push_str("</tr>\n");
    }
    output.push_str("</tbody></table></div>\n");
    Ok(())
}

fn escaped(text: &str) -> String {
    let mut safe = String::new();
    for ch in text.chars() {
        match ch {
            '&' => safe.push_str("&amp;"),
            '<' => safe.push_str("&lt;"),
            '>' => safe.push_str("&gt;"),
            '"' => safe.push_str("&quot;"),
            '\'' => safe.push_str("&#39;"),
            _ => safe.push(ch),
        }
    }
    safe
}
