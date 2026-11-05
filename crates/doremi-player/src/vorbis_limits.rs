//! Allocation preflight for public Vorbis streams, before decoder construction.
//! Reads the codebook layout from the Xiph Vorbis I specification, section 3.2.
//! Symphonia still validates/decodes the complete setup; no dependency is copied.
use crate::DecodeError;
use symphonia::core::io::{BitReaderRtl, ReadBitsRtl};

pub(super) fn check(extra: &[u8]) -> Result<(), DecodeError> {
    if extra.len() > 256 * 1024 {
        return Err(DecodeError::TooLarge);
    }
    if extra.len() < 38 || &extra[..7] != b"\x01vorbis" || &extra[30..37] != b"\x05vorbis" {
        return Err(DecodeError::InvalidOgg);
    }
    let mut bits = BitReaderRtl::new(&extra[37..]);
    let mut read = |count| {
        bits.read_bits_leq32(count)
            .map_err(|_| DecodeError::InvalidOgg)
    };
    let books = read(8)? + 1;
    let mut total_entries = 0u64;
    let mut total_values = 0u64;
    let mut table_bound = 0u64;
    for _ in 0..books {
        if read(24)? != 0x564342 {
            return Err(DecodeError::InvalidOgg);
        }
        let dimensions = read(16)?;
        let entries = read(24)?;
        total_entries += u64::from(entries);
        if dimensions == 0 || dimensions > 16 || entries == 0 || total_entries > 65536 {
            return Err(DecodeError::TooLarge);
        }
        // Each used codeword can introduce at most one 256-entry lookup block
        // per eight bits. Counting them separately is a conservative upper bound.
        let mut account = |length: u32, count: u32| -> Result<(), DecodeError> {
            if length == 0 || length > 32 {
                return Err(DecodeError::InvalidOgg);
            }
            table_bound += u64::from(length.div_ceil(8)) * 256 * 8 * u64::from(count);
            if table_bound > 64 * 1024 * 1024 {
                return Err(DecodeError::TooLarge);
            }
            Ok(())
        };
        if read(1)? == 1 {
            let mut length = read(5)? + 1;
            let mut left = entries;
            while left != 0 {
                if length > 32 {
                    return Err(DecodeError::InvalidOgg);
                }
                let count = read(u32::BITS - left.leading_zeros())?;
                if count > left {
                    return Err(DecodeError::InvalidOgg);
                }
                account(length, count)?;
                left -= count;
                length += 1;
            }
        } else {
            let sparse = read(1)? == 1;
            for _ in 0..entries {
                if !sparse || read(1)? == 1 {
                    account(read(5)? + 1, 1)?;
                }
            }
        }
        let lookup = read(4)?;
        match lookup {
            0 => {}
            1 | 2 => {
                let values = u64::from(entries) * u64::from(dimensions);
                total_values += values;
                if total_values > 1024 * 1024 {
                    return Err(DecodeError::TooLarge);
                }
                read(32)?;
                read(32)?;
                let width = read(4)? + 1;
                read(1)?;
                let encoded_values = if lookup == 2 {
                    values as u32
                } else {
                    lookup1(entries, dimensions)
                };
                for _ in 0..encoded_values {
                    read(width)?;
                }
            }
            _ => return Err(DecodeError::InvalidOgg),
        }
    }
    Ok(())
}
fn lookup1(entries: u32, dimensions: u32) -> u32 {
    let (mut low, mut high) = (1, entries);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if middle
            .checked_pow(dimensions)
            .is_some_and(|value| value <= entries)
        {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header(dimensions: u32, entries: u32) -> Vec<u8> {
        let mut bytes = vec![0; 37];
        bytes[..7].copy_from_slice(b"\x01vorbis");
        bytes[30..37].copy_from_slice(b"\x05vorbis");
        let mut at = 37 * 8;
        for (value, width) in [(0, 8), (0x564342, 24), (dimensions, 16), (entries, 24)] {
            for bit in 0..width {
                if at / 8 == bytes.len() {
                    bytes.push(0);
                }
                bytes[at / 8] |= (((value >> bit) & 1) as u8) << (at % 8);
                at += 1;
            }
        }
        bytes
    }
    #[test]
    fn rejects_tiny_headers_requesting_large_or_zero_dimension_allocations() {
        for (dimensions, entries) in [(65535, 16777215), (0, 1), (1, 65537)] {
            assert_eq!(
                check(&header(dimensions, entries)),
                Err(DecodeError::TooLarge)
            );
        }
        assert_eq!(check(&vec![0; 256 * 1024 + 1]), Err(DecodeError::TooLarge));
        assert_eq!(lookup1(625, 4), 5);
        assert_eq!(lookup1(65536, 32), 1);
    }
}
