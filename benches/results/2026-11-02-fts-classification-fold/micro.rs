use std::{hint::black_box,time::Instant};
#[inline(never)]
fn baseline<'a>(text: &'a str, scratch: &'a mut String) -> &'a str {
    if text.is_ascii() {
        // Fast path: check if already lowercase using byte-level SIMD detection
        // The .any() iterator is auto-vectorized by rustc for contiguous memory
        if !text.bytes().any(|b| b.is_ascii_uppercase()) {
            return text;
        }

        scratch.clear();
        // make_ascii_lowercase() uses single-instruction conversion on modern CPUs:
        // x86_64: AND with 0xDF to preserve case + OR with 0x20 unconditionally
        // ARM NEON: vbic/vorr pattern for selective bit manipulation
        scratch.reserve(text.len());
        scratch.push_str(text);
        scratch.make_ascii_lowercase();
        return scratch;
    }

    if !text.chars().any(|c| c.to_lowercase().next() != Some(c)) {
        return text;
    }
    scratch.clear();
    for c in text.chars() {
        scratch.extend(c.to_lowercase());
    }
    scratch
}
#[inline(never)]
fn candidate<'a>(text: &'a str, scratch: &'a mut String) -> &'a str {
    let class = text.bytes().fold(0u8, |flags, byte| {
        flags | (byte >> 7) | (u8::from(byte.is_ascii_uppercase()) << 1)
    });
    if class == 0 {
        return text;
    }
    if class == 2 {
        scratch.clear();
        // make_ascii_lowercase() uses single-instruction conversion on modern CPUs:
        // x86_64: AND with 0xDF to preserve case + OR with 0x20 unconditionally
        // ARM NEON: vbic/vorr pattern for selective bit manipulation
        scratch.reserve(text.len());
        scratch.push_str(text);
        scratch.make_ascii_lowercase();
        return scratch;
    }

    if !text.chars().any(|c| c.to_lowercase().next() != Some(c)) {
        return text;
    }
    scratch.clear();
    for c in text.chars() {
        scratch.extend(c.to_lowercase());
    }
    scratch
}

fn main(){let args:Vec<_>=std::env::args().collect();let mode=&args[1];let case=&args[2];let f:for<'a>fn(&'a str,&'a mut String)->&'a str=match mode.as_str(){"base"|"cont"=>baseline,"cand"=>candidate,_=>panic!()};let(text,calls)=match case.as_str(){"short_lower"=>("doc 99999 wal note".to_string(),2000000),"body_lower"=>("body 99999".to_string(),2000000),"long_lower"=>("lowercase words 123 ".repeat(64),100000),"short_upper"=>("DOC 99999 WAL NOTE".to_string(),2000000),"late_upper"=>(format!("{}Z","lower words ".repeat(64)),100000),"unicode_lower"=>("ёж é 🦔 ".repeat(16),100000),"unicode_upper"=>("ЁЖ İ ΣΟΣ ".repeat(16),100000),_=>panic!()};let mut scratch=String::new();let mut reference=String::new();for n in 0..=257{for marker in ["A","Z","@","[","İ","Σ","Ё","é","🦔"]{for pos in [0,n/2,n]{let input=format!("{}{}{}","x".repeat(pos),marker,"y".repeat(n-pos));assert_eq!(candidate(&input,&mut scratch),baseline(&input,&mut reference));}}}for code in 0..128{let input=char::from_u32(code).unwrap().to_string();assert_eq!(candidate(&input,&mut scratch),baseline(&input,&mut reference));}let expected:String=text.chars().flat_map(char::to_lowercase).collect();assert_eq!(f(&text,&mut scratch),expected);let start=Instant::now();for _ in 0..calls{black_box(f(black_box(&text),black_box(&mut scratch)));}let elapsed=start.elapsed().as_nanos();assert_eq!(f(&text,&mut scratch),expected);println!("{} {}",elapsed,calls);}
