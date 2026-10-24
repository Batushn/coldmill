//! Spreadsheets and structured data, converted in-process.
//!
//! Two shapes cover everything here. A **workbook** is named sheets of rows —
//! xlsx, xls, ods, csv. A **tree** is nested values — json, yaml, toml. Every
//! conversion reads one of the two, crosses over if the target is the other
//! shape, and writes. Nothing is downloaded and LibreOffice is not involved,
//! which is the point: turning a spreadsheet into a CSV used to need a system
//! office suite for what is, underneath, a loop over cells.
//!
//! Crossing between the shapes follows what people expect from the tools they
//! already use:
//!
//! * a sheet whose first row is a clean header becomes a list of records,
//!   anything else a list of rows;
//! * a list of records becomes a sheet with the keys as its header, and an
//!   object whose every value is such a list becomes one sheet per key — so a
//!   workbook taken to JSON and back keeps its sheets.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Number, Value};

/// Binary workbooks, read with calamine.
pub const WORKBOOK_INPUTS: &[&str] = &["xlsx", "xlsm", "xlsb", "xls", "ods"];

/// Plain-text inputs. These carry no signature, so detection reaches them by
/// extension once the bytes are known to be text.
pub const TEXT_INPUTS: &[&str] = &["csv", "tsv", "json", "yaml", "yml", "toml"];

/// What can always be written, with nothing installed.
pub const TARGETS: &[&str] = &["xlsx", "csv", "tsv", "json", "yaml", "toml", "md", "html"];

/// Written by pandoc from an HTML rendering of the table, when the document
/// module is installed.
pub const PANDOC_TARGETS: &[&str] = &["pdf", "docx", "odt"];

/// Written by LibreOffice, when the machine has it. It keeps a workbook's own
/// page setup in a PDF, which a table re-typeset by pandoc cannot.
pub const OFFICE_TARGETS: &[&str] = &["pdf", "ods"];

/// One file per sheet: these formats have no way to hold more than one table.
const SINGLE_TABLE_TARGETS: &[&str] = &["csv", "tsv"];

/// Excel's own ceilings. Writing past them produces a file Excel refuses to
/// open, so it is better to stop and say so.
const XLSX_MAX_ROWS: usize = 1_048_576;
const XLSX_MAX_COLUMNS: usize = 16_384;

pub fn is_input(extension: &str) -> bool {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    WORKBOOK_INPUTS.contains(&extension.as_str()) || TEXT_INPUTS.contains(&extension.as_str())
}

/// Whether LibreOffice can open this source as a spreadsheet. A JSON file is
/// not one, whatever its contents.
pub fn office_can_open(extension: &str) -> bool {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    WORKBOOK_INPUTS.contains(&extension.as_str()) || matches!(extension.as_str(), "csv" | "tsv")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    Workbook(Vec<Sheet>),
    Tree(Value),
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

#[cfg(test)]
pub fn read(path: &Path) -> Result<Data, String> {
    read_as(path, true)
}

/// `typed` decides whether a CSV's numbers and booleans are read as such. A
/// CSV going to another CSV, or to a table on a page, keeps its text exactly
/// as written — `1.50` stays `1.50` — and only formats that have types get
/// them.
fn read_as(path: &Path, typed: bool) -> Result<Data, String> {
    let extension = extension_of(path);
    if WORKBOOK_INPUTS.contains(&extension.as_str()) {
        return read_workbook(path).map(Data::Workbook);
    }

    let text = read_text(path)?;
    match extension.as_str() {
        "csv" => Ok(Data::Workbook(vec![read_delimited(
            &text,
            None,
            &stem_of(path),
            typed,
        )?])),
        "tsv" => Ok(Data::Workbook(vec![read_delimited(
            &text,
            Some(b'\t'),
            &stem_of(path),
            typed,
        )?])),
        "json" => serde_json::from_str(&text)
            .map(Data::Tree)
            .map_err(|e| format!("not valid JSON: {e}")),
        "yaml" | "yml" => serde_yaml_ng::from_str(&text)
            .map(Data::Tree)
            .map_err(|e| format!("not valid YAML: {e}")),
        "toml" => text
            .parse::<toml::Table>()
            .map(|table| Data::Tree(from_toml(toml::Value::Table(table))))
            .map_err(|e| format!("not valid TOML: {e}")),
        other => Err(format!(".{other} is not a data format")),
    }
}

fn read_workbook(path: &Path) -> Result<Vec<Sheet>, String> {
    use calamine::Reader;

    let mut workbook =
        calamine::open_workbook_auto(path).map_err(|e| format!("could not open workbook: {e}"))?;
    let names = workbook.sheet_names().to_vec();
    let mut sheets = Vec::with_capacity(names.len());
    for name in names {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| format!("could not read sheet \"{name}\": {e}"))?;
        let rows = range
            .rows()
            .map(|row| row.iter().map(from_cell).collect())
            .collect();
        sheets.push(Sheet {
            name,
            rows: trim_rows(rows),
        });
    }
    // A workbook of nothing but empty sheets still has to say which one it was,
    // or the output would be a file with no table and no explanation.
    if sheets.iter().all(|sheet| sheet.rows.is_empty()) {
        return Err("the workbook has no cells with anything in them".into());
    }
    // Empty sheets carry nothing, and an empty CSV per blank tab is noise.
    sheets.retain(|sheet| !sheet.rows.is_empty());
    Ok(sheets)
}

