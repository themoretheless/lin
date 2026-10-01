use super::GpuCompute;
use crate::Error;
use regex_automata::{
    nfa::thompson::{Config, NFA, State, WhichCaptures},
    util::syntax,
};

impl GpuCompute {
    /// NFA transitions run on GPU; assertion metadata uses the same look matcher
    /// and Unicode tables as Rust's regex engine.
    pub(crate) fn regex_mask(
        &self,
        texts: &[Option<&str>],
        pattern: &str,
        flags: &str,
    ) -> Result<Vec<bool>, Error> {
        let mut builder = regex::RegexBuilder::new(pattern);
        builder.case_insensitive(flags.contains('i'));
        if builder.build().is_err() {
            return Ok(vec![false; texts.len()]);
        }
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let nfa = NFA::compiler()
            .configure(Config::new().which_captures(WhichCaptures::None))
            .syntax(syntax::Config::new().case_insensitive(flags.contains('i')))
            .build(pattern)
            .map_err(|e| Error::runtime(format!("GPU regex compilation: {e}")))?;
        let states = nfa.states().len();
        let limits = self.device().limits();
        let max_words = (limits.max_storage_buffer_binding_size as usize)
            .min(limits.max_buffer_size as usize)
            / 4;
        let state_words = states
            .checked_mul(4)
            .and_then(|n| n.checked_add(2))
            .ok_or_else(|| Error::runtime("GPU regex state range overflow"))?;
        if state_words > max_words {
            return Err(Error::runtime("GPU regex program exceeds buffer limits"));
        }
        let scratch_words = states
            .checked_mul(3)
            .ok_or_else(|| Error::runtime("GPU regex scratch range overflow"))?;
        let row_limit = (max_words / scratch_words)
            .min(limits.max_compute_workgroups_per_dimension as usize * 64)
            .min(64);
        if row_limit == 0 {
            return Err(Error::runtime("GPU limits cannot hold regex scratch"));
        }
        let mut program = vec![0u32; state_words];
        program[0] = states as u32;
        program[1] = nfa.start_unanchored().as_usize() as u32;
        let mut looks = Vec::new();
        for (i, state) in nfa.states().iter().enumerate() {
            let mut transitions = Vec::new();
            let (kind, look) = match state {
                State::ByteRange { trans } => {
                    transitions.push([
                        trans.start as u32,
                        trans.end as u32,
                        trans.next.as_usize() as u32,
                    ]);
                    (0, 0)
                }
                State::Sparse(sparse) => {
                    transitions.extend(
                        sparse
                            .transitions
                            .iter()
                            .map(|t| [t.start as u32, t.end as u32, t.next.as_usize() as u32]),
                    );
                    (0, 0)
                }
                State::Dense(dense) => {
                    transitions.extend(
                        dense
                            .transitions
                            .iter()
                            .enumerate()
                            .filter(|(_, id)| id.as_usize() != 0)
                            .map(|(b, id)| [b as u32, b as u32, id.as_usize() as u32]),
                    );
                    (0, 0)
                }
                State::Look { look, next } => {
                    if !looks.contains(look) {
                        looks.push(*look);
                    }
                    transitions.push([0, 0, next.as_usize() as u32]);
                    (2, *look as u32)
                }
                State::Union { alternates } => {
                    transitions.extend(alternates.iter().map(|id| [0, 0, id.as_usize() as u32]));
                    (1, 0)
                }
                State::BinaryUnion { alt1, alt2 } => {
                    transitions.extend([
                        [0, 0, alt1.as_usize() as u32],
                        [0, 0, alt2.as_usize() as u32],
                    ]);
                    (1, 0)
                }
                State::Capture { next, .. } => {
                    transitions.push([0, 0, next.as_usize() as u32]);
                    (1, 0)
                }
                State::Match { .. } => (3, 0),
                State::Fail => (4, 0),
            };
            if transitions.len() > max_words.saturating_sub(program.len()) / 3 {
                return Err(Error::runtime("GPU regex program exceeds buffer limits"));
            }
            let header = 2 + i * 4;
            let transition_offset = program.len() as u32;
            program[header..header + 4].copy_from_slice(&[
                kind,
                transition_offset,
                transitions.len() as u32,
                look,
            ]);
            program.extend(transitions.into_iter().flatten());
        }
        let kernel = self.compile_wgsl(include_str!("shaders/regex.wgsl"), "main")?;
        let program = self.storage_buffer(bytemuck::cast_slice(&program))?;
        let mut result = Vec::with_capacity(texts.len());
        let mut position = 0;
        while position < texts.len() {
            let mut records = Vec::new();
            let mut data = Vec::new();
            while position < texts.len() && records.len() / 4 < row_limit {
                let text = texts[position];
                let bytes = text.unwrap_or("").as_bytes();
                let words = bytes
                    .len()
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(2))
                    .ok_or_else(|| Error::runtime("GPU regex text size overflow"))?;
                if words.checked_add(5).is_none_or(|n| n > max_words) {
                    return Err(Error::runtime("GPU regex text exceeds buffer limits"));
                }
                if !records.is_empty() && 1 + records.len() + 4 + data.len() + words > max_words {
                    break;
                }
                records.extend([
                    data.len() as u32,
                    bytes.len() as u32,
                    text.is_some() as u32,
                    0,
                ]);
                for at in 0..=bytes.len() {
                    let mut assertions = if text.is_some_and(|t| t.is_char_boundary(at)) {
                        1u32 << 31
                    } else {
                        0
                    };
                    for &look in &looks {
                        if nfa.look_matcher().matches(look, bytes, at) {
                            assertions |= look as u32;
                        }
                    }
                    data.extend([bytes.get(at).copied().unwrap_or(0) as u32, assertions]);
                }
                position += 1;
            }
            let count = records.len() / 4;
            let prefix = 1 + records.len();
            for record in records.chunks_exact_mut(4) {
                record[0] += prefix as u32;
            }
            let mut input = vec![count as u32];
            input.extend(records);
            input.extend(data);
            let input = self.storage_buffer(bytemuck::cast_slice(&input))?;
            let scratch = self.storage_buffer(&vec![0u8; count * scratch_words * 4])?;
            let output = self.storage_buffer(&vec![0u8; count * 4])?;
            let buffers = [&program, &input, &scratch, &output];
            let group = self.checked(|device, _| {
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("regex NFA"),
                    layout: &kernel.bind_group_layout(0),
                    entries: &entries,
                })
            })?;
            self.dispatch(&kernel, &[&group], [count.div_ceil(64) as u32, 1, 1])?;
            self.regex_dispatches
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            result.extend(
                self.read_buffer(&output)?
                    .chunks_exact(4)
                    .map(|b| u32::from_ne_bytes(b.try_into().unwrap()) != 0),
            );
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn regex_nfa_matches_rust_unicode_assertions_empty_and_flags() {
        let gpu = GpuCompute::new().unwrap();
        let texts = [
            None,
            Some(""),
            Some("a"),
            Some("ba"),
            Some("ababc"),
            Some("a\nb"),
            Some("a\r\nb"),
            Some("Привет мир"),
            Some("café"),
            Some("cafe\u{301}"),
            Some("Σςσ"),
            Some("İstanbul"),
            Some("😀a😀"),
            Some("foo_bar"),
            Some("123"),
            Some("a\0b"),
        ];
        for pattern in [
            "",
            "a",
            "^a$",
            "a|bc",
            "(ab)*c?",
            "(?:a?)*",
            "a{2,4}",
            "a.*b",
            "(?s)a.*b",
            "(?m)^b$",
            "(?mR)^a$",
            "\\A.*\\z",
            "\\b\\w+\\b",
            "\\B",
            "(?-u:\\b)[a-z]+",
            "(?-u:\\B)",
            "\\p{Greek}+",
            "[[:alpha:]]+",
            "\\d+",
            "[^a]+",
            "😀+",
            "Привет",
            "Σ",
            "i",
            "a\\x00b",
            "\\b{start}a",
            "a\\b{end}",
            "\\b{start-half}a",
            "a\\b{end-half}",
        ] {
            for flags in ["", "i"] {
                let mut builder = regex::RegexBuilder::new(pattern);
                builder.case_insensitive(flags.contains('i'));
                let reference = builder.build().unwrap();
                let expected: Vec<bool> = texts
                    .iter()
                    .map(|text| text.is_some_and(|t| reference.is_match(t)))
                    .collect();
                assert_eq!(
                    gpu.regex_mask(&texts, pattern, flags).unwrap(),
                    expected,
                    "pattern {pattern:?}, flags {flags}"
                );
            }
        }
        assert!(gpu.regex_dispatches() > 0);
        let before = gpu.regex_dispatches();
        assert_eq!(
            gpu.regex_mask(&texts, "[invalid", "").unwrap(),
            vec![false; texts.len()]
        );
        assert_eq!(gpu.regex_dispatches(), before);
    }
    #[test]
    #[ignore = "requires a hardware GPU"]
    fn regex_limits_are_errors_without_cpu_matching_fallback() {
        let mut options = super::super::GpuOptions::default();
        options.limits.max_storage_buffer_binding_size = 1024;
        let gpu = GpuCompute::with_options(options).unwrap();
        let huge_text = "a".repeat(1000);
        assert!(
            gpu.regex_mask(&[Some(&huge_text)], "a", "")
                .unwrap_err()
                .to_string()
                .contains("text exceeds buffer limits")
        );
        assert!(
            gpu.regex_mask(&[Some("a")], "a{1000}", "")
                .unwrap_err()
                .to_string()
                .contains("program exceeds buffer limits")
        );
        assert_eq!(gpu.regex_dispatches(), 0);
        let texts = [Some("aaa"), None, Some("b"), Some("ba")];
        assert_eq!(
            gpu.regex_mask(&texts, "a", "").unwrap(),
            [true, false, false, true]
        );
    }
}
