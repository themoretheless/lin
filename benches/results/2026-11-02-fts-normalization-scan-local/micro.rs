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
    if !text.bytes().any(|b| b >= 128 || b.is_ascii_uppercase()) {
        return text;
    }
    if text.is_ascii() {
        scratch.clear();
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
fn main(){let args:Vec<_>=std::env::args().collect();let f:for<'a>fn(&'a str,&'a mut String)->&'a str=if args[1]=="cand"{candidate}else{baseline};let text=match args[2].as_str(){"short_lower"=>"doc 99999 wal note".to_string(),"body_lower"=>"body 99999".to_string(),"long_lower"=>"lowercase words 123 ".repeat(64),"short_upper"=>"DOC 99999 WAL NOTE".to_string(),"late_upper"=>format!("{}Z","lower words ".repeat(64)),"unicode_lower"=>"ёж é 🦔 ".repeat(16),"unicode_upper"=>"ЁЖ İ ΣΟΣ ".repeat(16),_=>panic!()};let expected:String=text.chars().flat_map(char::to_lowercase).collect();let mut scratch=String::new();assert_eq!(f(&text,&mut scratch),expected);let start=Instant::now();for _ in 0..200000{black_box(f(black_box(&text),black_box(&mut scratch)));}let elapsed=start.elapsed().as_nanos();assert_eq!(f(&text,&mut scratch),expected);println!("{}",elapsed);}
