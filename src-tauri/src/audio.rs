//! 词典发音转码：Ogg Speex（.spx，牛津等 mdd 常用）→ 16 位 PCM WAV。
//! WebView2 不支持 Speex，lens:// 协议遇到 .spx 时调用这里。
//! 解码用随仓库编译的官方 libspeex（build.rs），Ogg 解析用纯 Rust 的 `ogg`。

use std::io::Cursor;
use std::os::raw::{c_char, c_int, c_void};

use crate::error::{Error, Result};

#[repr(C)]
struct SpeexBits {
    chars: *mut c_char,
    nb_bits: c_int,
    char_ptr: c_int,
    bit_ptr: c_int,
    owner: c_int,
    overflow: c_int,
    buf_size: c_int,
    reserved1: c_int,
    reserved2: *mut c_void,
}

const SPEEX_SET_ENH: c_int = 0;
const SPEEX_GET_FRAME_SIZE: c_int = 3;

extern "C" {
    fn speex_lib_get_mode(mode: c_int) -> *const c_void;
    fn speex_decoder_init(mode: *const c_void) -> *mut c_void;
    fn speex_decoder_destroy(state: *mut c_void);
    fn speex_decoder_ctl(state: *mut c_void, request: c_int, ptr: *mut c_void) -> c_int;
    fn speex_decode_int(state: *mut c_void, bits: *mut SpeexBits, out: *mut i16) -> c_int;
    fn speex_bits_init(bits: *mut SpeexBits);
    fn speex_bits_read_from(bits: *mut SpeexBits, bytes: *const c_char, len: c_int);
    fn speex_bits_remaining(bits: *mut SpeexBits) -> c_int;
    fn speex_bits_destroy(bits: *mut SpeexBits);
}

/// Speex 流头（80 字节，小端）里用得到的字段。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Header {
    rate: u32,
    mode: i32,
    channels: i32,
    frames_per_packet: i32,
    extra_headers: i32,
}

fn parse_header(p: &[u8]) -> Result<Header> {
    let bad = || Error::InvalidDict("不是有效的 Speex 流头".into());
    if p.len() < 80 || &p[..8] != b"Speex   " {
        return Err(bad());
    }
    let i32_at = |o: usize| i32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
    let h = Header {
        rate: u32::try_from(i32_at(36)).map_err(|_| bad())?,
        mode: i32_at(40),
        channels: i32_at(48),
        frames_per_packet: i32_at(64).max(1),
        extra_headers: i32_at(68).max(0),
    };
    if !(0..=2).contains(&h.mode) || h.rate == 0 || h.rate > 48_000 {
        return Err(bad());
    }
    Ok(h)
}

/// 解码器 + 位流，Drop 时释放 C 端资源。
struct Decoder {
    state: *mut c_void,
    bits: SpeexBits,
    frame_size: usize,
}

impl Decoder {
    fn new(mode: i32) -> Result<Self> {
        // SAFETY: mode 已校验在 0..=2，speex_lib_get_mode 返回静态模式表指针。
        let state = unsafe {
            let m = speex_lib_get_mode(mode);
            if m.is_null() {
                return Err(Error::InvalidDict("未知的 Speex 模式".into()));
            }
            speex_decoder_init(m)
        };
        if state.is_null() {
            return Err(Error::Other("Speex 解码器初始化失败".into()));
        }
        let mut d = Self {
            state,
            bits: SpeexBits {
                chars: std::ptr::null_mut(),
                nb_bits: 0,
                char_ptr: 0,
                bit_ptr: 0,
                owner: 0,
                overflow: 0,
                buf_size: 0,
                reserved1: 0,
                reserved2: std::ptr::null_mut(),
            },
            frame_size: 0,
        };
        let mut frame: c_int = 0;
        let mut enh: c_int = 1;
        // SAFETY: state 有效；ctl 写入一个 c_int。bits 由 speex_bits_init 接管初始化。
        unsafe {
            speex_bits_init(&mut d.bits);
            speex_decoder_ctl(d.state, SPEEX_GET_FRAME_SIZE, (&mut frame as *mut c_int).cast());
            speex_decoder_ctl(d.state, SPEEX_SET_ENH, (&mut enh as *mut c_int).cast());
        }
        d.frame_size = usize::try_from(frame).ok().filter(|&n| n > 0 && n <= 2048).ok_or_else(|| {
            Error::Other("Speex 帧长无效".into())
        })?;
        Ok(d)
    }

