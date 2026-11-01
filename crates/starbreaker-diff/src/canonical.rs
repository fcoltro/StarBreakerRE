//! Canonical DataCore record hashing: walks a record through a compact JSON
//! `ExportSink`, strips record identity metadata, and hashes the value with
//! BLAKE3 so records compare by content (`canonical_record_hash`).

use std::io::Write;

use serde_json::ser::{Formatter, PrettyFormatter};
use starbreaker_datacore::database::Database;
use starbreaker_datacore::sink::ExportSink;
use starbreaker_datacore::types::{CigGuid, Record};

use crate::Result;

pub(crate) fn canonical_record_hash(db: &Database, record: &Record) -> Result<String> {
    let mut bytes = Vec::new();
    let mut sink = CanonicalJsonSink::new(&mut bytes);
    starbreaker_datacore::walker::walk_record(db, record, &mut sink)?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    strip_record_metadata(&mut value);
    let record_value = value
        .get("_RecordValue_")
        .cloned()
        .unwrap_or(value);
    let canonical = serde_json::to_vec(&record_value)?;
    Ok(format!("blake3:{}", blake3::hash(&canonical)))
}

fn strip_record_metadata(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("_RecordId_");
            map.remove("_RecordName_");
            map.remove("_RecordTag_");
            for value in map.values_mut() {
                strip_record_metadata(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                strip_record_metadata(value);
            }
        }
        _ => {}
    }
}

struct CanonicalJsonSink<W: Write> {
    writer: W,
    fmt: PrettyFormatter<'static>,
    first_stack: Vec<bool>,
}

impl<W: Write> CanonicalJsonSink<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            fmt: PrettyFormatter::with_indent(b""),
            first_stack: Vec::new(),
        }
    }

    fn is_first(&self) -> bool {
        self.first_stack.last().copied().unwrap_or(true)
    }

    fn mark_not_first(&mut self) {
        if let Some(first) = self.first_stack.last_mut() {
            *first = false;
        }
    }

    fn key_prefix(&mut self, name: &str) -> std::io::Result<()> {
        let first = self.is_first();
        self.fmt.begin_object_key(&mut self.writer, first)?;
        self.mark_not_first();
        serde_json::to_writer(&mut self.writer, name).map_err(std::io::Error::other)?;
        self.fmt.end_object_key(&mut self.writer)?;
        self.fmt.begin_object_value(&mut self.writer)?;
        Ok(())
    }

    fn array_value_prefix(&mut self) -> std::io::Result<()> {
        let first = self.is_first();
        self.fmt.begin_array_value(&mut self.writer, first)?;
        self.mark_not_first();
        Ok(())
    }

    fn value_prefix(&mut self, name: Option<&str>) -> std::io::Result<()> {
        if let Some(name) = name {
            self.key_prefix(name)
        } else if self.first_stack.is_empty() {
            Ok(())
        } else {
            self.array_value_prefix()
        }
    }
}

impl<W: Write> ExportSink for CanonicalJsonSink<W> {
    type Error = std::io::Error;

    fn extension(&self) -> &str {
        "json"
    }

    fn begin_object(&mut self, name: Option<&str>) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.begin_object(&mut self.writer)?;
        self.first_stack.push(true);
        Ok(())
    }

    fn end_object(&mut self) -> std::result::Result<(), Self::Error> {
        self.first_stack.pop();
        self.fmt.end_object(&mut self.writer)
    }

    fn begin_array(&mut self, name: &str) -> std::result::Result<(), Self::Error> {
        self.key_prefix(name)?;
        self.fmt.begin_array(&mut self.writer)?;
        self.first_stack.push(true);
        Ok(())
    }

    fn end_array(&mut self) -> std::result::Result<(), Self::Error> {
        self.first_stack.pop();
        self.fmt.end_array(&mut self.writer)
    }

    fn write_null(&mut self, name: Option<&str>) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_null(&mut self.writer)
    }

    fn write_bool(
        &mut self,
        name: Option<&str>,
        value: bool,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_bool(&mut self.writer, value)
    }

    fn write_i8(&mut self, name: Option<&str>, value: i8) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_i8(&mut self.writer, value)
    }

    fn write_i16(
        &mut self,
        name: Option<&str>,
        value: i16,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_i16(&mut self.writer, value)
    }

    fn write_i32(
        &mut self,
        name: Option<&str>,
        value: i32,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_i32(&mut self.writer, value)
    }

    fn write_i64(
        &mut self,
        name: Option<&str>,
        value: i64,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_i64(&mut self.writer, value)
    }

    fn write_u8(&mut self, name: Option<&str>, value: u8) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_u8(&mut self.writer, value)
    }

    fn write_u16(
        &mut self,
        name: Option<&str>,
        value: u16,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_u16(&mut self.writer, value)
    }

    fn write_u32(
        &mut self,
        name: Option<&str>,
        value: u32,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_u32(&mut self.writer, value)
    }

    fn write_u64(
        &mut self,
        name: Option<&str>,
        value: u64,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        self.fmt.write_u64(&mut self.writer, value)
    }

    fn write_f32(
        &mut self,
        name: Option<&str>,
        value: f32,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        if value.is_finite() {
            self.fmt.write_f32(&mut self.writer, value)
        } else {
            self.fmt.write_null(&mut self.writer)
        }
    }

    fn write_f64(
        &mut self,
        name: Option<&str>,
        value: f64,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        if value.is_finite() {
            self.fmt.write_f64(&mut self.writer, value)
        } else {
            self.fmt.write_null(&mut self.writer)
        }
    }

    fn write_str(
        &mut self,
        name: Option<&str>,
        value: &str,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        serde_json::to_writer(&mut self.writer, value).map_err(std::io::Error::other)
    }

    fn write_guid(
        &mut self,
        name: Option<&str>,
        value: &CigGuid,
    ) -> std::result::Result<(), Self::Error> {
        self.value_prefix(name)?;
        serde_json::to_writer(&mut self.writer, &value.to_string()).map_err(std::io::Error::other)
    }

    fn write_record_ref(
        &mut self,
        name: Option<&str>,
        record_id: &CigGuid,
        _record_name: &str,
        _path: &str,
    ) -> std::result::Result<(), Self::Error> {
        self.write_guid(name, record_id)
    }
}
