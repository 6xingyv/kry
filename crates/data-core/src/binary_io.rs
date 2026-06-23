//! Little-endian binary read/write primitives shared by the artifact codecs in
//! `artifacts.rs`. Leaf helpers only (no artifact-struct knowledge).

use std::io::{self, Read, Write};

pub(crate) fn read_f32(reader: &mut impl Read) -> io::Result<f32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(f32::from_le_bytes(bytes))
}

pub(crate) fn write_f32(writer: &mut impl Write, value: f32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

pub(crate) fn write_f64(writer: &mut impl Write, value: f64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

pub(crate) fn read_f64(reader: &mut impl Read) -> io::Result<f64> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(f64::from_le_bytes(bytes))
}

pub(crate) fn write_u64(writer: &mut impl Write, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

pub(crate) fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

pub(crate) fn write_len(writer: &mut impl Write, value: usize) -> io::Result<()> {
    write_u64(writer, value as u64)
}

pub(crate) fn read_len(reader: &mut impl Read) -> io::Result<usize> {
    usize::try_from(read_u64(reader)?)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "length exceeds usize"))
}

pub(crate) fn write_bool(writer: &mut impl Write, value: bool) -> io::Result<()> {
    writer.write_all(&[u8::from(value)])
}

pub(crate) fn read_bool(reader: &mut impl Read) -> io::Result<bool> {
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte)?;
    match byte[0] {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "invalid bool")),
    }
}

pub(crate) fn write_string(writer: &mut impl Write, value: &str) -> io::Result<()> {
    write_len(writer, value.len())?;
    writer.write_all(value.as_bytes())
}

pub(crate) fn read_string(reader: &mut impl Read) -> io::Result<String> {
    let len = read_len(reader)?;
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub(crate) fn write_option_string(writer: &mut impl Write, value: &Option<String>) -> io::Result<()> {
    write_bool(writer, value.is_some())?;
    if let Some(value) = value {
        write_string(writer, value)?;
    }
    Ok(())
}

pub(crate) fn read_option_string(reader: &mut impl Read) -> io::Result<Option<String>> {
    if read_bool(reader)? {
        read_string(reader).map(Some)
    } else {
        Ok(None)
    }
}

pub(crate) fn write_string_vec(writer: &mut impl Write, values: &[String]) -> io::Result<()> {
    write_len(writer, values.len())?;
    for value in values {
        write_string(writer, value)?;
    }
    Ok(())
}

pub(crate) fn read_string_vec(reader: &mut impl Read) -> io::Result<Vec<String>> {
    let len = read_len(reader)?;
    (0..len).map(|_| read_string(reader)).collect()
}