    /// 解一个 Ogg 包（含 frames_per_packet 帧），样本追加到 out。
    fn decode_packet(&mut self, packet: &[u8], frames: i32, out: &mut Vec<i16>) -> Result<()> {
        let len = c_int::try_from(packet.len()).map_err(|_| Error::InvalidDict("Speex 包过大".into()))?;
        let mut buf = vec![0i16; self.frame_size];
        // SAFETY: read_from 复制 packet 内容到 bits 自有缓冲；decode_int 最多写 frame_size 个样本到 buf。
        unsafe {
            speex_bits_read_from(&mut self.bits, packet.as_ptr().cast(), len);
            for _ in 0..frames {
                match speex_decode_int(self.state, &mut self.bits, buf.as_mut_ptr()) {
                    0 => out.extend_from_slice(&buf),
                    -1 => break, // 流结束标记
                    _ => return Err(Error::InvalidDict("Speex 数据损坏".into())),
                }
                if speex_bits_remaining(&mut self.bits) < 0 {
                    break;
                }
            }
        }
        Ok(())
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: state/bits 由 new 创建，只释放一次。
        unsafe {
            speex_bits_destroy(&mut self.bits);
            speex_decoder_destroy(self.state);
        }
    }
}

/// 把 Ogg Speex 解码为单声道 PCM，返回 (采样率, 样本)。
pub fn decode_ogg_speex(data: &[u8]) -> Result<(u32, Vec<i16>)> {
    let mut reader = ogg::PacketReader::new(Cursor::new(data));
    let next = |r: &mut ogg::PacketReader<Cursor<&[u8]>>| {
        r.read_packet().map_err(|e| Error::InvalidDict(format!("Ogg 解析失败：{e}")))
    };
    let first = next(&mut reader)?.ok_or_else(|| Error::InvalidDict("空的 Ogg 流".into()))?;
    let header = parse_header(&first.data)?;
    if header.channels > 1 {
        tracing::debug!("Speex 立体声按单声道解码");
    }
    // 跳过注释包和额外头
    for _ in 0..1 + header.extra_headers {
        next(&mut reader)?;
    }

    let mut decoder = Decoder::new(header.mode)?;
    let mut samples = Vec::new();
    let mut last_granule = None;
    while let Some(p) = next(&mut reader)? {
        if first.stream_serial() != p.stream_serial() {
            continue;
        }
        decoder.decode_packet(&p.data, header.frames_per_packet, &mut samples)?;
        last_granule = Some(p.absgp_page());
    }
    // 最后一页的 granule position 是总样本数，截掉末帧的补零
    if let Some(g) = last_granule.and_then(|g| usize::try_from(g).ok()) {
        if g > 0 && g < samples.len() {
            samples.truncate(g);
        }
    }
    Ok((header.rate, samples))
}

