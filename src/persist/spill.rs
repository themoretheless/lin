use std::cell::RefCell;
use std::io::{Read, Write};
use std::sync::Arc;

use crate::error::Error;
use crate::exact_sum::ExactSum;
use crate::store::{Cell, InlineStr};

const MAX_SPILL_STR_BYTES: usize = 64 * 1024 * 1024;
const MAX_SPILL_VEC_LEN: usize = 16 * 1024 * 1024;

thread_local! {
    static SPILL_SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static LAST_SPILL_TEXT: RefCell<Option<Arc<str>>> = const { RefCell::new(None) };
}

#[inline]
fn io_err(e: std::io::Error) -> Error {
    Error::runtime(e.to_string())
}

pub trait Spillable: Sized {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error>;
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error>;
}

impl Spillable for () {
    #[inline]
    fn serialize(&self, _writer: &mut impl Write) -> Result<(), Error> {
        Ok(())
    }

    #[inline]
    fn deserialize(_reader: &mut impl Read) -> Result<Self, Error> {
        Ok(())
    }
}

impl Spillable for ExactSum {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        let (int, float, comp) = self.into_parts();
        let mut buf = [0u8; 32];
        buf[..16].copy_from_slice(&int.to_le_bytes());
        buf[16..24].copy_from_slice(&float.to_le_bytes());
        buf[24..32].copy_from_slice(&comp.to_le_bytes());
        writer.write_all(&buf).map_err(io_err)
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf).map_err(io_err)?;
        let int = i128::from_le_bytes(buf[..16].try_into().unwrap());
        let float = f64::from_le_bytes(buf[16..24].try_into().unwrap());
        let comp = f64::from_le_bytes(buf[24..32].try_into().unwrap());
        Ok(ExactSum::from_parts(int, float, comp))
    }
}

