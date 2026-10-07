//! 压缩块（4 字节类型 LE + 4 字节 adler32 BE + 数据）与 key block info 解密。

use flate2::{Decompress, FlushDecompress};
use ripemd::{Digest, Ripemd128};

use crate::error::{Error, Result};

pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

pub fn zlib_decompress(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected);
    Decompress::new(true)
        .decompress_vec(data, &mut out, FlushDecompress::Finish)
        .map_err(|e| Error::InvalidDict(format!("zlib 解压失败：{e}")))?;
    Ok(out)
}

/// 解压一个块并校验长度与 adler32。
pub fn decompress_block(raw: &[u8], expected: usize) -> Result<Vec<u8>> {
    let bad = |m: String| Error::InvalidDict(m);
    if raw.len() < 8 {
        return Err(bad("压缩块不足 8 字节".into()));
    }
    let ctype = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let checksum = u32::from_be_bytes([raw[4], raw[5], raw[6], raw[7]]);
    let data = &raw[8..];
    let out = match ctype {
        0 => data.to_vec(),
        1 => lzokay_native::decompress_all(data, Some(expected))
            .map_err(|e| bad(format!("LZO 解压失败：{e:?}")))?,
        2 => zlib_decompress(data, expected)?,
        other => return Err(bad(format!("未知的压缩类型 {other}"))),
    };
    if out.len() != expected {
        return Err(bad(format!("块解压后长度 {} ≠ 预期 {expected}", out.len())));
    }
    if adler32(&out) != checksum {
        return Err(bad("块校验和不匹配".into()));
    }
    Ok(out)
}

/// `Encrypted & 2`：key block info 第 8 字节起用 ripemd128(校验和 + 0x3695) 派生的密钥做异或加密。
pub fn decrypt_key_block_info(block: &[u8]) -> Result<Vec<u8>> {
    if block.len() < 8 {
        return Err(Error::InvalidDict("key block info 过短".into()));
    }
    let mut seed = block[4..8].to_vec();
    seed.extend_from_slice(&0x3695u32.to_le_bytes());
    let key = Ripemd128::digest(&seed);
    let mut out = block[..8].to_vec();
    let mut prev = 0x36u8;
    for (i, &c) in block[8..].iter().enumerate() {
        let t = c.rotate_left(4) ^ prev ^ (i as u8) ^ key[i % key.len()];
        prev = c;
        out.push(t);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adler32_matches_known_values() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        let big = vec![0xffu8; 100_000];
        // 与 zlib.adler32(b"\xff" * 100000) 一致
        assert_eq!(adler32(&big), 0x149A_302C);
    }

    #[test]
    fn ripemd128_test_vectors() {
        let hex: String = Ripemd128::digest(b"abc").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "c14a12199c66e4ba84636b0f69144c77");
    }

    #[test]
    fn stored_block_round_trip_and_checksum() -> Result<()> {
        let mut raw = 0u32.to_le_bytes().to_vec();
        raw.extend_from_slice(&adler32(b"hello").to_be_bytes());
        raw.extend_from_slice(b"hello");
        assert_eq!(decompress_block(&raw, 5)?, b"hello");
        assert!(decompress_block(&raw, 4).is_err());
        raw[4] ^= 1;
        assert!(decompress_block(&raw, 5).is_err());
        Ok(())
    }
}