/// 16 位单声道 PCM → WAV 文件字节。
pub fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// `.spx` → WAV。
pub fn spx_to_wav(data: &[u8]) -> Result<Vec<u8>> {
    let (rate, samples) = decode_ogg_speex(data)?;
    Ok(wav(rate, &samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" {
        fn speex_encoder_init(mode: *const c_void) -> *mut c_void;
        fn speex_encoder_destroy(state: *mut c_void);
        fn speex_encoder_ctl(state: *mut c_void, request: c_int, ptr: *mut c_void) -> c_int;
        fn speex_encode_int(state: *mut c_void, input: *mut i16, bits: *mut SpeexBits) -> c_int;
        fn speex_bits_reset(bits: *mut SpeexBits);
        fn speex_bits_write(bits: *mut SpeexBits, bytes: *mut c_char, max_len: c_int) -> c_int;
    }

    fn header_bytes(rate: u32, mode: i32, frame_size: i32) -> Vec<u8> {
        let mut h = b"Speex   ".to_vec();
        h.extend_from_slice(&[0u8; 20]); // speex_version
        for v in [1i32, 80, rate as i32, mode, 4, 1, -1, frame_size, 0, 1, 0, 0, 0] {
            h.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(h.len(), 80);
        h
    }

    /// 用 libspeex 编码一段 440Hz 正弦，封装成 Ogg Speex。
    fn make_spx(rate: u32, mode: i32, seconds: f32) -> (Vec<u8>, usize) {
        let mut ogg_out = Vec::new();
        let total;
        // SAFETY: 测试内按 libspeex 文档顺序调用编码 API。
        unsafe {
            let enc = speex_encoder_init(speex_lib_get_mode(mode));
            let mut frame: c_int = 0;
            speex_encoder_ctl(enc, SPEEX_GET_FRAME_SIZE, (&mut frame as *mut c_int).cast());
            let mut q: c_int = 8;
            speex_encoder_ctl(enc, 4, (&mut q as *mut c_int).cast()); // SPEEX_SET_QUALITY
            let mut bits: SpeexBits = std::mem::zeroed();
            speex_bits_init(&mut bits);

            let n = (rate as f32 * seconds) as usize;
            total = n;
            let pcm: Vec<i16> = (0..n)
                .map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16)
                .collect();
            let mut w = ogg::PacketWriter::new(&mut ogg_out);
            w.write_packet(header_bytes(rate, mode, frame), 7, ogg::PacketWriteEndInfo::EndPage, 0).unwrap();
            w.write_packet(vec![0u8; 8], 7, ogg::PacketWriteEndInfo::EndPage, 0).unwrap();
            let frames: Vec<&[i16]> = pcm.chunks(frame as usize).collect();
            for (i, chunk) in frames.iter().enumerate() {
                let mut buf = vec![0i16; frame as usize];
                buf[..chunk.len()].copy_from_slice(chunk);
                speex_bits_reset(&mut bits);
                speex_encode_int(enc, buf.as_mut_ptr(), &mut bits);
                let mut bytes = vec![0 as c_char; 2000];
                let len = speex_bits_write(&mut bits, bytes.as_mut_ptr(), 2000);
                let data: Vec<u8> = bytes[..len as usize].iter().map(|&b| b as u8).collect();
                let last = i + 1 == frames.len();
                let gp = if last { n as u64 } else { ((i + 1) * frame as usize) as u64 };
                let end = if last { ogg::PacketWriteEndInfo::EndStream } else { ogg::PacketWriteEndInfo::NormalPacket };
                w.write_packet(data, 7, end, gp).unwrap();
            }
            speex_bits_destroy(&mut bits);
            speex_encoder_destroy(enc);
        }
        (ogg_out, total)
    }

    fn rms(s: &[i16]) -> f64 {
        (s.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / s.len().max(1) as f64).sqrt()
    }

    #[test]
    fn round_trip_narrowband_and_wideband() -> Result<()> {
        for (rate, mode) in [(8000, 0), (16000, 1)] {
            let (spx, n) = make_spx(rate, mode, 0.75);
            let (r, pcm) = decode_ogg_speex(&spx)?;
            assert_eq!(r, rate);
            assert_eq!(pcm.len(), n, "按 granule position 截断到原始长度");
            // 正弦幅度 8000 → RMS ≈ 5657；编解码后能量应大致保持
            let e = rms(&pcm[pcm.len() / 4..]);
            assert!((3500.0..7500.0).contains(&e), "mode {mode} rms {e}");
        }
        Ok(())
    }

    #[test]
    fn wav_header_is_well_formed() -> Result<()> {
        let (spx, n) = make_spx(16000, 1, 0.2);
        let w = spx_to_wav(&spx)?;
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 16000);
        assert_eq!(w.len(), 44 + n * 2);
        Ok(())
    }

    #[test]
    fn rejects_non_speex() {
        assert!(spx_to_wav(b"ID3 not ogg").is_err());
        let mut fake = Vec::new();
        ogg::PacketWriter::new(&mut fake)
            .write_packet(b"OpusHead........".to_vec(), 1, ogg::PacketWriteEndInfo::EndStream, 0)
            .unwrap();
        assert!(spx_to_wav(&fake).is_err());
    }
}