fn from_cell(cell: &calamine::Data) -> Value {
    use calamine::Data as Cell;
    match cell {
        Cell::Empty => Value::Null,
        Cell::Bool(value) => Value::Bool(*value),
        Cell::Int(value) => Value::from(*value),
        // A whole number stored as a float is still a whole number: 3.0 in a
        // cell should reach JSON as 3, the way the sheet showed it.
        Cell::Float(value) if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 => {
            Value::from(*value as i64)
        }
        Cell::Float(value) => Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Cell::String(text) => Value::String(text.clone()),
        Cell::DateTime(date) if date.is_duration() => {
            let total = (date.as_f64() * 86_400.0).round() as i64;
            Value::String(format!(
                "{}:{:02}:{:02}",
                total / 3600,
                (total % 3600) / 60,
                total % 60
            ))
        }
        Cell::DateTime(date) => {
            let (year, month, day, hour, minute, second, _) = date.to_ymd_hms_milli();
            Value::String(if (hour, minute, second) == (0, 0, 0) {
                format!("{year:04}-{month:02}-{day:02}")
            } else {
                format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
            })
        }
        Cell::DateTimeIso(text) | Cell::DurationIso(text) => Value::String(text.clone()),
        // `#DIV/0!` and friends: what the sheet displayed, as text.
        Cell::Error(error) => Value::String(format!("{error}")),
    }
}

/// UTF-8 with or without the byte-order mark Excel puts on its CSV exports.
/// Anything else is decoded leniently rather than refused: a stray byte in a
/// ten-thousand-row export should not cost the other ten thousand rows.
fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("could not read the file: {e}"))?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn read_delimited(
    text: &str,
    delimiter: Option<u8>,
    name: &str,
    typed_cells: bool,
) -> Result<Sheet, String> {
    // Excel writes `sep=;` as a first line when it wants to name the separator.
    let (text, declared) = match text.strip_prefix("sep=") {
        Some(rest) => {
            let mut chars = rest.chars();
            let declared = chars.next().filter(char::is_ascii).map(|c| c as u8);
            let rest = chars.as_str();
            let rest = rest.split_once('\n').map(|(_, body)| body).unwrap_or("");
            (rest, declared)
        }
        None => (text, None),
    };
    let delimiter = delimiter
        .or(declared)
        .unwrap_or_else(|| sniff_delimiter(text));

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("could not read the table: {e}"))?;
        rows.push(
            record
                .iter()
                .map(|cell| match cell {
                    _ if typed_cells => typed(cell),
                    "" => Value::Null,
                    text => Value::String(text.to_string()),
                })
                .collect(),
        );
    }
    Ok(Sheet {
        name: name.to_string(),
        rows: trim_rows(rows),
    })
}

/// Comma, semicolon, tab or pipe — whichever the first line uses most outside
/// of quotes. Spreadsheets in most of Europe export with semicolons, because
/// the comma is their decimal point.
fn sniff_delimiter(text: &str) -> u8 {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let mut counts = [(b',', 0usize), (b';', 0), (b'\t', 0), (b'|', 0)];
    let mut quoted = false;
    for byte in line.bytes() {
        if byte == b'"' {
            quoted = !quoted;
        } else if !quoted {
            for (candidate, count) in counts.iter_mut() {
                if byte == *candidate {
                    *count += 1;
                }
            }
        }
    }
    // `max_by_key` keeps the last of equal maxima; reversing makes the comma
    // win a tie, which is what a line with no separator at all should get.
    counts
        .iter()
        .rev()
        .max_by_key(|(_, count)| *count)
        .map(|(delimiter, _)| *delimiter)
        .unwrap_or(b',')
}

