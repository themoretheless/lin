use std::io::{Read, Write};
use std::sync::Arc;
use crate::error::Error;
use crate::store::{Cell, InlineStr};
use crate::exact_sum::ExactSum;

pub trait Spillable: Sized {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error>;
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error>;
}

impl Spillable for () {
    fn serialize(&self, _writer: &mut impl Write) -> Result<(), Error> { Ok(()) }
    fn deserialize(_reader: &mut impl Read) -> Result<Self, Error> { Ok(()) }
}

impl Spillable for ExactSum {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        let (int, float, comp) = self.into_parts();
        writer.write_all(&int.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))?;
        writer.write_all(&float.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))?;
        writer.write_all(&comp.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))?;
        Ok(())
    }
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut buf = [0u8; 16];
        reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
        let int = i128::from_le_bytes(buf);
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
        let float = f64::from_le_bytes(buf);
        reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
        let comp = f64::from_le_bytes(buf);
        Ok(ExactSum::from_parts(int, float, comp))
    }
}

impl Spillable for Cell {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        match self {
            Cell::Null => writer.write_all(&[0u8]).map_err(|e| Error::runtime(e.to_string())),
            Cell::Text(s) => {
                writer.write_all(&[1u8]).map_err(|e| Error::runtime(e.to_string()))?;
                let b = s.as_bytes();
                writer.write_all(&(b.len() as u32).to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(b).map_err(|e| Error::runtime(e.to_string()))
            }
            Cell::SmallText(s) => {
                writer.write_all(&[2u8]).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(&[s.len() as u8]).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(s.as_bytes()).map_err(|e| Error::runtime(e.to_string()))
            }
            Cell::Int(n) => {
                writer.write_all(&[3u8]).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(&n.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))
            }
            Cell::Float(f) => {
                writer.write_all(&[4u8]).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(&f.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))
            }
            Cell::Bool(b) => {
                writer.write_all(&[5u8]).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(&[*b as u8]).map_err(|e| Error::runtime(e.to_string()))
            }
            Cell::Time(t) => {
                writer.write_all(&[6u8]).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(&t.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))
            }
            Cell::Vec(v) => {
                writer.write_all(&[7u8]).map_err(|e| Error::runtime(e.to_string()))?;
                writer.write_all(&(v.len() as u32).to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))?;
                for x in v.as_ref() {
                    writer.write_all(&x.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))?;
                }
                Ok(())
            }
        }
    }
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut tag = [0u8; 1];
        reader.read_exact(&mut tag).map_err(|e| Error::runtime(e.to_string()))?;
        match tag[0] {
            0 => Ok(Cell::Null),
            1 => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf).map_err(|e| Error::runtime(e.to_string()))?;
                let len = u32::from_le_bytes(len_buf) as usize;
                let mut buf = vec![0u8; len];
                reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
                let s = String::from_utf8(buf).map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::Text(Arc::from(s)))
            }
            2 => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf).map_err(|e| Error::runtime(e.to_string()))?;
                let len = len_buf[0] as usize;
                let mut buf = [0u8; 22];
                reader.read_exact(&mut buf[..len]).map_err(|e| Error::runtime(e.to_string()))?;
                let s = std::str::from_utf8(&buf[..len]).map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::SmallText(InlineStr::try_new(s).ok_or_else(|| Error::runtime("invalid inline string".to_string()))?))
            }
            3 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::Int(i64::from_le_bytes(buf)))
            }
            4 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::Float(f64::from_le_bytes(buf)))
            }
            5 => {
                let mut buf = [0u8; 1];
                reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::Bool(buf[0] != 0))
            }
            6 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::Time(i64::from_le_bytes(buf)))
            }
            7 => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf).map_err(|e| Error::runtime(e.to_string()))?;
                let len = u32::from_le_bytes(len_buf) as usize;
                let mut v = Vec::with_capacity(len);
                for _ in 0..len {
                    let mut buf = [0u8; 4];
                    reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
                    v.push(f32::from_le_bytes(buf));
                }
                Ok(Cell::Vec(Arc::from(v)))
            }
            _ => Err(Error::runtime("invalid tag".to_string())),
        }
    }
}

impl<T: Spillable> Spillable for Option<T> {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        match self {
            Some(v) => {
                writer.write_all(&[1u8]).map_err(|e| Error::runtime(e.to_string()))?;
                v.serialize(writer)
            }
            None => writer.write_all(&[0u8]).map_err(|e| Error::runtime(e.to_string())),
        }
    }
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut tag = [0u8; 1];
        reader.read_exact(&mut tag).map_err(|e| Error::runtime(e.to_string()))?;
        match tag[0] {
            1 => Ok(Some(T::deserialize(reader)?)),
            0 => Ok(None),
            _ => Err(Error::runtime("invalid tag".to_string())),
        }
    }
}

impl<A: Spillable, B: Spillable> Spillable for (A, B) {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        self.0.serialize(writer)?;
        self.1.serialize(writer)
    }
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        Ok((A::deserialize(reader)?, B::deserialize(reader)?))
    }
}

impl Spillable for f64 {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        writer.write_all(&self.to_le_bytes()).map_err(|e| Error::runtime(e.to_string()))
    }
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).map_err(|e| Error::runtime(e.to_string()))?;
        Ok(f64::from_le_bytes(buf))
    }
}
