//! PalmDOC 压缩（类 LZ77）

pub fn decompress_palmdoc(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut i = 0;
    while i < input.len() {
        let c = input[i];
        i += 1;
        match c {
            // 后面 c 个字节原样照抄
            1..=8 => {
                let end = (i + c as usize).min(input.len());
                out.extend_from_slice(&input[i..end]);
                i = end;
            }
            // 两字节：11 位距离 + 3 位长度，从已经解出的内容里复制（可能重叠）
            0x80..=0xBF => {
                let Some(&next) = input.get(i) else { break };
                i += 1;
                let v = (u16::from(c) << 8) | u16::from(next);
                let dist = usize::from((v >> 3) & 0x7FF);
                let len = usize::from(v & 7) + 3;
                if dist == 0 || dist > out.len() {
                    continue;
                }
                for _ in 0..len {
                    out.push(out[out.len() - dist]);
                }
            }
            // 空格 + 一个字符
            0xC0..=0xFF => {
                out.push(b' ');
                out.push(c ^ 0x80);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palmdoc_decompresses_known_sample() {
        // "ab" 字面量；0x01 后跟 1 个原样字节 0xF0；0xC8 = 空格 + 'H'；
        // 0x801A：(0x801A >> 3) & 0x7FF = 3 → 距离 3；(0x1A & 7) + 3 = 5 → 长度 5（可重叠复制）
        let input = [b'a', b'b', 0x01, 0xF0, 0xC8, 0x80, 0x1A];
        let out = decompress_palmdoc(&input);
        let mut want = b"ab\xF0 H".to_vec();
        for _ in 0..5 {
            let c = want[want.len() - 3];
            want.push(c);
        }
        assert_eq!(out, want);
    }
}