/// A CSV cell is always text; this gives back the numbers and booleans it was
/// written from. Anything that would not survive the round trip stays text —
/// `007` is a code, not seven, and `1e5` is more often an id than a hundred
/// thousand.
fn typed(cell: &str) -> Value {
    if cell.is_empty() {
        return Value::Null;
    }
    match cell {
        "true" | "TRUE" | "True" => return Value::Bool(true),
        "false" | "FALSE" | "False" => return Value::Bool(false),
        _ => {}
    }

    let digits = cell.strip_prefix('-').unwrap_or(cell);
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (digits, None),
    };
    let plain = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let leading_zero = whole.len() > 1 && whole.starts_with('0');
    if !plain(whole) || leading_zero {
        return Value::String(cell.to_string());
    }
    match fraction {
        None if whole.len() <= 15 => cell
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(cell.to_string())),
        Some(fraction) if plain(fraction) => cell
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(cell.to_string())),
        _ => Value::String(cell.to_string()),
    }
}

/// Drops the empty rows and columns a sheet carries past its last value.
fn trim_rows(mut rows: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
    for row in rows.iter_mut() {
        while row.last().is_some_and(Value::is_null) {
            row.pop();
        }
    }
    while rows.last().is_some_and(|row| row.is_empty()) {
        rows.pop();
    }
    rows
}

