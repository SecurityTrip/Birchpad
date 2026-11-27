//! Bounded parsing for the CSV viewer.

pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_COLUMNS: usize = 128;
pub const MAX_ROWS: usize = 50_000;

#[derive(Debug)]
pub enum TableError {
    TooManyBytes,
    TooManyColumns,
    TooManyRows,
    Invalid(::csv::Error),
}

/// Reads comma-separated UTF-8 records of equal width. The first record is data, too.
pub fn parse(text: &str) -> Result<Vec<::csv::StringRecord>, TableError> {
    if text.len() > MAX_BYTES {
        return Err(TableError::TooManyBytes);
    }
    let mut reader = ::csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(TableError::Invalid)?;
        if record.len() > MAX_COLUMNS {
            return Err(TableError::TooManyColumns);
        }
        if rows.len() == MAX_ROWS {
            return Err(TableError::TooManyRows);
        }
        rows.push(record);
    }
    Ok(rows)
}

pub use ::csv::StringRecord;