impl Spillable for Cell {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        match self {
            Cell::Null => writer.write_all(&[0u8]).map_err(io_err),
            Cell::Text(s) => {
                let b = s.as_bytes();
                if b.len() <= InlineStr::MAX_LEN {
                    let mut buf = [0u8; 2 + InlineStr::MAX_LEN];
                    buf[0] = 2;
                    buf[1] = b.len() as u8;
                    buf[2..2 + b.len()].copy_from_slice(b);
                    return writer.write_all(&buf[..2 + b.len()]).map_err(io_err);
                }
                let len = u32::try_from(b.len())
                    .map_err(|_| Error::runtime("spill text exceeds u32 length"))?;
                if b.len() <= 250 {
                    let mut buf = [0u8; 256];
                    buf[0] = 1;
                    buf[1..5].copy_from_slice(&len.to_le_bytes());
                    buf[5..5 + b.len()].copy_from_slice(b);
                    return writer.write_all(&buf[..5 + b.len()]).map_err(io_err);
                }
                let mut hdr = [0u8; 5];
                hdr[0] = 1;
                hdr[1..5].copy_from_slice(&len.to_le_bytes());
                writer.write_all(&hdr).map_err(io_err)?;
                writer.write_all(b).map_err(io_err)
            }
            Cell::SmallText(s) => {
                let b = s.as_bytes();
                let mut buf = [0u8; 2 + InlineStr::MAX_LEN];
                buf[0] = 2;
                buf[1] = b.len() as u8;
                buf[2..2 + b.len()].copy_from_slice(b);
                writer.write_all(&buf[..2 + b.len()]).map_err(io_err)
            }
            Cell::Int(n) => {
                let mut buf = [0u8; 9];
                buf[0] = 3;
                buf[1..9].copy_from_slice(&n.to_le_bytes());
                writer.write_all(&buf).map_err(io_err)
            }
            Cell::Float(f) => {
                let mut buf = [0u8; 9];
                buf[0] = 4;
                buf[1..9].copy_from_slice(&f.to_le_bytes());
                writer.write_all(&buf).map_err(io_err)
            }
            Cell::Bool(b) => writer.write_all(&[5u8, u8::from(*b)]).map_err(io_err),
            Cell::Time(t) => {
                let mut buf = [0u8; 9];
                buf[0] = 6;
                buf[1..9].copy_from_slice(&t.to_le_bytes());
                writer.write_all(&buf).map_err(io_err)
            }
            Cell::Vec(v) => {
                let len = u32::try_from(v.len())
                    .map_err(|_| Error::runtime("spill vector exceeds u32 length"))?;
                #[cfg(target_endian = "little")]
                {
                    let byte_len = v.len() * std::mem::size_of::<f32>();
                    // SAFETY: `[f32]` on little-endian targets has exact IEEE-754 LE byte layout.
                    let raw = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, byte_len) };
                    if byte_len <= 250 {
                        let mut buf = [0u8; 256];
                        buf[0] = 7;
                        buf[1..5].copy_from_slice(&len.to_le_bytes());
                        buf[5..5 + byte_len].copy_from_slice(raw);
                        return writer.write_all(&buf[..5 + byte_len]).map_err(io_err);
                    }
                    let mut hdr = [0u8; 5];
                    hdr[0] = 7;
                    hdr[1..5].copy_from_slice(&len.to_le_bytes());
                    writer.write_all(&hdr).map_err(io_err)?;
                    writer.write_all(raw).map_err(io_err)
                }
                #[cfg(not(target_endian = "little"))]
                {
                    let mut hdr = [0u8; 5];
                    hdr[0] = 7;
                    hdr[1..5].copy_from_slice(&len.to_le_bytes());
                    writer.write_all(&hdr).map_err(io_err)?;
                    let mut chunk_buf = [0u8; 256];
                    for chunk in v.chunks(64) {
                        let byte_len = chunk.len() * 4;
                        for (i, &x) in chunk.iter().enumerate() {
                            chunk_buf[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
                        }
                        writer.write_all(&chunk_buf[..byte_len]).map_err(io_err)?;
                    }
                    Ok(())
                }
            }
        }
    }

    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut tag = [0u8; 1];
        reader.read_exact(&mut tag).map_err(io_err)?;
        match tag[0] {
            0 => Ok(Cell::Null),
            1 => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf).map_err(io_err)?;
                let len = u32::from_le_bytes(len_buf) as usize;
                if len > MAX_SPILL_STR_BYTES {
                    return Err(Error::runtime("corrupt spill text length"));
                }
                if len <= InlineStr::MAX_LEN {
                    let mut buf = [0u8; InlineStr::MAX_LEN];
                    reader.read_exact(&mut buf[..len]).map_err(io_err)?;
                    let s = std::str::from_utf8(&buf[..len])
                        .map_err(|e| Error::runtime(e.to_string()))?;
                    return Ok(Cell::from_text(s));
                }
                SPILL_SCRATCH.with(|cell| {
                    let mut scratch = cell.borrow_mut();
                    scratch.resize(len, 0);
                    reader.read_exact(&mut scratch[..len]).map_err(io_err)?;
                    let s = std::str::from_utf8(&scratch[..len])
                        .map_err(|e| Error::runtime(e.to_string()))?;
                    let arc = LAST_SPILL_TEXT.with(|last| {
                        let mut slot = last.borrow_mut();
                        if let Some(prev) = slot.as_ref()
                            && prev.as_ref() == s
                        {
                            return Arc::clone(prev);
                        }
                        let created: Arc<str> = Arc::from(s);
                        if len <= 256 {
                            *slot = Some(Arc::clone(&created));
                        }
                        created
                    });
                    Ok(Cell::Text(arc))
                })
            }
            2 => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf).map_err(io_err)?;
                let len = len_buf[0] as usize;
                if len > InlineStr::MAX_LEN {
                    return Err(Error::runtime("corrupt inline string length in spill"));
                }
                let mut buf = [0u8; InlineStr::MAX_LEN];
                reader.read_exact(&mut buf[..len]).map_err(io_err)?;
                let s = std::str::from_utf8(&buf[..len])
                    .map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::SmallText(
                    InlineStr::try_new(s)
                        .ok_or_else(|| Error::runtime("invalid inline string"))?,
                ))
            }
            3 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Int(i64::from_le_bytes(buf)))
            }
            4 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Float(f64::from_le_bytes(buf)))
            }
            5 => {
                let mut buf = [0u8; 1];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Bool(buf[0] != 0))
            }
            6 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Time(i64::from_le_bytes(buf)))
            }
            7 => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf).map_err(io_err)?;
                let len = u32::from_le_bytes(len_buf) as usize;
                if len > MAX_SPILL_VEC_LEN {
                    return Err(Error::runtime("corrupt spill vector length"));
                }
                let byte_len = len * 4;
                #[cfg(target_endian = "little")]
                {
                    let mut v = vec![0.0f32; len];
                    if byte_len > 0 {
                        // SAFETY: `v` has `len` initialized `f32` elements (`byte_len` bytes),
                        // and every 4-byte pattern is a valid `f32` bit pattern.
                        let raw = unsafe {
                            std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, byte_len)
                        };
                        reader.read_exact(raw).map_err(io_err)?;
                    }
                    Ok(Cell::Vec(Arc::from(v)))
                }
                #[cfg(not(target_endian = "little"))]
                {
                    SPILL_SCRATCH.with(|cell| {
                        let mut scratch = cell.borrow_mut();
                        scratch.resize(byte_len, 0);
                        reader.read_exact(&mut scratch[..byte_len]).map_err(io_err)?;
                        let v: Vec<f32> = scratch[..byte_len]
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();
                        Ok(Cell::Vec(Arc::from(v)))
                    })
                }
            }
            _ => Err(Error::runtime("invalid spill cell tag")),
        }
    }
}

impl<T: Spillable> Spillable for Option<T> {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        match self {
            Some(v) => {
                writer.write_all(&[1u8]).map_err(io_err)?;
                v.serialize(writer)
            }
            None => writer.write_all(&[0u8]).map_err(io_err),
        }
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut tag = [0u8; 1];
        reader.read_exact(&mut tag).map_err(io_err)?;
        match tag[0] {
            1 => Ok(Some(T::deserialize(reader)?)),
            0 => Ok(None),
            _ => Err(Error::runtime("invalid spill option tag")),
        }
    }
}

impl<A: Spillable, B: Spillable> Spillable for (A, B) {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        self.0.serialize(writer)?;
        self.1.serialize(writer)
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        Ok((A::deserialize(reader)?, B::deserialize(reader)?))
    }
}

impl Spillable for f64 {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        writer.write_all(&self.to_le_bytes()).map_err(io_err)
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).map_err(io_err)?;
        Ok(f64::from_le_bytes(buf))
    }
}