fn from_toml(value: toml::Value) -> Value {
    match value {
        toml::Value::String(text) => Value::String(text),
        toml::Value::Integer(number) => Value::from(number),
        toml::Value::Float(number) => Number::from_f64(number)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml::Value::Boolean(flag) => Value::Bool(flag),
        // JSON and YAML have no date type; the text is what carries over.
        toml::Value::Datetime(date) => Value::String(date.to_string()),
        toml::Value::Array(items) => Value::Array(items.into_iter().map(from_toml).collect()),
        toml::Value::Table(table) => Value::Object(
            table
                .into_iter()
                .map(|(key, value)| (key, from_toml(value)))
                .collect(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Crossing between the shapes
// ---------------------------------------------------------------------------

/// A workbook as nested values: records where the header allows, rows where it
/// does not, and one key per sheet when there is more than one.
pub fn to_tree(sheets: &[Sheet]) -> Value {
    match sheets {
        [only] => sheet_to_value(only),
        _ => Value::Object(
            sheets
                .iter()
                .map(|sheet| (sheet.name.clone(), sheet_to_value(sheet)))
                .collect(),
        ),
    }
}

fn sheet_to_value(sheet: &Sheet) -> Value {
    let Some((header, body)) = sheet.rows.split_first() else {
        return Value::Array(Vec::new());
    };
    // A body row wider than the header has cells with no name to go under;
    // records would quietly drop them, so the sheet stays rows instead.
    let fits = |names: &Vec<String>| body.iter().all(|row| row.len() <= names.len());
    match header_names(header).filter(fits) {
        Some(names) => Value::Array(
            body.iter()
                .map(|row| {
                    let mut record = Map::new();
                    for (index, name) in names.iter().enumerate() {
                        record.insert(name.clone(), row.get(index).cloned().unwrap_or(Value::Null));
                    }
                    Value::Object(record)
                })
                .collect(),
        ),
        None => Value::Array(sheet.rows.iter().cloned().map(Value::Array).collect()),
    }
}

/// A row is a header when every cell is distinct, non-empty text. A row that
/// starts with numbers is data, and treating it as names would invent a
/// record whose keys are "2019" and "41.5".
fn header_names(row: &[Value]) -> Option<Vec<String>> {
    let mut seen = HashSet::new();
    let names: Vec<String> = row
        .iter()
        .map(|cell| match cell {
            Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    // An empty row is no header: every record would come out as `{}`.
    (!names.is_empty() && names.iter().all(|name| seen.insert(name.clone()))).then_some(names)
}

/// Nested values as a workbook, or an explanation of why they are not one.
pub fn to_sheets(value: &Value, name: &str) -> Result<Vec<Sheet>, String> {
    if let Value::Object(map) = value {
        // `{ "people": [...], "places": [...] }` — what a workbook becomes on
        // its way to JSON — comes back as a sheet per key.
        let tabular = !map.is_empty()
            && map
                .values()
                .all(|value| matches!(value, Value::Array(items) if rows_from(items).is_some()));
        if tabular {
            return Ok(map
                .iter()
                .map(|(key, value)| Sheet {
                    name: key.clone(),
                    rows: match value {
                        Value::Array(items) => rows_from(items).unwrap_or_default(),
                        _ => Vec::new(),
                    },
                })
                .collect());
        }
        // A single object is a single record.
        return Ok(vec![Sheet {
            name: name.to_string(),
            rows: rows_from(std::slice::from_ref(value)).unwrap_or_default(),
        }]);
    }
    match value {
        Value::Array(items) => rows_from(items)
            .map(|rows| {
                vec![Sheet {
                    name: name.to_string(),
                    rows,
                }]
            })
            .ok_or_else(|| {
                "the list mixes records, lists and plain values, so it has no single table shape"
                    .into()
            }),
        _ => Err("a single value has no table shape".into()),
    }
}

fn rows_from(items: &[Value]) -> Option<Vec<Vec<Value>>> {
    if items.is_empty() {
        return Some(Vec::new());
    }
    if items.iter().all(Value::is_object) {
        // Every key that appears anywhere, in the order it first appears.
        let mut header: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for item in items {
            if let Value::Object(record) = item {
                for key in record.keys() {
                    if seen.insert(key.clone()) {
                        header.push(key.clone());
                    }
                }
            }
        }
        let mut rows = vec![header.iter().cloned().map(Value::String).collect()];
        for item in items {
            if let Value::Object(record) = item {
                rows.push(
                    header
                        .iter()
                        .map(|key| flatten(record.get(key).unwrap_or(&Value::Null)))
                        .collect(),
                );
            }
        }
        return Some(rows);
    }
    if items.iter().all(Value::is_array) {
        return Some(
            items
                .iter()
                .map(|row| match row {
                    Value::Array(cells) => cells.iter().map(flatten).collect(),
                    _ => Vec::new(),
                })
                .collect(),
        );
    }
    if items
        .iter()
        .all(|item| !item.is_object() && !item.is_array())
    {
        let mut rows = vec![vec![Value::String("value".into())]];
        rows.extend(items.iter().map(|item| vec![item.clone()]));
        return Some(rows);
    }
    None
}

/// A cell holds one value. Anything nested is kept as its JSON text, which is
/// what a spreadsheet user can read and what a JSON user can paste back.
fn flatten(value: &Value) -> Value {
    match value {
        Value::Object(_) | Value::Array(_) => Value::String(value.to_string()),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Sheet names when this conversion writes one file per sheet. Empty when it
/// writes a single file, which is every target but CSV and TSV.
pub fn split_names(input: &Path, target: &str) -> Result<Vec<String>, String> {
    if !SINGLE_TABLE_TARGETS.contains(&target) {
        return Ok(Vec::new());
    }
    let sheets = sheets_of(read_as(input, false)?, &stem_of(input))?;
    Ok(if sheets.len() > 1 {
        sheets.into_iter().map(|sheet| sheet.name).collect()
    } else {
        Vec::new()
    })
}

/// Converts `input` into `outputs`. A CSV or TSV of a workbook with several
/// sheets gets one output per sheet, in sheet order.
pub fn convert(input: &Path, outputs: &[PathBuf], target: &str) -> Result<(), String> {
    let output = outputs.first().ok_or("no output path")?;
    let data = read_as(input, matches!(target, "json" | "yaml" | "toml" | "xlsx"))?;
    let name = stem_of(input);

    match target {
        "json" | "yaml" | "toml" => {
            let tree = match data {
                Data::Tree(value) => value,
                Data::Workbook(sheets) => to_tree(&sheets),
            };
            let text = match target {
                "json" => serde_json::to_string_pretty(&tree)
                    .map(|text| text + "\n")
                    .map_err(|e| e.to_string())?,
                "yaml" => serde_yaml_ng::to_string(&tree).map_err(|e| e.to_string())?,
                _ => to_toml(tree, &name)?,
            };
            write_file(output, text.as_bytes())
        }
        "csv" | "tsv" => {
            let sheets = sheets_of(data, &name)?;
            let delimiter = if target == "tsv" { b'\t' } else { b',' };
            for (sheet, path) in sheets.iter().zip(outputs) {
                write_delimited(sheet, path, delimiter)?;
            }
            Ok(())
        }
        "xlsx" => write_xlsx(&sheets_of(data, &name)?, output),
        "md" => write_file(output, markdown(&sheets_of(data, &name)?).as_bytes()),
        "html" => write_file(output, html(&sheets_of(data, &name)?, &name).as_bytes()),
        other => Err(format!("no built-in writer for .{other}")),
    }
}

// ---------------------------------------------------------------------------
// The child process
// ---------------------------------------------------------------------------

/// Flags `main` checks for before starting the app. Workbooks arrive from
/// anywhere, a malformed one can make a parser panic, and release builds
/// abort on panic — so, like PDF text, the reading happens in a child process
/// of this same executable, where a bad file can only take itself down.
pub const CONVERT_FLAG: &str = "--data";
pub const SHEETS_FLAG: &str = "--sheets";

/// The arguments that make the app convert `input` as a child process.
pub fn helper_args(target: &str, input: &Path, outputs: &[PathBuf]) -> Vec<String> {
    let mut args = vec![
        CONVERT_FLAG.to_string(),
        target.to_string(),
        input.to_string_lossy().into_owned(),
    ];
    args.extend(
        outputs
            .iter()
            .map(|path| path.to_string_lossy().into_owned()),
    );
    args
}

pub fn helper_convert(target: &str, input: &str, outputs: &[String]) -> i32 {
    quiet_panics();
    let outputs: Vec<PathBuf> = outputs.iter().map(PathBuf::from).collect();
    match convert(Path::new(input), &outputs, target) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// Prints the sheet names a CSV or TSV of `input` would be split into, as a
/// JSON list, for the queue to name its outputs by.
pub fn helper_sheets(target: &str, input: &str) -> i32 {
    quiet_panics();
    match split_names(Path::new(input), target) {
        Ok(names) => {
            println!("{}", Value::from(names));
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn quiet_panics() {
    std::panic::set_hook(Box::new(|_| {
        eprintln!("This file could not be read; it is damaged or uses a feature the reader does not understand");
    }));
}

fn sheets_of(data: Data, name: &str) -> Result<Vec<Sheet>, String> {
    match data {
        Data::Workbook(sheets) => Ok(sheets),
        Data::Tree(value) => to_sheets(&value, name),
    }
}

fn to_toml(tree: Value, name: &str) -> Result<String, String> {
    // TOML's top level is always a table. A list gets a key to sit under,
    // named after the file it came from.
    let tree = match tree {
        Value::Object(_) => tree,
        other => {
            let mut wrapper = Map::new();
            wrapper.insert(toml_key(name), other);
            Value::Object(wrapper)
        }
    };
    let tree = without_nulls(tree)?;
    toml::to_string_pretty(&tree).map_err(|e| format!("cannot be written as TOML: {e}"))
}

/// TOML has no empty value. An absent key reads the same as a null one to
/// almost every program, so those keys are left out; a null in the middle of
/// a list has no such stand-in.
fn without_nulls(value: Value) -> Result<Value, String> {
    Ok(match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| without_nulls(value).map(|value| (key, value)))
                .collect::<Result<_, _>>()?,
        ),
        Value::Array(items) => {
            if items.iter().any(Value::is_null) {
                return Err("TOML has no empty value, and this list contains one".into());
            }
            Value::Array(
                items
                    .into_iter()
                    .map(without_nulls)
                    .collect::<Result<_, _>>()?,
            )
        }
        other => other,
    })
}

fn toml_key(name: &str) -> String {
    let key: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if key.is_empty() {
        "rows".into()
    } else {
        key
    }
}

fn text_of(cell: &Value) -> String {
    match cell {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn write_delimited(sheet: &Sheet, path: &Path, delimiter: u8) -> Result<(), String> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    for row in &sheet.rows {
        writer
            .write_record(row.iter().map(text_of))
            .map_err(|e| e.to_string())?;
    }
    writer.flush().map_err(|e| e.to_string())
}

fn write_xlsx(sheets: &[Sheet], path: &Path) -> Result<(), String> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let mut used = HashSet::new();
    for sheet in sheets {
        if sheet.rows.len() > XLSX_MAX_ROWS {
            return Err(format!(
                "\"{}\" has {} rows; an Excel sheet holds at most {XLSX_MAX_ROWS}",
                sheet.name,
                sheet.rows.len()
            ));
        }
        if sheet.rows.iter().any(|row| row.len() > XLSX_MAX_COLUMNS) {
            return Err(format!(
                "\"{}\" is wider than the {XLSX_MAX_COLUMNS} columns an Excel sheet holds",
                sheet.name
            ));
        }

        let worksheet = workbook.add_worksheet();
        worksheet
            .set_name(sheet_name(&sheet.name, &mut used))
            .map_err(|e| e.to_string())?;
        for (row_index, row) in sheet.rows.iter().enumerate() {
            for (column_index, cell) in row.iter().enumerate() {
                let (row_index, column_index) = (row_index as u32, column_index as u16);
                let written = match cell {
                    Value::Null => continue,
                    Value::Bool(flag) => worksheet
                        .write_boolean(row_index, column_index, *flag)
                        .map(|_| ()),
                    // Beyond 2^53 a float cannot hold the integer exactly, and
                    // a silently changed account number is worse than text.
                    Value::Number(number)
                        if number.is_f64()
                            || number.as_i64().is_some_and(|n| n.unsigned_abs() < 1 << 53) =>
                    {
                        match number.as_f64() {
                            Some(value) => worksheet
                                .write_number(row_index, column_index, value)
                                .map(|_| ()),
                            None => worksheet
                                .write_string(row_index, column_index, number.to_string())
                                .map(|_| ()),
                        }
                    }
                    other => worksheet
                        .write_string(row_index, column_index, text_of(other))
                        .map(|_| ()),
                };
                written.map_err(|e| format!("could not write a cell: {e}"))?;
            }
        }
        worksheet.autofit();
    }
    workbook
        .save(path)
        .map_err(|e| format!("could not save the workbook: {e}"))
}

/// Excel sheet names are at most 31 characters, never contain `[]:*?/\`, and
/// must be unique ignoring case.
fn sheet_name(name: &str, used: &mut HashSet<String>) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '_' } else { c })
        .collect();
    let clean = clean.trim_matches('\'').trim();
    // Excel keeps "History" for its change tracking and refuses a sheet by
    // that name.
    let clean = if clean.eq_ignore_ascii_case("history") {
        "History_"
    } else {
        clean
    };
    let base: String = if clean.is_empty() {
        "Sheet".into()
    } else {
        clean.chars().take(31).collect()
    };

    let mut candidate = base.clone();
    let mut counter = 2;
    while !used.insert(candidate.to_lowercase()) {
        let suffix = format!(" ({counter})");
        let room = 31 - suffix.chars().count();
        candidate = base.chars().take(room).collect::<String>() + &suffix;
        counter += 1;
    }
    candidate
}

fn markdown(sheets: &[Sheet]) -> String {
    let mut out = String::new();
    for sheet in sheets {
        if sheets.len() > 1 {
            out.push_str(&format!("## {}\n\n", sheet.name));
        }
        let width = sheet.rows.iter().map(Vec::len).max().unwrap_or(0);
        if width == 0 {
            continue;
        }
        let cell = |row: &[Value], index: usize| {
            row.get(index)
                .map(text_of)
                .unwrap_or_default()
                .replace('|', "\\|")
                .replace("\r\n", "<br>")
                .replace('\n', "<br>")
        };
        for (index, row) in sheet.rows.iter().enumerate() {
            let cells: Vec<String> = (0..width).map(|column| cell(row, column)).collect();
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
            if index == 0 {
                out.push_str(&format!("|{}\n", " --- |".repeat(width)));
            }
        }
        out.push('\n');
    }
    out
}

fn html(sheets: &[Sheet], title: &str) -> String {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let mut out = format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n\
         <style>body{{font-family:sans-serif}}table{{border-collapse:collapse}}\
         th,td{{border:1px solid #ccc;padding:4px 8px;text-align:left}}th{{background:#f3f3f3}}</style>\n\
         </head>\n<body>\n",
        escape(title)
    );
    for sheet in sheets {
        if sheets.len() > 1 {
            out.push_str(&format!("<h2>{}</h2>\n", escape(&sheet.name)));
        }
        let Some((header, body)) = sheet.rows.split_first() else {
            continue;
        };
        let width = sheet.rows.iter().map(Vec::len).max().unwrap_or(0);
        let cells = |row: &[Value], tag: &str| {
            (0..width)
                .map(|index| {
                    let text = row.get(index).map(text_of).unwrap_or_default();
                    format!("<{tag}>{}</{tag}>", escape(&text))
                })
                .collect::<String>()
        };
        out.push_str("<table>\n<thead><tr>");
        out.push_str(&cells(header, "th"));
        out.push_str("</tr></thead>\n<tbody>\n");
        for row in body {
            out.push_str("<tr>");
            out.push_str(&cells(row, "td"));
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody>\n</table>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("coldmill-data-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn csv_cells_get_their_types_back_but_codes_stay_text() {
        assert_eq!(typed("42"), json!(42));
        assert_eq!(typed("-3.5"), json!(-3.5));
        assert_eq!(typed("true"), json!(true));
        assert_eq!(typed(""), Value::Null);
        // A postcode, an id in scientific notation, and a phone number.
        assert_eq!(typed("007"), json!("007"));
        assert_eq!(typed("1e5"), json!("1e5"));
        assert_eq!(typed("+905551234567"), json!("+905551234567"));
        assert_eq!(typed("1.2.3"), json!("1.2.3"));
        assert_eq!(typed(".5"), json!(".5"));
    }

    #[test]
    fn the_separator_is_found_not_assumed() {
        assert_eq!(sniff_delimiter("a,b,c\n1,2,3"), b',');
        assert_eq!(sniff_delimiter("ad;soyad;tutar\nAli;Kaya;12,5"), b';');
        assert_eq!(sniff_delimiter("a\tb\n"), b'\t');
        // The commas inside quotes are part of a value, not separators.
        assert_eq!(sniff_delimiter("\"Kaya, Ali\";\"İzmir, TR\"\n"), b';');
        assert_eq!(sniff_delimiter("one column"), b',');
    }

    #[test]
    fn excel_bom_and_sep_line_are_understood() {
        let path = scratch("excel.csv");
        std::fs::write(&path, b"\xEF\xBB\xBFsep=;\nname;age\nAyse;31\n").unwrap();
        let Data::Workbook(sheets) = read(&path).unwrap() else {
            panic!("a CSV is a workbook");
        };
        assert_eq!(
            sheets[0].rows,
            vec![
                vec![json!("name"), json!("age")],
                vec![json!("Ayse"), json!(31)]
            ]
        );
    }

    #[test]
    fn a_headed_sheet_becomes_records_and_comes_back() {
        let sheet = Sheet {
            name: "people".into(),
            rows: vec![
                vec![json!("name"), json!("age")],
                vec![json!("Ayşe"), json!(31)],
                vec![json!("Mehmet")],
            ],
        };
        let tree = to_tree(std::slice::from_ref(&sheet));
        assert_eq!(
            tree,
            json!([{"name": "Ayşe", "age": 31}, {"name": "Mehmet", "age": null}])
        );
        let back = to_sheets(&tree, "people").unwrap();
        assert_eq!(back[0].rows[0], vec![json!("name"), json!("age")]);
        assert_eq!(back[0].rows[2], vec![json!("Mehmet"), Value::Null]);
    }

    #[test]
    fn cells_past_the_header_are_not_dropped() {
        let wide = Sheet {
            name: "s".into(),
            rows: vec![
                vec![json!("name"), json!("age")],
                vec![json!("Ali"), json!(31), json!("Istanbul")],
            ],
        };
        assert_eq!(
            to_tree(&[wide]),
            json!([["name", "age"], ["Ali", 31, "Istanbul"]])
        );
        // An empty first row is no header at all.
        assert_eq!(header_names(&[]), None);
    }

    #[test]
    fn csv_to_csv_keeps_the_text_as_written() {
        let path = scratch("prices.csv");
        std::fs::write(&path, "item,price\nçay,1.50\nflag,TRUE\n").unwrap();
        let out = scratch("prices-out.tsv");
        convert(&path, std::slice::from_ref(&out), "tsv").unwrap();
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "item\tprice\nçay\t1.50\nflag\tTRUE\n"
        );
    }

    #[test]
    fn a_numeric_first_row_is_data_not_a_header() {
        let sheet = Sheet {
            name: "s".into(),
            rows: vec![vec![json!(2019), json!(41.5)], vec![json!(2020), json!(42)]],
        };
        assert_eq!(to_tree(&[sheet]), json!([[2019, 41.5], [2020, 42]]));
    }

    #[test]
    fn several_sheets_survive_the_trip_through_json() {
        let sheets = vec![
            Sheet {
                name: "a".into(),
                rows: vec![vec![json!("x")], vec![json!(1)]],
            },
            Sheet {
                name: "b".into(),
                rows: vec![vec![json!("y")], vec![json!(2)]],
            },
        ];
        let tree = to_tree(&sheets);
        assert_eq!(tree, json!({"a": [{"x": 1}], "b": [{"y": 2}]}));
        assert_eq!(to_sheets(&tree, "file").unwrap(), sheets);
    }

    #[test]
    fn nested_values_are_kept_as_json_text_in_a_cell() {
        let sheets = to_sheets(&json!([{"id": 1, "tags": ["a", "b"]}]), "x").unwrap();
        assert_eq!(sheets[0].rows[1], vec![json!(1), json!("[\"a\",\"b\"]")]);
    }

    #[test]
    fn a_lone_value_has_no_table_shape() {
        assert!(to_sheets(&json!(42), "x").is_err());
        assert!(to_sheets(&json!([1, {"a": 1}]), "x").is_err());
    }

    #[test]
    fn toml_wraps_a_list_and_drops_empty_keys() {
        let text = to_toml(json!([{"a": 1, "b": null}]), "my file").unwrap();
        assert!(text.contains("[[my_file]]"), "{text}");
        assert!(text.contains("a = 1"));
        assert!(!text.contains('b'));
        assert!(to_toml(json!({"list": [1, null]}), "x").is_err());
    }

    #[test]
    fn excel_sheet_names_are_made_legal_and_unique() {
        let mut used = HashSet::new();
        assert_eq!(sheet_name("Q1/Q2: sales", &mut used), "Q1_Q2_ sales");
        assert_eq!(sheet_name("Data", &mut used), "Data");
        assert_eq!(sheet_name("data", &mut used), "data (2)");
        let long = "x".repeat(40);
        assert_eq!(sheet_name(&long, &mut used).chars().count(), 31);
        assert_eq!(sheet_name("", &mut used), "Sheet");
        assert_eq!(sheet_name("History", &mut used), "History_");
    }

    #[test]
    fn markdown_tables_escape_their_pipes() {
        let text = markdown(&[Sheet {
            name: "s".into(),
            rows: vec![vec![json!("a"), json!("b")], vec![json!("x|y"), json!(1)]],
        }]);
        assert_eq!(text, "| a | b |\n| --- | --- |\n| x\\|y | 1 |\n\n");
    }

    #[test]
    fn json_to_yaml_to_xlsx_and_back() {
        let json_path = scratch("round.json");
        std::fs::write(
            &json_path,
            r#"[{"şehir": "İzmir", "nüfus": 4479525, "kıyı": true}]"#,
        )
        .unwrap();

        // Key order is the file's own, not alphabetical.
        let yaml_path = scratch("round.yaml");
        convert(&json_path, std::slice::from_ref(&yaml_path), "yaml").unwrap();
        let yaml = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(
            yaml.find("şehir").unwrap() < yaml.find("nüfus").unwrap(),
            "{yaml}"
        );

        let xlsx_path = scratch("round.xlsx");
        convert(&yaml_path, std::slice::from_ref(&xlsx_path), "xlsx").unwrap();
        let back = scratch("back.json");
        convert(&xlsx_path, std::slice::from_ref(&back), "json").unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(&back).unwrap()).unwrap();
        assert_eq!(
            value,
            json!([{"şehir": "İzmir", "nüfus": 4479525, "kıyı": true}])
        );
    }

    #[test]
    fn a_workbook_splits_into_one_csv_per_sheet() {
        let json_path = scratch("book.json");
        std::fs::write(&json_path, r#"{"first": [{"a": 1}], "second": [{"b": 2}]}"#).unwrap();
        assert_eq!(
            split_names(&json_path, "csv").unwrap(),
            vec!["first", "second"]
        );
        assert!(split_names(&json_path, "xlsx").unwrap().is_empty());

        let outputs = [scratch("book-first.csv"), scratch("book-second.csv")];
        convert(&json_path, &outputs, "csv").unwrap();
        assert_eq!(std::fs::read_to_string(&outputs[1]).unwrap(), "b\n2\n");
    }

    #[test]
    fn toml_dates_become_text() {
        let path = scratch("conf.toml");
        std::fs::write(&path, "title = \"x\"\nwhen = 1979-05-27T07:32:00Z\n").unwrap();
        let Data::Tree(tree) = read(&path).unwrap() else {
            panic!("TOML is a tree");
        };
        assert_eq!(tree, json!({"title": "x", "when": "1979-05-27T07:32:00Z"}));
    }

    /// Workbooks written by a real office suite, read back here, and a
    /// workbook written here opened by one. Ignored by default because it
    /// needs LibreOffice installed:
    ///
    ///   cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn workbooks_cross_with_libreoffice() {
        let Some(soffice) = crate::engines::find_libreoffice() else {
            println!("LibreOffice not found; skipped");
            return;
        };
        let dir = std::env::temp_dir().join(format!("coldmill-data-office-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let profile = format!(
            "-env:UserInstallation=file:///{}",
            dir.join("profile").to_string_lossy().replace('\\', "/")
        );
        let office = |input: &Path, to: &str| {
            let status = std::process::Command::new(&soffice)
                .args([
                    profile.as_str(),
                    "--headless",
                    "--convert-to",
                    to,
                    "--outdir",
                ])
                .arg(&dir)
                .arg(input)
                .output()
                .unwrap();
            assert!(status.status.success(), "{status:?}");
            dir.join(format!(
                "{}.{}",
                stem_of(input),
                to.split(':').next().unwrap()
            ))
        };

        let source = dir.join("cities.csv");
        // Text and whole numbers only: how LibreOffice types a CSV's
        // booleans and decimals depends on the machine's locale, and that is
        // its import being tested, not this reader.
        std::fs::write(&source, "şehir,nüfus\nİzmir,4479525\nAnkara,5803482\n").unwrap();
        let expected = json!([
            {"şehir": "İzmir", "nüfus": 4479525},
            {"şehir": "Ankara", "nüfus": 5803482},
        ]);

        for format in ["xlsx", "xls", "ods"] {
            let workbook = office(&source, format);
            let json_path = dir.join(format!("from-{format}.json"));
            convert(&workbook, std::slice::from_ref(&json_path), "json").unwrap();
            let value: Value = serde_json::from_slice(&std::fs::read(&json_path).unwrap()).unwrap();
            assert_eq!(value, expected, "read back from {format}");
        }

        // And the other way: our workbook, opened by LibreOffice.
        let ours = dir.join("ours.xlsx");
        convert(&source, std::slice::from_ref(&ours), "xlsx").unwrap();
        let round = dir.join("ours-round.csv");
        std::fs::rename(office(&ours, "csv"), &round).unwrap();
        let text = std::fs::read_to_string(&round).unwrap();
        assert!(text.contains("İzmir,4479525"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
